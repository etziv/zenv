//! AI routers (OmniRoute, 9router) from the npm registry. The package
//! tarball is checked against the registry's sha512 integrity, then `npm
//! install` (with a supported Node.js) puts it and its dependencies in the
//! version folder; npm checks every dependency's integrity the same way.
//!
//! A router pulls in up to ~1,200 packages, and one hijacked package would
//! run with access to the user's AI keys. So:
//! - Dependencies resolve as they were when the release was published
//!   (`npm install --before=<publish time>`): a package version published
//!   later, hijacked or not, never gets in.
//! - Install scripts never run, except those of the few packages the router
//!   names (`npm rebuild <names>`, which npm 10 and 11 both honour).
//! - Auto-update only takes releases at least [`SETTLE`] old; hijacked
//!   releases are usually found and pulled from the registry by then.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha2::Digest;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::ai_router::RouterSpec;
use crate::error::{AppError, AppResult, IoContext};
use crate::runtime_catalog::ComponentId;

pub struct NpmToolProvider(pub &'static RouterSpec);

pub static OMNIROUTE: NpmToolProvider = NpmToolProvider(&crate::ai_router::OMNIROUTE);
pub static NINEROUTER: NpmToolProvider = NpmToolProvider(&crate::ai_router::NINEROUTER);

/// How old a release must be before auto-update installs it.
pub const SETTLE: time::Duration = time::Duration::days(7);

#[derive(Deserialize)]
struct Packument {
    #[serde(rename = "dist-tags")]
    dist_tags: std::collections::HashMap<String, String>,
    versions: std::collections::HashMap<String, Manifest>,
    /// Publish time of each version (RFC 3339).
    #[serde(default)]
    time: std::collections::HashMap<String, String>,
}

impl Packument {
    fn published(&self, version: &str) -> Option<OffsetDateTime> {
        self.time.get(version).and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
    }
}

/// A release and when it was published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub published: OffsetDateTime,
}

#[derive(Deserialize)]
struct Manifest {
    dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
    tarball: String,
    integrity: Option<String>,
}

fn key(v: &str) -> Vec<u32> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// The full document: the abbreviated one has no publish times.
fn packument(http: &ureq::Agent, package: &str) -> AppResult<Packument> {
    http.get(format!("https://registry.npmjs.org/{package}"))
        .header("Accept", "application/json")
        .call()
        .map_err(|e| AppError::Platform(format!("cannot reach the npm registry: {e}")))?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_json()
        .map_err(|e| AppError::Platform(format!("unexpected npm registry data: {e}")))
}

/// Every release (no pre-releases, nothing past `latest`) with a known
/// publish time, newest first.
fn all_releases(p: &Packument) -> Vec<Release> {
    let latest = p.dist_tags.get("latest").map(|v| key(v));
    let mut list: Vec<Release> = p
        .versions
        .keys()
        .filter(|v| !v.contains('-') && latest.as_ref().is_none_or(|l| key(v) <= *l))
        .filter_map(|v| Some(Release { version: v.clone(), published: p.published(v)? }))
        .collect();
    list.sort_by_key(|r| std::cmp::Reverse(key(&r.version)));
    list
}

/// The newest release published at least [`SETTLE`] before `now`.
pub fn newest_settled(releases: &[Release], now: OffsetDateTime) -> Option<&Release> {
    releases.iter().find(|r| r.published + SETTLE <= now)
}

/// The newest `count` releases, plus the newest settled one when it is older
/// (auto-update installs that one).
fn offered(p: &Packument, count: usize, now: OffsetDateTime) -> Vec<String> {
    let all = all_releases(p);
    let mut list: Vec<String> = all.iter().take(count).map(|r| r.version.clone()).collect();
    if let Some(settled) = newest_settled(&all, now).filter(|s| !list.contains(&s.version)) {
        list.push(settled.version.clone());
    }
    list
}

/// A router's releases with their publish times, newest first.
pub fn releases(spec: &RouterSpec) -> AppResult<Vec<Release>> {
    Ok(all_releases(&packument(&super::http_agent(), spec.package)?))
}

/// `sha512-<base64>` of a file.
fn sha512_integrity(path: &Path) -> AppResult<String> {
    let mut f = std::fs::File::open(path).ctx(|| format!("opening {}", path.display()))?;
    let mut h = sha2::Sha512::new();
    std::io::copy(&mut f, &mut h).ctx(|| format!("reading {}", path.display()))?;
    Ok(format!("sha512-{}", crate::thumbnails::base64(&h.finalize())))
}

