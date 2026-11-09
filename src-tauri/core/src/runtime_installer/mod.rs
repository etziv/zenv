//! Installs component versions from their official sources into the Zenv
//! home (`<home>\bin\<component>\<version>`).
//!
//! Every provider lists versions from verified metadata (with checksums), and
//! every install goes: download (HTTPS, allow-listed hosts) → SHA-256 check →
//! extract into a staging folder (zip-slip safe) → atomic rename → run the
//! executable's own version command → register as a *managed* installation.
//! Nothing is reported as installed before that last check succeeds.

pub mod github;
pub mod databases;
pub mod go;
pub mod node;
pub mod npm_tool;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod webservers;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use sha2::{Digest, Sha256};
use ureq::ResponseExt;

use crate::error::{AppError, AppResult, IoContext};
use crate::paths::AppPaths;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, probe, InstallSource, Installation};
use crate::storage::Db;
use crate::util::{new_id, now_rfc3339};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableVersion {
    pub component: ComponentId,
    pub version: String,
    pub stable: bool,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// Where the metadata came from, e.g. "go.dev".
    pub source: &'static str,
    /// Set when this exact version is already installed by Zenv.
    pub installed_id: Option<String>,
    /// How the download is checked, e.g. "SHA-256 from go.dev" (set by `available`).
    pub verified_by: &'static str,
}

/// How a downloaded file becomes an installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unpack {
    /// A zip; `root` is a leading folder to strip (`None`: strip a single
    /// top-level folder if that is all the archive contains).
    Zip { root: Option<&'static str> },
    /// A zip whose `root` folder becomes the installation, leaving out the
    /// listed folders inside it (bundled tools Zenv does not use).
    ZipSkipping { root: &'static str, skip: &'static [&'static str] },
    /// A 7z archive with the same single-folder rule.
    SevenZip,
    /// The download is an installer the provider runs itself.
    Custom,
}

/// One installation source. Providers differ in metadata format, archive
/// layout and post-install steps; there is no universal installer.
pub trait InstallProvider: Send + Sync {
    fn component(&self) -> ComponentId;
    fn source(&self) -> &'static str;
    /// HTTPS hosts downloads may come from (redirects included).
    fn allowed_hosts(&self) -> &'static [&'static str];
    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>>;

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: None }
    }

    /// Whether every offer must carry a SHA-256 to check the download against.
    /// Sources without one verify the installed files instead (`post_install`).
    fn requires_checksum(&self) -> bool {
        true
    }

    /// How downloads from this source are checked, shown when installing.
    fn verification(&self) -> &'static str {
        "SHA-256 published by the source"
    }

    /// For `Unpack::Custom`: install from the verified download into `target`.
    fn custom_install(&self, _download: &Path, _target: &Path, _offer: &AvailableVersion) -> AppResult<()> {
        Err(AppError::Unsupported("this provider installs from an archive".into()))
    }

    /// Prepares a fresh install (e.g. a php.ini); only runs on folders Zenv just created.
    fn post_install(&self, _target: &Path) -> AppResult<()> {
        Ok(())
    }

    /// Finds the executable to register and probe.
    fn locate(&self, target: &Path) -> AppResult<probe::Located> {
        probe::locate(self.component(), target)
    }
}

/// Every `.exe` and `.dll` under `dir`.
pub(crate) fn programs_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe") || x.eq_ignore_ascii_case("dll")) {
                out.push(p);
            }
        }
    }
    out
}

/// Each file must carry `signer`'s code signature, valid for its exact
/// content: an independent check next to a checksum from the download
/// server, since the signing key is not on that server.
pub(crate) fn require_signer(files: &[PathBuf], signer: &str) -> AppResult<()> {
    if files.is_empty() {
        return Err(AppError::Validation(format!("no program signed by {signer} was found; the install was discarded")));
    }
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match crate::platform::authenticode_signer(f)? {
            Some(s) if s == signer => {}
            Some(s) => return Err(AppError::Validation(format!("{name} is signed by {s}, not {signer}; the install was discarded"))),
            None => return Err(AppError::Validation(format!("{name} is not signed by {signer}; the install was discarded"))),
        }
    }
    Ok(())
}

