//! MySQL and PostgreSQL for Windows from their official sources.
//!
//! - MySQL: releases from the tags of github.com/mysql/mysql-server (newest
//!   patch of the LTS series and of the newest series), zips from
//!   cdn.mysql.com. Oracle publishes only MD5 sums, behind a page that refuses
//!   automated requests, so Zenv checks Oracle's code signature on every
//!   program and library in it instead: a changed file or another signer
//!   fails the install.
//! - PostgreSQL: supported versions from postgresql.org's versions.json, zips
//!   from EDB (the Windows builds postgresql.org links to). EDB publishes no
//!   checksums and signs none of these files, so Zenv takes the SHA-256 that
//!   Scoop (github.com/ScoopInstaller) recorded for the same zip: a witness
//!   outside EDB's server. Without one (a build Scoop has not seen yet) the
//!   download is only protected by HTTPS, and Zenv says so. pgAdmin and
//!   StackBuilder in the zip are not extracted.

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

pub struct MysqlProvider;
pub struct PostgresqlProvider;

/// Long-term support series, offered next to the newest series.
const MYSQL_LTS: &[(u32, u32)] = &[(8, 4), (9, 7)];
const MYSQL_SIGNER: &str = "Oracle America, Inc.";

#[derive(Deserialize)]
struct GitRef {
    #[serde(rename = "ref")]
    name: String,
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.split('.').map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(v)
}

/// Releases to offer, newest first: per series the patches newest first
/// (a newest tag may not have its Windows zip published yet).
pub fn mysql_candidates(tags: &[String]) -> Vec<((u32, u32), Vec<(u32, u32, u32)>)> {
    let mut by_series: std::collections::BTreeMap<(u32, u32), Vec<(u32, u32, u32)>> = Default::default();
    for t in tags {
        if let Some(v) = t.strip_prefix("refs/tags/mysql-").and_then(parse_version) {
            by_series.entry((v.0, v.1)).or_default().push(v);
        }
    }
    let newest = by_series.keys().next_back().copied();
    let mut out: Vec<_> = by_series
        .into_iter()
        .filter(|(s, _)| MYSQL_LTS.contains(s) || Some(*s) == newest)
        .map(|(s, mut vs)| {
            vs.sort_unstable_by(|a, b| b.cmp(a));
            vs.truncate(3);
            (s, vs)
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

/// The size of a downloadable file, or None when it is not there.
fn head(http: &ureq::Agent, url: &str) -> Option<u64> {
    let resp = http.head(url).call().ok()?;
    Some(resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0))
}

impl InstallProvider for MysqlProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Mysql
    }
    fn source(&self) -> &'static str {
        "cdn.mysql.com"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["cdn.mysql.com"]
    }
    fn requires_checksum(&self) -> bool {
        false
    }
    fn verification(&self) -> &'static str {
        "Oracle's code signature on every program and library"
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let refs: Vec<GitRef> = http
            .get("https://api.github.com/repos/mysql/mysql-server/git/matching-refs/tags/mysql-")
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach MySQL's release list on GitHub: {e}")))?
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_json()
            .map_err(|e| AppError::Platform(format!("unexpected MySQL release list: {e}")))?;
        let tags: Vec<String> = refs.into_iter().map(|r| r.name).collect();
        let mut out = Vec::new();
        for ((maj, min), patches) in mysql_candidates(&tags) {
            for (a, b, c) in patches {
                let version = format!("{a}.{b}.{c}");
                let filename = format!("mysql-{version}-winx64.zip");
                let found = [
                    format!("https://cdn.mysql.com/Downloads/MySQL-{maj}.{min}/{filename}"),
                    format!("https://cdn.mysql.com/archives/mysql-{maj}.{min}/{filename}"),
                ]
                .into_iter()
                .find_map(|url| head(http, &url).map(|size| (url, size)));
                if let Some((url, size)) = found {
                    out.push(AvailableVersion {
                        component: ComponentId::Mysql,
                        version,
                        // Every offered series is a GA release (LTS or Innovation).
                        stable: true,
                        filename,
                        url,
                        sha256: String::new(),
                        size,
                        source: "cdn.mysql.com",
                        installed_id: None,
                        verified_by: "",
                    });
                    break;
                }
            }
        }
        Ok(out)
    }

    fn post_install(&self, target: &std::path::Path) -> AppResult<()> {
        if !target.join("bin").join("mysqld.exe").is_file() {
            return Err(AppError::Validation("the MySQL zip has no bin\\mysqld.exe; the install was discarded".into()));
        }
        super::require_signer(&super::programs_under(target), MYSQL_SIGNER)
    }
}

const PG_WITNESS: &str = "SHA-256 recorded independently by Scoop (github.com/ScoopInstaller)";
const PG_HTTPS_ONLY: &str = "HTTPS from EDB's server only: no independent checksum for this build yet";