impl InstallProvider for NpmToolProvider {
    fn component(&self) -> ComponentId {
        self.0.component
    }
    fn source(&self) -> &'static str {
        "registry.npmjs.org"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["registry.npmjs.org"]
    }
    fn requires_checksum(&self) -> bool {
        false
    }
    fn verification(&self) -> &'static str {
        "the npm registry's sha512 integrity (package and every dependency)"
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let spec = self.0;
        let p = packument(http, spec.package)?;
        Ok(offered(&p, 6, OffsetDateTime::now_utc())
            .into_iter()
            .filter_map(|v| {
                let m = p.versions.get(&v)?;
                Some(AvailableVersion {
                    component: spec.component,
                    filename: format!("{}-{v}.tgz", spec.package),
                    url: m.dist.tarball.clone(),
                    version: v,
                    stable: true,
                    sha256: String::new(),
                    size: 0,
                    source: "registry.npmjs.org",
                    installed_id: None,
                    verified_by: "",
                })
            })
            .collect())
    }

    fn unpack(&self) -> Unpack {
        Unpack::Custom
    }

    fn custom_install(&self, download: &Path, target: &Path, offer: &AvailableVersion) -> AppResult<()> {
        let spec = self.0;
        // 1. The tarball must match the registry's integrity for this version.
        let http = super::http_agent();
        let p = packument(&http, spec.package)?;
        let expected = p
            .versions
            .get(&offer.version)
            .and_then(|m| m.dist.integrity.clone())
            .ok_or_else(|| AppError::Validation(format!("the registry lists no integrity for {} {}", spec.package, offer.version)))?;
        if sha512_integrity(download)? != expected {
            return Err(AppError::Validation(format!("{} {} does not match the registry's sha512; the download was discarded", spec.package, offer.version)));
        }
        let published = p
            .published(&offer.version)
            .and_then(|t| t.format(&Rfc3339).ok())
            .ok_or_else(|| AppError::Validation(format!("the registry lists no publish time for {} {}", spec.package, offer.version)))?;
        // 2. npm installs it (and checks every dependency) with a supported
        //    Node.js, with dependencies as of the release and no scripts.
        let bin = target.parent().and_then(Path::parent).unwrap_or(target);
        let node = crate::ai_router::find_node(bin, spec.node_supported)
            .ok_or_else(|| AppError::Conflict(format!("{} needs {}; install Node.js in Languages first", spec.name, spec.node_requirement)))?;
        let node_dir = node.parent().unwrap_or(bin);
        let npm = node_dir.join("npm.cmd");
        if !npm.is_file() {
            return Err(AppError::invalid_path(&npm, "npm was not found next to node.exe"));
        }
        // npm reads local tarballs by their extension.
        let tgz = target.join(format!("{}-{}.tgz", spec.package, offer.version));
        std::fs::copy(download, &tgz).ctx(|| format!("preparing {}", tgz.display()))?;
        let prefix = target.display().to_string();
        let mut path = vec![node_dir.to_path_buf()];
        if let Some(p) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&p));
        }
        let path = std::env::join_paths(path).unwrap_or_default();
        let npm_run = |what: &str, args: Vec<String>| -> AppResult<()> {
            let out = run_with_path(&npm, &args, &path, spec.install_timeout)?;
            if out.code != Some(0) {
                let tail: Vec<&str> = out.text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
                return Err(AppError::Platform(format!("npm {what} failed:\n{}", tail.join("\n"))));
            }
            Ok(())
        };
        let installed = npm_run("install", install_args(&prefix, &published, &tgz.display().to_string()));
        let _ = std::fs::remove_file(&tgz);
        installed?;
        // 3. Only the install scripts the router needs.
        if let Some(args) = rebuild_args(&prefix, spec.allow_scripts) {
            npm_run("rebuild", args)?;
        }
        if !target.join(spec.entry).is_file() {
            return Err(AppError::Platform(format!("npm installed {} but {} is missing", spec.package, spec.entry)));
        }
        crate::ai_router::record_node(target, &node)
    }
}

fn install_args(prefix: &str, published: &str, tarball: &str) -> Vec<String> {
    ["install", "--prefix", prefix, "--no-audit", "--no-fund", "--loglevel=error", "--ignore-scripts"]
        .into_iter()
        .map(String::from)
        .chain([format!("--before={published}"), tarball.to_string()])
        .collect()
}

/// `npm rebuild` runs the install scripts of the named packages only.
fn rebuild_args(prefix: &str, allow_scripts: &str) -> Option<Vec<String>> {
    let names: Vec<String> = allow_scripts.split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect();
    if names.is_empty() {
        return None;
    }
    Some(["rebuild", "--prefix", prefix, "--loglevel=error"].into_iter().map(String::from).chain(names).collect())
}

