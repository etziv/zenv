//! Web servers from their official Windows sources:
//! - Apache httpd: Apache Lounge (the ASF publishes no Windows binaries and
//!   points to it); each zip has a `.zip.txt` with its SHA-256.
//! - Nginx: nginx/nginx GitHub releases (`nginx-x.y.z.zip` is the Windows build).
//! - Caddy: caddyserver/caddy GitHub releases.
//! - FrankenPHP: php/frankenphp GitHub releases (Windows builds since 1.12).

use super::{github, AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;

// ----- Apache (Apache Lounge) ---------------------------------------------------

pub const APACHE_LOUNGE: &str = "https://www.apachelounge.com/download/";

pub struct ApacheProvider;

/// `/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip` → (path, version).
pub fn apache_zips(html: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for part in html.split("href=\"").skip(1) {
        let Some(href) = part.split('"').next() else { continue };
        let Some(name) = href.rsplit('/').next() else { continue };
        let lower = name.to_ascii_lowercase();
        if !(lower.starts_with("httpd-2.") && lower.contains("-win64-") && lower.ends_with(".zip")) {
            continue;
        }
        let version = name["httpd-".len()..].split('-').next().unwrap_or_default().to_string();
        if !version.is_empty() && !out.iter().any(|(_, v)| *v == version) {
            out.push((href.to_string(), version));
        }
    }
    out
}

/// The SHA-256 line of an Apache Lounge `.zip.txt` checksum file.
pub fn apache_sha256(text: &str) -> Option<String> {
    let mut lines = text.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line.to_ascii_uppercase().starts_with("SHA256-CHECKSUM") {
            let hash = lines.find(|l| !l.is_empty())?.to_ascii_lowercase();
            return (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(hash);
        }
    }
    None
}

impl InstallProvider for ApacheProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Apache
    }

    fn source(&self) -> &'static str {
        "apachelounge.com"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["www.apachelounge.com", "apachelounge.com"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let get = |url: &str| -> AppResult<String> {
            http.get(url)
                .call()
                .map_err(|e| AppError::Platform(format!("cannot reach Apache Lounge: {e}")))?
                .body_mut()
                .read_to_string()
                .map_err(|e| AppError::Platform(format!("unexpected Apache Lounge data: {e}")))
        };
        let html = get(APACHE_LOUNGE)?;
        let mut out = Vec::new();
        for (href, version) in apache_zips(&html) {
            let url = if href.starts_with("http") { href.clone() } else { format!("https://www.apachelounge.com{href}") };
            // Without a published checksum the build is not offered.
            let Some(sha256) = get(&format!("{url}.txt")).ok().as_deref().and_then(apache_sha256) else { continue };
            out.push(AvailableVersion {
                component: ComponentId::Apache,
                version,
                stable: true,
                filename: url.rsplit('/').next().unwrap_or_default().to_string(),
                url,
                sha256,
                size: 0,
                source: "apachelounge.com",
                installed_id: None,
                verified_by: "",
            });
        }
        Ok(out)
    }

    fn unpack(&self) -> Unpack {
        Unpack::Zip { root: Some("Apache24") }
    }
}

// ----- GitHub-released servers ------------------------------------------------------

pub struct NginxProvider;
pub struct CaddyProvider;
pub struct FrankenphpProvider;

/// nginx: even minor versions are the stable branch, odd ones mainline.
pub fn nginx_version(asset: &str) -> Option<(String, bool)> {
    let v = asset.strip_prefix("nginx-")?.strip_suffix(".zip")?;
    let minor: u32 = v.split('.').nth(1)?.parse().ok()?;
    v.split('.').all(|p| p.parse::<u32>().is_ok()).then(|| (v.to_string(), minor % 2 == 0))
}

pub fn caddy_version(asset: &str) -> Option<(String, bool)> {
    let v = asset.strip_prefix("caddy_")?.strip_suffix("_windows_amd64.zip")?;
    Some((v.to_string(), !v.contains('-')))
}

impl InstallProvider for NginxProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Nginx
    }
    fn source(&self) -> &'static str {
        "nginx.org (GitHub releases)"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        github::HOSTS
    }
    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        Ok(github::offers(ComponentId::Nginx, "nginx.org (GitHub releases)", github::releases(http, "nginx/nginx")?, |_, a| {
            nginx_version(&a.name)
        }))
    }
}

