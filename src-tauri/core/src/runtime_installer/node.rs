//! Node.js from nodejs.org. `dist/index.json` lists every release; each
//! release folder has a `SHASUMS256.txt`. Zenv offers the newest release of
//! every major line (LTS lines marked stable) and installs the Windows x64 zip.
//! Besides the checksum, node.exe must carry the OpenJS Foundation's code
//! signature (the checksum comes from the same server as the zip).

use serde::Deserialize;

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

pub const INDEX: &str = "https://nodejs.org/dist/index.json";
/// Older lines are out of support and not offered.
const OLDEST_MAJOR: u32 = 18;

pub struct NodeProvider;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub version: String,
    /// `false`, or the LTS codename.
    pub lts: serde_json::Value,
    pub files: Vec<String>,
}

fn major(version: &str) -> Option<u32> {
    version.trim_start_matches('v').split('.').next()?.parse().ok()
}

/// Newest Windows x64 release of each major line, newest line first.
pub fn newest_per_major(releases: &[Release]) -> Vec<&Release> {
    let mut out: Vec<&Release> = Vec::new();
    for r in releases.iter().filter(|r| r.files.iter().any(|f| f == "win-x64-zip")) {
        let Some(m) = major(&r.version) else { continue };
        if m >= OLDEST_MAJOR && !out.iter().any(|o| major(&o.version) == Some(m)) {
            out.push(r);
        }
    }
    out
}

/// The SHA-256 of `file` in a `SHASUMS256.txt`.
pub fn sha_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim() == file && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

impl InstallProvider for NodeProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Node
    }

    fn source(&self) -> &'static str {
        "nodejs.org"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["nodejs.org"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let releases: Vec<Release> = http
            .get(INDEX)
            .call()
            .map_err(|e| AppError::Platform(format!("cannot reach nodejs.org: {e}")))?
            .body_mut()
            .read_json()
            .map_err(|e| AppError::Platform(format!("unexpected nodejs.org data: {e}")))?;
        let mut out = Vec::new();
        for r in newest_per_major(&releases) {
            let file = format!("node-{}-win-x64.zip", r.version);
            let sums_url = format!("https://nodejs.org/dist/{}/SHASUMS256.txt", r.version);
            let Ok(mut resp) = http.get(&sums_url).call() else { continue };
            let Ok(sums) = resp.body_mut().read_to_string() else { continue };
            let Some(sha256) = sha_for(&sums, &file) else { continue };
            out.push(AvailableVersion {
                component: ComponentId::Node,
                version: r.version.trim_start_matches('v').to_string(),
                stable: !matches!(r.lts, serde_json::Value::Bool(false)),
                url: format!("https://nodejs.org/dist/{}/{file}", r.version),
                filename: file,
                sha256,
                size: 0,
                source: "nodejs.org",
                installed_id: None,
                verified_by: "",
            });
        }
        Ok(out)
    }

    fn verification(&self) -> &'static str {
        "SHA-256 from nodejs.org and the OpenJS Foundation's code signature on node.exe"
    }

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: None }
    }

    fn post_install(&self, target: &std::path::Path) -> AppResult<()> {
        super::require_signer(&[target.join("node.exe")], "OpenJS Foundation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_newest_release_of_each_supported_line() {
        let json = r#"[
          {"version":"v26.12.0","lts":false,"files":["win-x64-zip","linux-x64"]},
          {"version":"v26.11.1","lts":false,"files":["win-x64-zip"]},
          {"version":"v24.22.0","lts":"Krypton","files":["win-x64-zip"]},
          {"version":"v24.21.0","lts":"Krypton","files":["win-x64-zip"]},
          {"version":"v23.0.0","lts":false,"files":["linux-x64"]},
          {"version":"v16.20.2","lts":"Gallium","files":["win-x64-zip"]}
        ]"#;
        let releases: Vec<Release> = serde_json::from_str(json).unwrap();
        let picked: Vec<&str> = newest_per_major(&releases).iter().map(|r| r.version.as_str()).collect();
        assert_eq!(picked, ["v26.12.0", "v24.22.0"], "no Windows zip for 23, 16 is out of support");
        let sums = "abc  node-v24.22.0-x64.msi\nf9683a19877e12a9b167305c8db71040d812876359de21f0d2e97c3f321f80c3  node-v24.22.0-win-x64.zip\n";
        assert_eq!(
            sha_for(sums, "node-v24.22.0-win-x64.zip").as_deref(),
            Some("f9683a19877e12a9b167305c8db71040d812876359de21f0d2e97c3f321f80c3")
        );
        assert_eq!(sha_for(sums, "node-v24.22.0-win-arm64.zip"), None);
    }
}
