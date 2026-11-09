//! PHP from windows.php.net (now served from downloads.php.net). Its
//! `releases.json` lists the latest patch of every supported branch with
//! SHA-256 checksums; older patches have no published checksum and are not
//! offered. Zenv installs the non-thread-safe x64 build, which is what its
//! php-cgi FastCGI workers use.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult, IoContext};
use crate::runtime_catalog::ComponentId;

pub const FEED: &str = "https://downloads.php.net/~windows/releases/releases.json";
pub const BASE: &str = "https://downloads.php.net/~windows/releases/";

/// Extensions a typical Laravel / Composer project needs, enabled in the
/// generated php.ini when the DLL is present.
const EXTENSIONS: &[&str] = &[
    "bz2", "curl", "fileinfo", "gd", "gettext", "intl", "mbstring", "exif", "mysqli", "openssl", "pdo_mysql",
    "pdo_pgsql", "pdo_sqlite", "pgsql", "sodium", "sqlite3", "zip",
];

pub struct PhpProvider;

#[derive(Debug, Deserialize)]
struct ZipFile {
    path: String,
    size: Option<String>,
    sha256: Option<String>,
}

/// `"33.48MB"` → bytes (approximate; display only).
fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    let (num, mul) = if let Some(n) = s.strip_suffix("MB") {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1024.0)
    } else {
        (s, 1.0)
    };
    (num.trim().parse::<f64>().unwrap_or(0.0) * mul) as u64
}

/// Picks each branch's NTS x64 zip (`nts-vs17-x64`, `nts-vs16-x64`, …).
pub fn parse(feed: &serde_json::Value) -> Vec<AvailableVersion> {
    let Some(branches) = feed.as_object() else { return vec![] };
    let mut out: BTreeMap<String, AvailableVersion> = BTreeMap::new();
    for branch in branches.values() {
        let Some(obj) = branch.as_object() else { continue };
        let Some(version) = obj.get("version").and_then(|v| v.as_str()) else { continue };
        // A Windows rebuild is listed as `8.5.12-1`; PHP itself reports 8.5.12.
        let version = match version.split_once('-') {
            Some((v, build)) if !build.is_empty() && build.chars().all(|c| c.is_ascii_digit()) => v,
            _ => version,
        };
        let Some(entry) = obj
            .iter()
            .filter(|(k, _)| k.starts_with("nts-") && k.ends_with("-x64"))
            .find_map(|(_, v)| v.get("zip"))
            .and_then(|z| serde_json::from_value::<ZipFile>(z.clone()).ok())
        else {
            continue;
        };
        let Some(sha) = entry.sha256 else { continue };
        out.insert(
            version.to_string(),
            AvailableVersion {
                component: ComponentId::Php,
                version: version.to_string(),
                stable: true,
                url: format!("{BASE}{}", entry.path),
                filename: entry.path,
                sha256: sha.to_ascii_lowercase(),
                size: entry.size.as_deref().map(parse_size).unwrap_or(0),
                source: "windows.php.net",
                installed_id: None,
                verified_by: "",
            },
        );
    }
    let mut list: Vec<_> = out.into_values().collect();
    list.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    list
}

fn version_key(v: &str) -> Vec<u32> {
    v.split('.').filter_map(|p| p.parse().ok()).collect()
}

/// php.ini from php.ini-development with extension_dir and common extensions.
pub fn write_php_ini(root: &Path) -> AppResult<()> {
    let ini = root.join("php.ini");
    if ini.exists() {
        return Ok(());
    }
    let template = std::fs::read_to_string(root.join("php.ini-development")).unwrap_or_default();
    let ext_dir = root.join("ext");
    let mut text = String::with_capacity(template.len() + 1024);
    text.push_str(&template);
    text.push_str("\n\n; ---- Added by Zenv when it installed this PHP ----\n");
    text.push_str(&format!("extension_dir = \"{}\"\n", ext_dir.display()));
    for ext in EXTENSIONS {
        if ext_dir.join(format!("php_{ext}.dll")).is_file() {
            text.push_str(&format!("extension={ext}\n"));
        }
    }
    text.push_str("memory_limit = 512M\nupload_max_filesize = 64M\npost_max_size = 64M\n");
    std::fs::write(&ini, text).ctx(|| format!("writing {}", ini.display()))
}

impl InstallProvider for PhpProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Php
    }

    fn source(&self) -> &'static str {
        "windows.php.net"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["downloads.php.net", "windows.php.net"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let feed: serde_json::Value = http
            .get(FEED)
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach windows.php.net: {e}")))?
            .body_mut()
            .read_json()
            .map_err(|e| AppError::Platform(format!("unexpected PHP release data: {e}")))?;
        Ok(parse(&feed))
    }

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: None }
    }

    fn post_install(&self, target: &Path) -> AppResult<()> {
        write_php_ini(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nts_x64_with_checksums() {
        let feed: serde_json::Value = serde_json::from_str(
            r#"{
              "8.4": {"version": "8.4.26",
                "ts-vs17-x64": {"zip": {"path": "php-8.4.26-Win32-vs17-x64.zip", "size": "33.51MB", "sha256": "AA"}},
                "nts-vs17-x64": {"zip": {"path": "php-8.4.26-nts-Win32-vs17-x64.zip", "size": "33.48MB", "sha256": "BB"}},
                "nts-vs17-x86": {"zip": {"path": "php-8.4.26-nts-Win32-vs17-x86.zip", "size": "30MB", "sha256": "CC"}}},
              "7.4": {"version": "7.4.33",
                "nts-vc15-x64": {"zip": {"path": "php-7.4.33-nts-Win32-vc15-x64.zip", "size": "25MB", "sha256": "DD"}}},
              "8.5": {"version": "8.5.1", "source": {"path": "x.tar.gz"}},
              "8.6": {"version": "8.6.2-1",
                "nts-vs17-x64": {"zip": {"path": "php-8.6.2-1-nts-Win32-vs17-x64.zip", "size": "35MB", "sha256": "EE"}}}
            }"#,
        )
        .unwrap();
        let list = parse(&feed);
        assert_eq!(list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["8.6.2", "8.4.26", "7.4.33"], "a rebuild is the version PHP reports");
        let list: Vec<_> = list.into_iter().skip(1).collect();
        assert_eq!(list[0].url, "https://downloads.php.net/~windows/releases/php-8.4.26-nts-Win32-vs17-x64.zip");
        assert_eq!(list[0].sha256, "bb");
        assert!(list[0].size > 30_000_000);
    }

    #[test]
    fn php_ini_enables_only_present_extensions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ext")).unwrap();
        std::fs::write(dir.path().join("php.ini-development"), "[PHP]\n").unwrap();
        for e in ["mbstring", "openssl", "pdo_mysql"] {
            std::fs::write(dir.path().join("ext").join(format!("php_{e}.dll")), "").unwrap();
        }
        write_php_ini(dir.path()).unwrap();
        let ini = std::fs::read_to_string(dir.path().join("php.ini")).unwrap();
        assert!(ini.contains("extension=mbstring") && ini.contains("extension=pdo_mysql"));
        assert!(!ini.contains("extension=intl"), "missing DLLs are not enabled");
        // Never overwrites an existing php.ini.
        std::fs::write(dir.path().join("php.ini"), "custom").unwrap();
        write_php_ini(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("php.ini")).unwrap(), "custom");
    }
}
