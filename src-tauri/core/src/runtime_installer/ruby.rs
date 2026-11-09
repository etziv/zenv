//! Ruby from RubyInstaller2 (the Windows Ruby distribution). Its GitHub
//! releases carry SHA-256 digests for every asset; Zenv installs the
//! `rubyinstaller-<version>-<n>-x64.7z` archive (no installer, no registry).
//! Native gems additionally need the MSYS2 DevKit (`ridk install`).

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

pub const RELEASES: &str = "https://api.github.com/repos/oneclick/rubyinstaller2/releases?per_page=40";

pub struct RubyProvider;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    pub digest: Option<String>,
}

/// `rubyinstaller-3.4.11-1-x64.7z` → `3.4.11`.
fn version_of(name: &str) -> Option<String> {
    let rest = name.strip_prefix("rubyinstaller-")?.strip_suffix("-x64.7z")?;
    let (version, build) = rest.rsplit_once('-')?;
    (build.chars().all(|c| c.is_ascii_digit()) && version.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .then(|| version.to_string())
}

pub fn parse(releases: Vec<Release>) -> Vec<AvailableVersion> {
    let mut out: Vec<AvailableVersion> = Vec::new();
    for r in releases.into_iter().filter(|r| !r.draft) {
        for a in r.assets {
            let (Some(version), Some(digest)) = (version_of(&a.name), a.digest.as_deref()) else { continue };
            let Some(sha) = digest.strip_prefix("sha256:") else { continue };
            if out.iter().any(|v| v.version == version) {
                continue; // newest build of each Ruby version only
            }
            out.push(AvailableVersion {
                component: ComponentId::Ruby,
                version,
                stable: !r.prerelease,
                filename: a.name,
                url: a.browser_download_url,
                sha256: sha.to_ascii_lowercase(),
                size: a.size,
                source: "rubyinstaller.org",
                installed_id: None,
                verified_by: "",
            });
        }
    }
    let key = |v: &str| v.split('.').filter_map(|p| p.parse::<u32>().ok()).collect::<Vec<_>>();
    out.sort_by(|a, b| key(&b.version).cmp(&key(&a.version)));
    out
}

impl InstallProvider for RubyProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Ruby
    }

    fn source(&self) -> &'static str {
        "rubyinstaller.org"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let releases: Vec<Release> = http
            .get(RELEASES)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach GitHub releases: {e}")))?
            .body_mut()
            .read_json()
            .map_err(|e| AppError::Platform(format!("unexpected RubyInstaller release data: {e}")))?;
        Ok(parse(releases))
    }

    fn unpack(&self) -> Unpack {
        Unpack::SevenZip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions_and_digests() {
        assert_eq!(version_of("rubyinstaller-3.4.11-1-x64.7z").as_deref(), Some("3.4.11"));
        assert_eq!(version_of("rubyinstaller-devkit-3.4.11-1-x64.exe"), None);
        assert_eq!(version_of("rubyinstaller-3.4.11-1-x86.7z"), None);
        let json = r#"[
          {"prerelease":false,"draft":false,"assets":[
            {"name":"rubyinstaller-3.4.11-1-x64.7z","size":15000000,"browser_download_url":"https://github.com/x/a.7z","digest":"sha256:AA11"},
            {"name":"rubyinstaller-devkit-3.4.11-1-x64.exe","size":1,"browser_download_url":"https://github.com/x/b.exe","digest":"sha256:BB"}]},
          {"prerelease":false,"draft":false,"assets":[
            {"name":"rubyinstaller-4.0.7-1-x64.7z","size":16000000,"browser_download_url":"https://github.com/x/c.7z","digest":"sha256:CC"},
            {"name":"rubyinstaller-3.3.0-1-x64.7z","size":1,"browser_download_url":"https://github.com/x/d.7z","digest":null}]}
        ]"#;
        let list = parse(serde_json::from_str(json).unwrap());
        assert_eq!(list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["4.0.7", "3.4.11"], "assets without a digest are skipped");
        assert_eq!(list[1].sha256, "aa11");
    }
}
