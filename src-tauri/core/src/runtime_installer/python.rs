//! Python from python.org, using the index the official Python install
//! manager reads (`index-windows.json`, PEP 773). Each entry is a full
//! (non-embeddable) package with pip and venv, and a SHA-256.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::probe;

pub const INDEX: &str = "https://www.python.org/ftp/python/index-windows.json";

pub struct PythonProvider;

#[derive(Debug, Deserialize)]
pub struct Index {
    pub versions: Vec<Entry>,
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "sort-version")]
    pub sort_version: String,
    pub url: String,
    pub hash: BTreeMap<String, String>,
}

/// `pythoncore-3.13-64` only: CPython, x64, not free-threaded, not embeddable.
fn wanted(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("pythoncore-").and_then(|r| r.strip_suffix("-64")) else { return false };
    rest.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn parse(entries: Vec<Entry>) -> Vec<AvailableVersion> {
    let mut seen = BTreeMap::new();
    for e in entries.into_iter().filter(|e| wanted(&e.id)) {
        let Some(sha) = e.hash.get("sha256") else { continue };
        let stable = e.sort_version.chars().all(|c| c.is_ascii_digit() || c == '.');
        let filename = e.url.rsplit('/').next().unwrap_or_default().to_string();
        seen.entry(e.sort_version.clone()).or_insert(AvailableVersion {
            component: ComponentId::Python,
            version: e.sort_version,
            stable,
            filename,
            url: e.url,
            sha256: sha.to_ascii_lowercase(),
            size: 0,
            source: "python.org",
            installed_id: None,
            verified_by: "",
        });
    }
    let mut list: Vec<_> = seen.into_values().collect();
    let key = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse::<u32>().ok()).collect::<Vec<_>>();
    list.sort_by(|a, b| key(&b.version).cmp(&key(&a.version)));
    list
}

impl InstallProvider for PythonProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Python
    }

    fn source(&self) -> &'static str {
        "python.org"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["www.python.org"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let fetch = |url: &str| -> AppResult<Index> {
            http.get(url)
                .call()
                .map_err(|e| AppError::Platform(format!("cannot reach python.org: {e}")))?
                .body_mut()
                .read_json()
                .map_err(|e| AppError::Platform(format!("unexpected python.org index data: {e}")))
        };
        let first = fetch(INDEX)?;
        let mut entries = first.versions;
        // One more page covers the recent patch releases of every branch.
        if let Some(next) = first.next.filter(|n| !n.contains('/')) {
            if let Ok(more) = fetch(&format!("https://www.python.org/ftp/python/{next}")) {
                entries.extend(more.versions);
            }
        }
        Ok(parse(entries))
    }

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: None }
    }

    fn verification(&self) -> &'static str {
        "SHA-256 from python.org and the Python Software Foundation's code signature on Python itself"
    }

    /// Python itself (python.exe, pythonw.exe, python3XX.dll) must carry the
    /// PSF's code signature. The full package normally ships pip; if not,
    /// bootstrap it offline.
    fn post_install(&self, target: &Path) -> AppResult<()> {
        let core: Vec<_> = std::fs::read_dir(target)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                name == "python.exe" || name == "pythonw.exe" || (name.starts_with("python3") && name.ends_with(".dll"))
            })
            .collect();
        super::require_signer(&core, "Python Software Foundation")?;
        if target.join("Lib").join("site-packages").join("pip").is_dir() {
            return Ok(());
        }
        let out = probe::run_capture(&target.join("python.exe"), &["-m", "ensurepip", "--default-pip"], Duration::from_secs(300))?;
        if out.code != Some(0) {
            return Err(AppError::Platform(format!("pip could not be set up: {}", out.text.trim())));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_full_x64_cpython_only() {
        let json = r#"[
          {"id":"pythoncore-3.13-64","sort-version":"3.13.16","url":"https://www.python.org/ftp/python/3.13.16/python-3.13.16-amd64.zip","hash":{"sha256":"AB"}},
          {"id":"pythonembed-3.13-64","sort-version":"3.13.16","url":"https://x/embed.zip","hash":{"sha256":"CD"}},
          {"id":"pythoncore-3.14t-64","sort-version":"3.14.8","url":"https://x/t.zip","hash":{"sha256":"EF"}},
          {"id":"pythoncore-3.13-32","sort-version":"3.13.16","url":"https://x/32.zip","hash":{"sha256":"GH"}},
          {"id":"pythoncore-3.16-64","sort-version":"3.16.0a1","url":"https://www.python.org/ftp/python/3.16.0/python-3.16.0a1-amd64.zip","hash":{"sha256":"IJ"}},
          {"id":"pythoncore-3.14-64","sort-version":"3.14.8","url":"https://www.python.org/ftp/python/3.14.8/python-3.14.8-amd64.zip","hash":{"sha256":"KL"}}
        ]"#;
        let list = parse(serde_json::from_str(json).unwrap());
        assert_eq!(list.iter().map(|v| (v.version.as_str(), v.stable)).collect::<Vec<_>>(), [("3.16.0a1", false), ("3.14.8", true), ("3.13.16", true)]);
        assert_eq!(list[1].sha256, "kl");
        assert_eq!(list[1].filename, "python-3.14.8-amd64.zip");
    }
}
