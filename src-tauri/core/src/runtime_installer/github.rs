//! Releases published on GitHub. Since 2025 the API reports a SHA-256
//! `digest` for every release asset, which Zenv verifies downloads against;
//! assets without one are not offered.

use serde::Deserialize;

use super::AvailableVersion;
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

/// Hosts a GitHub release download goes through (redirects included).
pub const HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

#[derive(Debug, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub tag_name: String,
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

pub fn releases(http: &ureq::Agent, repo: &str) -> AppResult<Vec<Release>> {
    http.get(&format!("https://api.github.com/repos/{repo}/releases?per_page=40"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| AppError::Platform(format!("cannot reach GitHub releases of {repo}: {e}")))?
        .body_mut()
        .read_json()
        .map_err(|e| AppError::Platform(format!("unexpected release data from {repo}: {e}")))
}

/// One offer per version: the first asset `pick` accepts (returning the
/// version and whether it is a stable release), newest release first.
pub fn offers(
    component: ComponentId,
    source: &'static str,
    releases: Vec<Release>,
    pick: impl Fn(&Release, &Asset) -> Option<(String, bool)>,
) -> Vec<AvailableVersion> {
    let mut out: Vec<AvailableVersion> = Vec::new();
    for r in releases.iter().filter(|r| !r.draft) {
        for a in &r.assets {
            let Some((version, stable)) = pick(r, a) else { continue };
            let Some(sha) = a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else { continue };
            if out.iter().any(|v| v.version == version) {
                continue;
            }
            out.push(AvailableVersion {
                component,
                version,
                stable: stable && !r.prerelease,
                filename: a.name.clone(),
                url: a.browser_download_url.clone(),
                sha256: sha.to_ascii_lowercase(),
                size: a.size,
                source,
                installed_id: None,
                verified_by: "",
            });
        }
    }
    let key = |v: &str| v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse::<u32>().ok()).collect::<Vec<_>>();
    out.sort_by(|a, b| key(&b.version).cmp(&key(&a.version)));
    out
}