/// What Scoop recorded for a PostgreSQL zip: (version, url, sha256).
#[derive(Debug, PartialEq, Eq)]
pub struct Witness {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

/// Reads a Scoop manifest (`version`, `architecture.64bit.url` and `.hash`).
pub fn parse_scoop(json: &str) -> Option<Witness> {
    let j: serde_json::Value = serde_json::from_str(json).ok()?;
    let arch = j.get("architecture").and_then(|a| a.get("64bit")).unwrap_or(&j);
    let url = arch.get("url")?.as_str()?;
    let hash = arch.get("hash")?.as_str()?;
    let hash = hash.strip_prefix("sha256:").unwrap_or(hash);
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(Witness {
        version: j.get("version")?.as_str()?.to_string(),
        // Scoop may append `#/name` to rename the download.
        url: url.split('#').next().unwrap_or(url).to_string(),
        sha256: hash.to_ascii_lowercase(),
    })
}

/// The witness for `version` (`18.6`) when Scoop recorded that release
/// (`18.6` or a rebuild like `18.6-5`) from EDB's server.
fn pg_witness(http: &ureq::Agent, version: &str) -> Option<Witness> {
    let major = version.split('.').next()?;
    [
        format!("https://raw.githubusercontent.com/ScoopInstaller/Versions/master/bucket/postgresql{major}.json"),
        "https://raw.githubusercontent.com/ScoopInstaller/Main/master/bucket/postgresql.json".to_string(),
    ]
    .iter()
    .filter_map(|u| http.get(u).call().ok()?.body_mut().read_to_string().ok())
    .filter_map(|text| parse_scoop(&text))
    .find(|w| {
        (w.version == version || w.version.starts_with(&format!("{version}-")))
            && ["https://get.enterprisedb.com/", "https://sbp.enterprisedb.com/"].iter().any(|h| w.url.starts_with(h))
    })
}

#[derive(Deserialize)]
struct PgVersion {
    major: String,
    #[serde(rename = "latestMinor")]
    latest_minor: serde_json::Value,
    supported: bool,
}

/// Supported PostgreSQL versions (major.latestMinor), newest first.
pub fn pg_versions(json: &str) -> AppResult<Vec<String>> {
    let list: Vec<PgVersion> = serde_json::from_str(json).map_err(|e| AppError::Platform(format!("unexpected PostgreSQL version list: {e}")))?;
    let mut out: Vec<(u32, String)> = list
        .into_iter()
        .filter(|v| v.supported)
        .filter_map(|v| {
            let minor = match &v.latest_minor {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => return None,
            };
            Some((v.major.parse().ok()?, format!("{}.{minor}", v.major)))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(out.into_iter().map(|(_, v)| v).collect())
}

impl InstallProvider for PostgresqlProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Postgresql
    }
    fn source(&self) -> &'static str {
        "EDB (linked from postgresql.org)"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["get.enterprisedb.com", "sbp.enterprisedb.com"]
    }
    fn requires_checksum(&self) -> bool {
        false
    }
    fn verification(&self) -> &'static str {
        "HTTPS from EDB's server only (EDB publishes no checksums or signatures)"
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let json = http
            .get("https://www.postgresql.org/versions.json")
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach postgresql.org: {e}")))?
            .body_mut()
            .read_to_string()
            .map_err(|e| AppError::Platform(format!("unexpected PostgreSQL version list: {e}")))?;
        let mut out = Vec::new();
        for version in pg_versions(&json)? {
            let found = match pg_witness(http, &version) {
                Some(w) => head(http, &w.url).map(|size| {
                    let filename = w.url.rsplit('/').next().unwrap_or_default().to_string();
                    (filename, w.url, size, w.sha256, PG_WITNESS)
                }),
                // EDB's build number starts at 1; a rebuild bumps it. The newest wins.
                None => (1..=6).rev().find_map(|build| {
                    let filename = format!("postgresql-{version}-{build}-windows-x64-binaries.zip");
                    let url = format!("https://get.enterprisedb.com/postgresql/{filename}");
                    head(http, &url).map(|size| (filename, url, size, String::new(), PG_HTTPS_ONLY))
                }),
            };
            if let Some((filename, url, size, sha256, verified_by)) = found {
                out.push(AvailableVersion {
                    component: ComponentId::Postgresql,
                    version,
                    stable: true,
                    filename,
                    url,
                    sha256,
                    size,
                    source: "get.enterprisedb.com",
                    installed_id: None,
                    verified_by,
                });
            }
        }
        Ok(out)
    }

    fn unpack(&self) -> Unpack {
        Unpack::ZipSkipping { root: "pgsql", skip: &["pgAdmin 4", "StackBuilder", "doc"] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_offers_lts_series_and_the_newest() {
        let tags: Vec<String> = ["8.0.43", "8.4.12", "8.4.13", "9.6.1", "9.7.3", "9.7.4", "26.7.0", "26.10.0", "26.10.1-rc", "cluster-9.7.4"]
            .iter()
            .map(|v| format!("refs/tags/mysql-{v}"))
            .collect();
        let c = mysql_candidates(&tags);
        let series: Vec<(u32, u32)> = c.iter().map(|(s, _)| *s).collect();
        assert_eq!(series, vec![(26, 10), (9, 7), (8, 4)]);
        assert_eq!(c[1].1, vec![(9, 7, 4), (9, 7, 3)]);
    }

    #[test]
    fn scoop_records_the_postgresql_zip_hash() {
        let json = r#"{"version":"18.6-5","architecture":{"64bit":{
            "url":"https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip#/pg.zip",
            "hash":"E2246BA91D22345BC3D017586C09EDE52D9DF180B1EEB480F050445F1CAD84E2"}}}"#;
        let w = parse_scoop(json).unwrap();
        assert_eq!(w.version, "18.6-5");
        assert_eq!(w.url, "https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip");
        assert_eq!(w.sha256, "e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2");
        assert!(parse_scoop(r#"{"version":"1","url":"https://x","hash":"md5:abc"}"#).is_none());
    }

    #[test]
    fn postgresql_supported_versions_newest_first() {
        let json = r#"[{"major":"18","latestMinor":6,"supported":true},{"major":"13","latestMinor":23,"supported":false},
                      {"major":"14","latestMinor":"24","supported":true},{"major":"17","latestMinor":11,"supported":true}]"#;
        assert_eq!(pg_versions(json).unwrap(), vec!["18.6", "17.11", "14.24"]);
    }
}
