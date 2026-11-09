//! Go toolchains from go.dev. The release feed
//! (`https://go.dev/dl/?mode=json&include=all`) lists every file with its
//! SHA-256; Windows archives are zips with a top-level `go/` folder.

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

pub const FEED: &str = "https://go.dev/dl/?mode=json&include=all";

pub struct GoProvider;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub version: String,
    pub stable: bool,
    pub files: Vec<ReleaseFile>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseFile {
    pub filename: String,
    pub os: String,
    pub arch: String,
    pub sha256: String,
    pub size: u64,
    pub kind: String,
}

pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86" => "386",
        _ => "amd64",
    }
}

/// Windows archives for this machine's architecture, newest first.
pub fn parse(releases: Vec<Release>, arch: &str) -> Vec<AvailableVersion> {
    releases
        .into_iter()
        .filter_map(|r| {
            let file = r.files.into_iter().find(|f| f.os == "windows" && f.arch == arch && f.kind == "archive" && f.filename.ends_with(".zip"))?;
            Some(AvailableVersion {
                component: ComponentId::Go,
                version: r.version.trim_start_matches("go").to_string(),
                stable: r.stable,
                url: format!("https://go.dev/dl/{}", file.filename),
                filename: file.filename,
                sha256: file.sha256.to_ascii_lowercase(),
                size: file.size,
                source: "go.dev",
                installed_id: None,
                verified_by: "",
            })
        })
        .collect()
}

impl InstallProvider for GoProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Go
    }

    fn source(&self) -> &'static str {
        "go.dev"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["go.dev", "dl.google.com"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let releases: Vec<Release> = http
            .get(FEED)
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach go.dev: {e}")))?
            .body_mut()
            .read_json()
            .map_err(|e| AppError::Platform(format!("unexpected go.dev release data: {e}")))?;
        Ok(parse(releases, host_arch()))
    }

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: Some("go") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_windows_zip_for_the_architecture() {
        let json = r#"[
          {"version":"go1.27.0","stable":true,"files":[
            {"filename":"go1.27.0.src.tar.gz","os":"","arch":"","version":"go1.27.0","sha256":"aa","size":1,"kind":"source"},
            {"filename":"go1.27.0.windows-amd64.msi","os":"windows","arch":"amd64","version":"go1.27.0","sha256":"bb","size":2,"kind":"installer"},
            {"filename":"go1.27.0.windows-amd64.zip","os":"windows","arch":"amd64","version":"go1.27.0","sha256":"F0C0","size":78924880,"kind":"archive"}
          ]},
          {"version":"go1.28rc1","stable":false,"files":[
            {"filename":"go1.28rc1.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.28rc1","sha256":"cc","size":3,"kind":"archive"}
          ]}
        ]"#;
        let releases: Vec<Release> = serde_json::from_str(json).unwrap();
        let list = parse(releases, "amd64");
        assert_eq!(list.len(), 1, "releases without a Windows zip are skipped");
        let v = &list[0];
        assert_eq!((v.version.as_str(), v.stable, v.size), ("1.27.0", true, 78924880));
        assert_eq!(v.url, "https://go.dev/dl/go1.27.0.windows-amd64.zip");
        assert_eq!(v.sha256, "f0c0");
    }
}