pub fn provider_for(component: ComponentId) -> Option<&'static dyn InstallProvider> {
    match component {
        ComponentId::Go => Some(&go::GoProvider),
        ComponentId::Php => Some(&php::PhpProvider),
        ComponentId::Python => Some(&python::PythonProvider),
        ComponentId::Node => Some(&node::NodeProvider),
        ComponentId::Ruby => Some(&ruby::RubyProvider),
        ComponentId::Rust => Some(&rust::RustProvider),
        ComponentId::Apache => Some(&webservers::ApacheProvider),
        ComponentId::Nginx => Some(&webservers::NginxProvider),
        ComponentId::Caddy => Some(&webservers::CaddyProvider),
        ComponentId::Frankenphp => Some(&webservers::FrankenphpProvider),
        ComponentId::Mysql => Some(&databases::MysqlProvider),
        ComponentId::Postgresql => Some(&databases::PostgresqlProvider),
        ComponentId::Omniroute => Some(&npm_tool::OMNIROUTE),
        ComponentId::Ninerouter => Some(&npm_tool::NINEROUTER),
        _ => None,
    }
}

pub fn installable_components() -> Vec<ComponentId> {
    ComponentId::ALL.into_iter().filter(|c| provider_for(*c).is_some()).collect()
}

pub fn http_agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    // Windows' own TLS stack and certificate store (schannel), like the browser.
    ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build())
        .user_agent(concat!("zenv/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Downloading,
    Verifying,
    Extracting,
    Validating,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallJob {
    pub id: String,
    pub component: ComponentId,
    pub version: String,
    pub phase: Phase,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub installation_id: Option<String>,
    pub started_at: String,
}

pub struct Installer {
    db: Arc<Db>,
    paths: AppPaths,
    jobs: Arc<Mutex<HashMap<String, InstallJob>>>,
}

fn host_of(url: &str) -> Option<String> {
    url.strip_prefix("https://")?.split(['/', '?', '#']).next().map(|h| h.to_ascii_lowercase())
}

/// Streams `url` to `dest`, hashing as it goes. Returns the SHA-256 (hex).
pub fn download(
    http: &ureq::Agent,
    url: &str,
    allowed: &[&str],
    dest: &Path,
    mut progress: impl FnMut(u64, u64),
) -> AppResult<String> {
    let host = host_of(url).ok_or_else(|| AppError::Validation(format!("only HTTPS downloads are allowed: {url}")))?;
    if !allowed.contains(&host.as_str()) {
        return Err(AppError::Validation(format!("{host} is not an allowed download source")));
    }
    let mut resp = http.get(url).call().map_err(|e| AppError::Platform(format!("download failed: {e}")))?;
    // The final URL after redirects must also be an allowed host.
    if let Some(final_host) = resp.get_uri().host() {
        if !allowed.contains(&final_host.to_ascii_lowercase().as_str()) {
            return Err(AppError::Validation(format!("download was redirected to {final_host}, which is not allowed")));
        }
    }
    let total = resp.body().content_length().unwrap_or(0);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
    }
    let mut file = std::fs::File::create(dest).ctx(|| format!("creating {}", dest.display()))?;
    let mut reader = resp.body_mut().with_config().limit(2 * 1024 * 1024 * 1024).reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).ctx(|| "reading the download".into())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).ctx(|| format!("writing {}", dest.display()))?;
        done += n as u64;
        progress(done, total);
    }
    file.sync_all().ctx(|| format!("flushing {}", dest.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Extracts a 7z archive into `target`, refusing entries that would escape it.
pub fn extract_7z(archive: &Path, target: &Path) -> AppResult<()> {
    std::fs::create_dir_all(target).ctx(|| format!("creating {}", target.display()))?;
    let root = target.to_path_buf();
    sevenz_rust2::decompress_file_with_extract_fn(archive, target, |entry, reader, dest| {
        let inside = dest.starts_with(&root) && !dest.components().any(|c| matches!(c, std::path::Component::ParentDir));
        if !inside {
            return Err(sevenz_rust2::Error::Other(format!("archive entry '{}' points outside the target folder", entry.name()).into()));
        }
        sevenz_rust2::default_entry_extract_fn(entry, reader, dest)
    })
    .map_err(|e| AppError::Platform(format!("cannot extract {}: {e}", archive.display())))
}

/// If `dir` holds exactly one folder and nothing else, that folder is the real root.
pub fn single_root(dir: &Path) -> PathBuf {
    let entries: Vec<_> = std::fs::read_dir(dir).map(|rd| rd.flatten().collect()).unwrap_or_default();
    match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => dir.to_path_buf(),
    }
}

/// Extracts a zip into `target`, stripping `root` (e.g. `go/`). Refuses
/// entries that would escape the target folder.
pub fn extract_zip(archive: &Path, target: &Path, root: Option<&str>) -> AppResult<usize> {
    extract_zip_skipping(archive, target, root, &[])
}

/// Like `extract_zip`, leaving out entries under the `skip` folders (relative
/// to `root`).
pub fn extract_zip_skipping(archive: &Path, target: &Path, root: Option<&str>, skip: &[&str]) -> AppResult<usize> {
    let file = std::fs::File::open(archive).ctx(|| format!("opening {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| AppError::Platform(format!("not a valid zip archive: {e}")))?;
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| AppError::Platform(format!("corrupt archive entry: {e}")))?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(AppError::Validation(format!("archive entry '{}' points outside the target folder", entry.name())));
        };
        let rel = match root {
            Some(prefix) => match rel.strip_prefix(prefix) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            },
            None => rel,
        };
        if rel.as_os_str().is_empty() || skip.iter().any(|s| rel.starts_with(s)) {
            continue;
        }
        let out = target.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ctx(|| format!("creating {}", out.display()))?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).ctx(|| format!("creating {}", parent.display()))?;
            }
            let mut f = std::fs::File::create(&out).ctx(|| format!("creating {}", out.display()))?;
            std::io::copy(&mut entry, &mut f).ctx(|| format!("extracting {}", out.display()))?;
            count += 1;
        }
    }
    Ok(count)
}