/// Runs `program` with `PATH` set, capturing output.
fn run_with_path(program: &Path, args: &[String], path: &std::ffi::OsStr, timeout: Duration) -> AppResult<crate::runtime_manager::probe::CapturedOutput> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args).env("PATH", path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::platform::hide_console_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot run {}: {e}", program.display())))?;
    let mut so = child.stdout.take().expect("piped");
    let mut se = child.stderr.take().expect("piped");
    let t1 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let t2 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let deadline = std::time::Instant::now() + timeout;
    let code = loop {
        if let Some(status) = child.try_wait().map_err(|e| AppError::Platform(e.to_string()))? {
            break status.code();
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err(AppError::Platform(format!("{} did not finish within {} minutes", program.display(), timeout.as_secs() / 60)));
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let text = format!("{}{}", t1.join().unwrap_or_default(), t2.join().unwrap_or_default());
    Ok(crate::runtime_manager::probe::CapturedOutput { code, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: &str) -> OffsetDateTime {
        OffsetDateTime::parse(t, &Rfc3339).unwrap()
    }

    const PACKUMENT: &str = r#"{"dist-tags":{"latest":"3.8.52","next":"3.9.0-rc.1"},"versions":{
        "3.8.9":{"dist":{"tarball":"t"}},"3.8.52":{"dist":{"tarball":"t"}},"3.8.10":{"dist":{"tarball":"t"}},
        "3.9.0-rc.1":{"dist":{"tarball":"t"}},"3.8.53":{"dist":{"tarball":"t"}}},
        "time":{"created":"2026-01-01T00:00:00.000Z","3.8.9":"2026-10-01T10:00:00.000Z","3.8.10":"2026-10-12T10:00:00.000Z",
        "3.8.52":"2026-10-19T22:40:03.431Z","3.8.53":"2026-10-20T00:00:00.000Z","3.9.0-rc.1":"2026-10-20T00:00:00.000Z"}}"#;

    #[test]
    fn newest_releases_without_pre_releases() {
        let p: Packument = serde_json::from_str(PACKUMENT).unwrap();
        let versions: Vec<String> = all_releases(&p).into_iter().map(|r| r.version).collect();
        assert_eq!(versions, vec!["3.8.52", "3.8.10", "3.8.9"], "nothing past latest, no pre-releases");
    }

    #[test]
    fn auto_update_waits_until_a_release_has_settled() {
        let p: Packument = serde_json::from_str(PACKUMENT).unwrap();
        let all = all_releases(&p);
        assert_eq!(newest_settled(&all, at("2026-10-20T00:00:00Z")).unwrap().version, "3.8.10", "3.8.52 is a day old");
        assert_eq!(newest_settled(&all, at("2026-10-26T22:40:04Z")).unwrap().version, "3.8.52");
        assert!(newest_settled(&all, at("2026-10-02T00:00:00Z")).is_none());
        // Offered for install even when newer releases push it out of the list.
        assert_eq!(offered(&p, 1, at("2026-10-20T00:00:00Z")), vec!["3.8.52", "3.8.10"]);
        assert_eq!(offered(&p, 2, at("2026-10-20T00:00:00Z")), vec!["3.8.52", "3.8.10"]);
    }

    #[test]
    fn npm_runs_no_scripts_and_resolves_as_of_the_release() {
        let args = install_args("C:/zenv/bin/omniroute/3.8.52", "2026-10-19T22:40:03.431Z", "x.tgz");
        assert!(args.contains(&"--ignore-scripts".to_string()));
        assert!(args.contains(&"--before=2026-10-19T22:40:03.431Z".to_string()));
        assert_eq!(args.last().unwrap(), "x.tgz");
        let rebuild = rebuild_args("p", "omniroute, keytar,@parcel/watcher").unwrap();
        assert_eq!(rebuild[..4], ["rebuild", "--prefix", "p", "--loglevel=error"]);
        assert_eq!(rebuild[4..], ["omniroute", "keytar", "@parcel/watcher"]);
        assert!(rebuild_args("p", "").is_none());
    }

    #[test]
    fn integrity_matches_npm_format() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x");
        std::fs::write(&f, "hello").unwrap();
        // echo -n hello | openssl dgst -sha512 -binary | base64
        assert_eq!(sha512_integrity(&f).unwrap(), "sha512-m3HSJL1i83hdltRq0+o9czGb+8KJDKra4t/3JRlnPKcjI8PZm6XBHXx6zG4UuMXaDEZjR1wuXDre9G9zvN7AQw==");
    }
}