impl InstallProvider for CaddyProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Caddy
    }
    fn source(&self) -> &'static str {
        "caddyserver.com (GitHub releases)"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        github::HOSTS
    }
    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        Ok(github::offers(ComponentId::Caddy, "caddyserver.com (GitHub releases)", github::releases(http, "caddyserver/caddy")?, |_, a| {
            caddy_version(&a.name)
        }))
    }
}

impl InstallProvider for FrankenphpProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Frankenphp
    }
    fn source(&self) -> &'static str {
        "frankenphp.dev (GitHub releases)"
    }
    fn allowed_hosts(&self) -> &'static [&'static str] {
        github::HOSTS
    }
    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        Ok(github::offers(ComponentId::Frankenphp, "frankenphp.dev (GitHub releases)", github::releases(http, "php/frankenphp")?, |r, a| {
            (a.name == "frankenphp-windows-x86_64.zip").then(|| {
                let v = r.tag_name.trim_start_matches('v').to_string();
                let stable = !v.contains('-');
                (v, stable)
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_apache_lounge_downloads_and_checksums() {
        let html = r#"<a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip">x</a>
            <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip.txt">sha</a>
            <a href="/download/VS18/binaries/httpd-2.4.69-261002-win32-vs18.zip">32</a>
            <a href="/download/VS17/binaries/httpd-2.4.66-250101-Win64-VS17.zip">old</a>"#;
        assert_eq!(
            apache_zips(html),
            vec![
                ("/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip".to_string(), "2.4.69".to_string()),
                ("/download/VS17/binaries/httpd-2.4.66-250101-Win64-VS17.zip".to_string(), "2.4.66".to_string()),
            ]
        );
        let txt = "\u{feff}Checksums created with GPGHash\r\n\r\nSHA1-Checksum for: x.zip:\r\nD7B5\r\n\r\nSHA256-Checksum for: x.zip:\r\nCA272E0B75ABAD78B1E4029A0753B20FF2232D2D857E8EEBEEA5D469C5776977\r\n";
        assert_eq!(apache_sha256(txt).as_deref(), Some("ca272e0b75abad78b1e4029a0753b20ff2232d2d857e8eebeea5d469c5776977"));
        assert_eq!(apache_sha256("SHA1-Checksum for: x\nabc\n"), None);
    }

    #[test]
    fn reads_github_asset_versions() {
        assert_eq!(nginx_version("nginx-1.30.6.zip"), Some(("1.30.6".into(), true)));
        assert_eq!(nginx_version("nginx-1.31.7.zip"), Some(("1.31.7".into(), false)), "odd minor = mainline");
        assert_eq!(nginx_version("nginx-1.31.7.zip.asc"), None);
        assert_eq!(caddy_version("caddy_2.11.7_windows_amd64.zip"), Some(("2.11.7".into(), true)));
        assert_eq!(caddy_version("caddy_2.12.0-beta.1_windows_amd64.zip"), Some(("2.12.0-beta.1".into(), false)));
        assert_eq!(caddy_version("caddy_2.11.7_linux_amd64.tar.gz"), None);
    }

    #[test]
    fn github_offers_need_a_digest() {
        let releases: Vec<github::Release> = serde_json::from_str(
            r#"[{"tag_name":"v1.13.1","prerelease":false,"draft":false,"assets":[
                 {"name":"frankenphp-windows-x86_64.zip","size":9,"browser_download_url":"https://github.com/x/a.zip","digest":"sha256:AB"},
                 {"name":"frankenphp-linux-x86_64","size":9,"browser_download_url":"https://github.com/x/b","digest":"sha256:CD"}]},
                {"tag_name":"v1.13.0","prerelease":false,"draft":false,"assets":[
                 {"name":"frankenphp-windows-x86_64.zip","size":9,"browser_download_url":"https://github.com/x/c.zip","digest":null}]}]"#,
        )
        .unwrap();
        let offers = github::offers(ComponentId::Frankenphp, "t", releases, |r, a| {
            (a.name == "frankenphp-windows-x86_64.zip").then(|| (r.tag_name.trim_start_matches('v').to_string(), true))
        });
        assert_eq!(offers.iter().map(|o| o.version.as_str()).collect::<Vec<_>>(), ["1.13.1"]);
        assert_eq!(offers[0].sha256, "ab");
    }
}