impl Installer {
    pub fn new(db: Arc<Db>, paths: AppPaths) -> Self {
        Installer { db, paths, jobs: Arc::default() }
    }

    /// Versions offered by the component's official source.
    pub fn available(&self, component: ComponentId) -> AppResult<Vec<AvailableVersion>> {
        let provider = provider_for(component)
            .ok_or_else(|| AppError::Unsupported(format!("Zenv cannot install {component} yet; register an existing installation")))?;
        let mut list = provider.list(&http_agent())?;
        let installed = runtime_manager::list(&self.db, Some(component))?;
        for v in &mut list {
            if v.verified_by.is_empty() {
                v.verified_by = provider.verification();
            }
            v.installed_id = installed
                .iter()
                .find(|i| i.source == InstallSource::Managed && i.version == v.version)
                .map(|i| i.id.clone());
        }
        Ok(list)
    }

    pub fn jobs(&self) -> Vec<InstallJob> {
        let mut jobs: Vec<_> = self.jobs.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().collect();
        jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        jobs
    }

    fn update(jobs: &Mutex<HashMap<String, InstallJob>>, id: &str, f: impl FnOnce(&mut InstallJob)) {
        if let Some(job) = jobs.lock().unwrap_or_else(|p| p.into_inner()).get_mut(id) {
            f(job);
        }
    }

    /// Starts an install in the background and returns its job id.
    pub fn start(&self, component: ComponentId, version: &str) -> AppResult<String> {
        let provider = provider_for(component)
            .ok_or_else(|| AppError::Unsupported(format!("Zenv cannot install {component} yet")))?;
        let target = self.paths.install_dir(component.as_str(), version);
        if target.exists() {
            return Err(AppError::Conflict(format!("{} already exists; remove that version first", target.display())));
        }
        {
            let jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            if jobs.values().any(|j| j.component == component && j.version == version && !matches!(j.phase, Phase::Done | Phase::Failed)) {
                return Err(AppError::Conflict(format!("{component} {version} is already being installed")));
            }
        }
        let id = new_id();
        self.jobs.lock().unwrap_or_else(|p| p.into_inner()).insert(
            id.clone(),
            InstallJob {
                id: id.clone(),
                component,
                version: version.to_string(),
                phase: Phase::Downloading,
                downloaded: 0,
                total: 0,
                error: None,
                installation_id: None,
                started_at: now_rfc3339(),
            },
        );
        let (db, paths, jobs, job_id, version) = (self.db.clone(), self.paths.clone(), self.jobs.clone(), id.clone(), version.to_string());
        std::thread::spawn(move || {
            let result = run_install(&db, &paths, provider, &version, &target, |phase, done, total| {
                Installer::update(&jobs, &job_id, |j| {
                    j.phase = phase;
                    if total > 0 || done > 0 {
                        j.downloaded = done;
                        j.total = total;
                    }
                })
            });
            Installer::update(&jobs, &job_id, |j| match result {
                Ok(inst) => {
                    j.phase = Phase::Done;
                    j.installation_id = Some(inst.id);
                }
                Err(e) => {
                    j.phase = Phase::Failed;
                    j.error = Some(e.to_string());
                }
            });
        });
        Ok(id)
    }

    /// Same as `start` but waits; used by tests and the CLI path.
    pub fn install_blocking(&self, component: ComponentId, version: &str) -> AppResult<Installation> {
        let provider = provider_for(component).ok_or_else(|| AppError::Unsupported(format!("cannot install {component}")))?;
        let target = self.paths.install_dir(component.as_str(), version);
        if target.exists() {
            return Err(AppError::Conflict(format!("{} already exists", target.display())));
        }
        run_install(&self.db, &self.paths, provider, version, &target, |_, _, _| {})
    }

    /// Unregisters a Zenv-installed version and deletes its folder. External
    /// installations are never deleted (use unregister instead).
    pub fn uninstall(&self, installation_id: &str) -> AppResult<()> {
        let inst = runtime_manager::get(&self.db, installation_id)?;
        if inst.source != InstallSource::Managed || !inst.root_path.starts_with(&self.paths.bin_dir) {
            return Err(AppError::Conflict(format!(
                "{} was not installed by Zenv; it can only be removed from the list, not deleted",
                inst.root_path.display()
            )));
        }
        runtime_manager::unregister(&self.db, installation_id)?;
        std::fs::remove_dir_all(&inst.root_path).ctx(|| format!("deleting {}", inst.root_path.display()))?;
        tracing::info!(component = %inst.component, version = %inst.version, "managed installation removed");
        Ok(())
    }
}

fn run_install(
    db: &Db,
    paths: &AppPaths,
    provider: &dyn InstallProvider,
    version: &str,
    target: &Path,
    mut progress: impl FnMut(Phase, u64, u64),
) -> AppResult<Installation> {
    let component = provider.component();
    let http = http_agent();
    let offer = provider
        .list(&http)?
        .into_iter()
        .find(|v| v.version == version)
        .ok_or_else(|| AppError::NotFound { entity: "version", id: format!("{component} {version} at {}", provider.source()) })?;

    let archive = paths.downloads_dir.join(format!("{}.{}.part", offer.filename, new_id()));
    let cleanup = |p: &Path| {
        let _ = std::fs::remove_file(p);
    };
    let digest = match download(&http, &offer.url, provider.allowed_hosts(), &archive, |d, t| progress(Phase::Downloading, d, t)) {
        Ok(d) => d,
        Err(e) => {
            cleanup(&archive);
            return Err(e);
        }
    };
    progress(Phase::Verifying, 0, 0);
    let checked = if offer.sha256.is_empty() { !provider.requires_checksum() } else { digest.eq_ignore_ascii_case(&offer.sha256) };
    if !checked {
        cleanup(&archive);
        return Err(AppError::Validation(format!(
            "checksum mismatch for {}: expected {}, got {digest}; the download was discarded",
            offer.filename, offer.sha256
        )));
    }

    progress(Phase::Extracting, 0, 0);
    let unpacked = unpack_into(provider, &archive, target, &offer);
    cleanup(&archive);
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(target);
        return Err(e);
    }

    progress(Phase::Validating, 0, 0);
    let validated = provider.post_install(target).and_then(|_| provider.locate(target)).and_then(|located| {
        let found = probe::detect_version(component, &located.executable)?;
        if found.version != version {
            return Err(AppError::Validation(format!("installed files report version {}, expected {version}", found.version)));
        }
        let install = runtime_manager::insert(db, component, &located, &found, InstallSource::Managed)?;
        if component == ComponentId::Frankenphp {
            // Its embedded PHP, so apps on that PHP version run in FrankenPHP.
            // External: removing it must never delete the FrankenPHP folder.
            if let Err(e) = runtime_manager::register(db, ComponentId::Php, target) {
                tracing::warn!(error = %e, "could not register FrankenPHP's PHP");
            }
        }
        Ok(install)
    });
    if validated.is_err() {
        // Zenv created this folder moments ago; remove the broken install.
        let _ = std::fs::remove_dir_all(target);
    }
    validated
}

/// Extracts (or runs) the verified download so `target` holds the installation.
fn unpack_into(provider: &dyn InstallProvider, download: &Path, target: &Path, offer: &AvailableVersion) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).ctx(|| format!("creating {}", parent.display()))?;
    }
    let staging: PathBuf = target.with_file_name(format!(".staging-{}-{}", offer.version, new_id()));
    let result = (|| match provider.unpack() {
        Unpack::Custom => {
            std::fs::create_dir_all(target).ctx(|| format!("creating {}", target.display()))?;
            provider.custom_install(download, target, offer)
        }
        Unpack::Zip { root } => {
            match root {
                Some(prefix) => extract_zip(download, &staging, Some(prefix)).map(|_| ())?,
                None => extract_zip(download, &staging, None).map(|_| ())?,
            }
            let real = if root.is_some() { staging.clone() } else { single_root(&staging) };
            std::fs::rename(&real, target).ctx(|| format!("moving into {}", target.display()))
        }
        Unpack::ZipSkipping { root, skip } => {
            extract_zip_skipping(download, &staging, Some(root), skip)?;
            std::fs::rename(&staging, target).ctx(|| format!("moving into {}", target.display()))
        }
        Unpack::SevenZip => {
            extract_7z(download, &staging)?;
            std::fs::rename(single_root(&staging), target).ctx(|| format!("moving into {}", target.display()))
        }
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

#[cfg(test)]
mod tests {

    use super::*;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_and_strips_the_archive_root() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("go.zip");
        make_zip(&archive, &[("go/bin/go.exe", b"x"), ("go/VERSION", b"go1.27.0"), ("other/readme", b"skip")]);
        let target = dir.path().join("out");
        assert_eq!(extract_zip(&archive, &target, Some("go")).unwrap(), 2);
        assert!(target.join("bin").join("go.exe").is_file());
        assert!(!target.join("other").exists(), "entries outside the root are skipped");
    }

    #[test]
    fn refuses_zip_slip() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        make_zip(&archive, &[("../escape.txt", b"x")]);
        assert!(extract_zip(&archive, &dir.path().join("out"), None).is_err());
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    #[cfg(windows)]
    fn programs_must_carry_the_expected_signature() {
        // This test binary is not signed.
        let me = std::env::current_exe().unwrap();
        assert!(require_signer(std::slice::from_ref(&me), "Oracle America, Inc.").is_err());
        assert!(require_signer(&[], "Oracle America, Inc.").is_err(), "nothing to check is a failure");
        // Edge ships with Windows and is signed by Microsoft.
        if let Some(edge) = crate::thumbnails::browser().filter(|p| p.ends_with("msedge.exe")) {
            assert!(require_signer(std::slice::from_ref(&edge), "Microsoft Corporation").is_ok());
            assert!(require_signer(&[edge], "OpenJS Foundation").is_err(), "another signer");
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin").join("plugin")).unwrap();
        for f in ["bin/a.exe", "bin/plugin/b.DLL", "bin/readme.txt"] {
            std::fs::write(dir.path().join(f), "x").unwrap();
        }
        assert_eq!(programs_under(dir.path()).len(), 2, "exe and dll at any depth");
    }

    #[test]
    fn downloads_only_from_allowed_https_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let http = http_agent();
        let err = download(&http, "http://go.dev/dl/x.zip", &["go.dev"], &dir.path().join("x"), |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("HTTPS"));
        let err = download(&http, "https://evil.example/x.zip", &["go.dev"], &dir.path().join("x"), |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("not an allowed"));
    }

    #[test]
    fn languages_and_web_servers_are_installable() {
        assert_eq!(
            installable_components(),
            vec![
                ComponentId::Apache,
                ComponentId::Nginx,
                ComponentId::Caddy,
                ComponentId::Frankenphp,
                ComponentId::Php,
                ComponentId::Go,
                ComponentId::Rust,
                ComponentId::Ruby,
                ComponentId::Python,
                ComponentId::Node,
                ComponentId::Mysql,
                ComponentId::Postgresql,
                ComponentId::Omniroute,
                ComponentId::Ninerouter
            ]
        );
    }

    #[test]
    fn managed_uninstall_never_touches_external_installations() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let paths = AppPaths::new(dir.path().join("home"));
        let external_root = dir.path().join("Go");
        std::fs::create_dir_all(external_root.join("bin")).unwrap();
        let ext = runtime_manager::insert(
            &db,
            ComponentId::Go,
            &probe::Located { root: external_root.clone(), executable: external_root.join("bin/go.exe") },
            &probe::VersionInfo { version: "1.27.0".into(), detail: String::new() },
            InstallSource::External,
        )
        .unwrap();
        let installer = Installer::new(db, paths);
        assert!(matches!(installer.uninstall(&ext.id), Err(AppError::Conflict(_))));
        assert!(external_root.join("bin").is_dir());
    }
}
