//! Opening apps on the user's Tailscale network.
//!
//! Devices signed in to the same Tailscale account reach an app at
//! `http://<this computer's name or tailnet IP>:<port>`; nothing is reachable
//! from outside the tailnet. `tailscale serve --tcp=<port>` (no administrator
//! rights needed) passes those connections unchanged to Zenv's web server on
//! `127.0.0.1:<port>`. TCP mode works for the IP as well as the names (HTTP
//! mode matches names only). The traffic is encrypted by Tailscale itself, so
//! plain HTTP is used.
//!
//! On that port:
//! - Only this computer's tailnet names and addresses are accepted as the
//!   host name. Any other one means a web page elsewhere pointed its own name
//!   at this port (DNS rebinding) to use the app through a visitor's browser.
//! - Visitors arrive from 127.0.0.1, which apps take for this computer (and
//!   then open "local only" pages). They are given [`PEER`] instead: PHP's
//!   REMOTE_ADDR, and X-Forwarded-For / X-Real-IP for proxy apps.
//!
//! An app may instead get its own name as a Tailscale Service:
//! `https://<service>.<tailnet>.ts.net`, with a certificate from Tailscale
//! (`tailscale serve --service=svc:<name> --https=443`, to the same loopback
//! port). Tailscale requires the computer hosting a Service to be a tagged
//! device, HTTPS certificates on in the tailnet, and the Service defined (and
//! this host approved) in its admin console.

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::storage::Db;
use crate::util::now_rfc3339;

/// Where Zenv starts looking for tailnet ports.
const PORT_BASE: u16 = 9001;

/// The address apps see for a tailnet visitor: the first address of
/// Tailscale's range, which no device gets. Not loopback, so not "local".
pub const PEER: &str = "100.64.0.0";

/// This computer's tailnet names and addresses, space-separated, kept so the
/// web servers' configuration does not depend on Tailscale running.
const HOSTS_KEY: &str = "tailnet.hosts";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailnetStatus {
    pub installed: bool,
    /// Signed in and connected.
    pub online: bool,
    /// Short MagicDNS name, e.g. "my-laptop".
    pub host: Option<String>,
    /// Full name, e.g. "my-laptop.tail1234.ts.net".
    pub dns_name: Option<String>,
    pub ip: Option<String>,
    /// Every tailnet address (IPv4 and IPv6).
    #[serde(skip)]
    pub ips: Vec<String>,
    /// Why it cannot be used, when it cannot.
    pub problem: Option<String>,
    /// This computer's tags (Services need a tagged host).
    pub tags: Vec<String>,
    /// HTTPS certificates are on in the tailnet (Services need them).
    pub https: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailnetApp {
    pub site_id: String,
    pub port: u16,
    /// `http://my-laptop:9001`
    pub url: Option<String>,
    /// `http://my-laptop.tail1234.ts.net:9001`
    pub full_url: Option<String>,
    /// `http://100.101.102.103:9001`
    pub ip_url: Option<String>,
    /// Its Tailscale Service, e.g. "vaultwarden".
    pub service: Option<String>,
    /// `https://vaultwarden.tail1234.ts.net`
    pub service_url: Option<String>,
}

#[derive(Deserialize)]
struct StatusJson {
    #[serde(rename = "BackendState")]
    backend_state: Option<String>,
    #[serde(rename = "Self")]
    me: Option<SelfJson>,
    #[serde(rename = "CertDomains")]
    cert_domains: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SelfJson {
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    ips: Option<Vec<String>>,
    #[serde(rename = "Online")]
    online: Option<bool>,
    #[serde(rename = "Tags")]
    tags: Option<Vec<String>>,
}

/// A Service name as Tailscale takes it after `svc:`: a DNS label.
pub fn valid_service(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// "tail1234.ts.net" from this computer's full name.
fn tailnet_domain(hosts: &[String]) -> Option<&str> {
    hosts.iter().find(|h| h.ends_with(".ts.net")).and_then(|h| h.split_once('.')).map(|(_, rest)| rest)
}

/// A Service's full name, e.g. "vaultwarden.tail1234.ts.net".
pub fn service_name(service: &str, hosts: &[String]) -> Option<String> {
    tailnet_domain(hosts).map(|d| format!("{service}.{d}"))
}

/// The Tailscale command-line tool (on PATH or in its usual folder).
pub fn tailscale_exe() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        if let Some(p) = std::env::split_paths(&path).map(|d| d.join("tailscale.exe")).find(|p| p.is_file()) {
            return Some(p);
        }
    }
    ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join("Tailscale").join("tailscale.exe"))
        .find(|p| p.is_file())
}

fn run(args: &[&str]) -> AppResult<String> {
    let exe = tailscale_exe().ok_or_else(|| AppError::Unsupported("Tailscale is not installed".into()))?;
    let out = crate::runtime_manager::probe::run_capture(&exe, &args.iter().map(std::ffi::OsString::from).collect::<Vec<_>>(), Duration::from_secs(20))?;
    if out.code != Some(0) {
        return Err(AppError::Platform(format!("tailscale {}: {}", args.join(" "), out.text.trim())));
    }
    Ok(out.text)
}

pub fn status() -> TailnetStatus {
    let mut st = TailnetStatus { installed: tailscale_exe().is_some(), online: false, host: None, dns_name: None, ip: None, ips: vec![], problem: None, tags: vec![], https: false };
    if !st.installed {
        st.problem = Some("Tailscale is not installed on this computer.".into());
        return st;
    }
    let json = match run(&["status", "--json"]) {
        Ok(text) => text,
        Err(e) => {
            st.problem = Some(format!("Tailscale is not running: {e}"));
            return st;
        }
    };
    let parsed: Option<StatusJson> = serde_json::from_str(json.trim_start_matches(|c: char| c != '{')).ok();
    let Some(parsed) = parsed else {
        st.problem = Some("Tailscale's status could not be read.".into());
        return st;
    };
    st.https = parsed.cert_domains.as_ref().is_some_and(|d| !d.is_empty());
    if let Some(me) = parsed.me {
        st.tags = me.tags.unwrap_or_default();
        st.dns_name = me.dns_name.map(|d| d.trim_end_matches('.').to_ascii_lowercase()).filter(|d| !d.is_empty());
        st.host = st.dns_name.as_deref().and_then(|d| d.split('.').next()).map(str::to_string);
        st.ips = me.ips.unwrap_or_default();
        st.ip = st.ips.iter().find(|i| i.contains('.')).cloned();
        st.online = me.online.unwrap_or(false) && parsed.backend_state.as_deref() == Some("Running");
    }
    if !st.online {
        st.problem = Some("Tailscale is not connected; sign in and turn it on.".into());
    }
    st
}

/// What a tailnet visitor's Host header may name (port aside): the short and
/// full MagicDNS names and every tailnet address (IPv6 without brackets).
pub fn hosts_of(st: &TailnetStatus) -> Vec<String> {
    let mut out: Vec<String> = st.host.iter().chain(st.dns_name.iter()).chain(st.ips.iter()).map(|h| h.to_ascii_lowercase()).collect();
    out.retain(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')));
    out.dedup();
    out
}

/// Keeps this computer's tailnet names for the web servers' configuration.
fn remember_hosts(db: &Db, st: &TailnetStatus) {
    if !st.online {
        return;
    }
    let hosts = hosts_of(st).join(" ");
    let saved = db.write(|tx| {
        tx.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            params![HOSTS_KEY, hosts, now_rfc3339()],
        )?;
        Ok(())
    });
    if let Err(e) = saved {
        tracing::warn!(error = %e, "tailnet names not saved");
    }
}

/// This computer's tailnet names as last seen (asks Tailscale the first time).
/// Empty when they were never known: then no host name is accepted.
pub fn known_hosts(db: &Db) -> Vec<String> {
    let saved: Option<String> = db
        .read(|c| Ok(rusqlite::OptionalExtension::optional(c.query_row("SELECT value FROM app_settings WHERE key = ?1", [HOSTS_KEY], |r| r.get(0)))?))
        .ok()
        .flatten();
    match saved {
        Some(v) => v.split_whitespace().map(str::to_string).collect(),
        None => {
            let st = status();
            remember_hosts(db, &st);
            if st.online { hosts_of(&st) } else { vec![] }
        }
    }
}

/// The host names as a PCRE alternation (`desktop-x|desktop-x\.tail1\.ts\.net|\[fd7a::1\]`),
/// IPv6 in brackets as in a Host header; one that never matches when empty.
pub fn host_pattern(hosts: &[String]) -> String {
    if hosts.is_empty() {
        return "(?!)".into();
    }
    hosts
        .iter()
        .map(|h| {
            let e = h.replace('.', "\\.");
            if h.contains(':') { format!("\\[{e}\\]") } else { e }
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// The names a site's tailnet port accepts: this computer's, and the app's
/// own Service name when it has one.
pub fn hosts_for(db: &Db, site_id: &str, machine: &[String]) -> Vec<String> {
    let mut hosts = machine.to_vec();
    let service: Option<String> = db
        .read(|c| Ok(rusqlite::OptionalExtension::optional(c.query_row("SELECT service FROM tailnet_apps WHERE site_id = ?1", [site_id], |r| r.get(0)))?))
        .ok()
        .flatten()
        .flatten();
    if let Some(name) = service.and_then(|s| service_name(&s, machine)) {
        hosts.push(name);
    }
    hosts
}

fn app(site_id: &str, port: u16, service: Option<String>, st: &TailnetStatus) -> TailnetApp {
    let hosts = hosts_of(st);
    if let Some(svc) = service {
        let url = service_name(&svc, &hosts).map(|n| format!("https://{n}"));
        return TailnetApp { site_id: site_id.to_string(), port, url: url.clone(), full_url: url.clone(), ip_url: None, service: Some(svc), service_url: url };
    }
    TailnetApp {
        site_id: site_id.to_string(),
        port,
        url: st.host.as_ref().map(|h| format!("http://{h}:{port}")),
        full_url: st.dns_name.as_ref().map(|d| format!("http://{d}:{port}")),
        ip_url: st.ip.as_ref().map(|ip| format!("http://{ip}:{port}")),
        service: None,
        service_url: None,
    }
}

/// Why this computer can't host a Tailscale Service yet, if it can't.
pub fn service_problem(st: &TailnetStatus) -> Option<String> {
    if let Some(p) = &st.problem {
        return Some(p.clone());
    }
    if st.tags.is_empty() {
        return Some("Tailscale Services need this computer to be a tagged device (for example tag:server).".into());
    }
    if !st.https {
        return Some("Turn on HTTPS certificates in the Tailscale admin console (DNS page) first.".into());
    }
    None
}

impl Core {
    pub fn tailnet_status(&self) -> TailnetStatus {
        status()
    }

    /// Apps opened on the tailnet.
    pub fn tailnet_apps(&self) -> AppResult<Vec<TailnetApp>> {
        let rows: Vec<(String, u16, Option<String>)> = self.db.read(|c| {
            let mut stmt = c.prepare("SELECT site_id, port, service FROM tailnet_apps ORDER BY port")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })?;
        let st = status();
        if !rows.is_empty() && st.online && hosts_of(&st) != known_hosts(&self.db) {
            // The names changed (renamed device, new tailnet): serve the new ones.
            remember_hosts(&self.db, &st);
            for id in self.services.web_servers()?.into_iter().map(|s| s.id) {
                if let Err(e) = self.sites.reapply(&id) {
                    tracing::warn!(service = %id, error = %e, "tailnet names not applied");
                }
            }
        }
        Ok(rows.into_iter().map(|(id, port, service)| app(&id, port, service, &st)).collect())
    }

    /// Web servers whose configuration includes `site_id` (re-applied after a change).
    fn servers_of(&self, site_id: &str) -> AppResult<Vec<String>> {
        let site = self.sites.get(site_id)?;
        let mut ids = match &site.service_id {
            Some(id) => vec![id.clone()],
            None => self.services.web_servers()?.into_iter().map(|s| s.id).collect(),
        };
        // The server in front of a server running behind it forwards its apps.
        let fronts: Vec<String> = ids.iter().filter_map(|id| self.services.hosted(id).map(|(f, _)| f)).collect();
        ids.extend(fronts);
        Ok(ids)
    }

    /// Opens an app on the tailnet: a loopback port on its web server, and
    /// `tailscale serve` from the same port on this computer's tailnet name,
    /// or as the Tailscale Service `service` (its own https name).
    pub fn tailnet_open(&self, site_id: &str, service: Option<&str>) -> AppResult<TailnetApp> {
        let st = status();
        if let Some(problem) = st.problem.clone() {
            return Err(AppError::Conflict(problem));
        }
        if let Some(svc) = service {
            if !valid_service(svc) {
                return Err(AppError::Validation(format!("\"{svc}\" is not a Service name: use lower-case letters, digits and hyphens")));
            }
            if let Some(problem) = service_problem(&st) {
                return Err(AppError::Conflict(problem));
            }
            let taken: Option<String> = self.db.read(|c| {
                Ok(rusqlite::OptionalExtension::optional(c.query_row(
                    "SELECT site_id FROM tailnet_apps WHERE service = ?1 AND site_id != ?2",
                    params![svc, site_id],
                    |r| r.get(0),
                ))?)
            })?;
            if taken.is_some() {
                return Err(AppError::Conflict(format!("another app already uses the Service name {svc}")));
            }
        }
        // Switching between this computer's name and a Service: undo the old one first.
        if self.sites.tailnet_port(site_id).is_some() && self.tailnet_service(site_id).as_deref() != service {
            self.tailnet_close(site_id)?;
        }
        let site = self.sites.get(site_id)?;
        if !site.enabled {
            return Err(AppError::Conflict(format!("{} is switched off", site.domain)));
        }
        remember_hosts(&self.db, &st);
        let port = match self.sites.tailnet_port(site_id) {
            Some(p) => p,
            None => {
                let taken: Vec<u16> = self.tailnet_apps()?.into_iter().map(|a| a.port).collect();
                let mut from = PORT_BASE;
                loop {
                    let p = crate::app_process::free_port(self, from, None)?;
                    if !taken.contains(&p) {
                        break p;
                    }
                    from = p + 1;
                }
            }
        };
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO tailnet_apps (site_id, port, service, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(site_id) DO UPDATE SET port = ?2, service = ?3",
                params![site_id, port, service, now_rfc3339()],
            )?;
            Ok(())
        })?;
        let applied = (|| {
            for id in self.servers_of(site_id)? {
                self.sites.reapply(&id)?;
            }
            match service {
                Some(svc) => run(&["serve", &format!("--service=svc:{svc}"), "--https=443", &format!("http://127.0.0.1:{port}")]),
                None => run(&["serve", "--bg", &format!("--tcp={port}"), &format!("tcp://127.0.0.1:{port}")]),
            }
        })();
        if let Err(e) = applied {
            let _ = self.tailnet_close(site_id);
            return Err(e);
        }
        tracing::info!(domain = %site.domain, port, service = ?service, "app opened on the tailnet");
        Ok(app(site_id, port, service.map(str::to_string), &st))
    }

    fn tailnet_service(&self, site_id: &str) -> Option<String> {
        self.db
            .read(|c| Ok(rusqlite::OptionalExtension::optional(c.query_row("SELECT service FROM tailnet_apps WHERE site_id = ?1", [site_id], |r| r.get(0)))?))
            .ok()
            .flatten()
            .flatten()
    }

    /// Closes an app on the tailnet (also used when the app is removed).
    pub fn tailnet_close(&self, site_id: &str) -> AppResult<()> {
        let Some(port) = self.sites.tailnet_port(site_id) else { return Ok(()) };
        let service = self.tailnet_service(site_id);
        if tailscale_exe().is_some() {
            // Fails harmlessly when Tailscale has no such entry any more.
            let undone = match &service {
                Some(svc) => run(&["serve", "clear", &format!("svc:{svc}")]),
                None => run(&["serve", &format!("--tcp={port}"), "off"]),
            };
            if let Err(e) = undone {
                tracing::warn!(port, service = ?service, error = %e, "tailscale serve entry not removed");
            }
        }
        self.db.write(|tx| {
            tx.execute("DELETE FROM tailnet_apps WHERE site_id = ?1", [site_id])?;
            Ok(())
        })?;
        if self.sites.get(site_id).is_ok() {
            for id in self.servers_of(site_id)? {
                self.sites.reapply(&id)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> TailnetStatus {
        TailnetStatus {
            installed: true,
            online: true,
            host: Some("my-laptop".into()),
            dns_name: Some("my-laptop.tail1234.ts.net".into()),
            ip: Some("100.101.102.103".into()),
            ips: vec!["100.101.102.103".into(), "fd7a:115c:a1e0::1".into()],
            problem: None,
            tags: vec![],
            https: false,
        }
    }

    #[test]
    fn urls_use_the_magicdns_names() {
        let a = app("s1", 9001, None, &example());
        assert_eq!(a.url.as_deref(), Some("http://my-laptop:9001"));
        assert_eq!(a.full_url.as_deref(), Some("http://my-laptop.tail1234.ts.net:9001"));
        assert_eq!(a.ip_url.as_deref(), Some("http://100.101.102.103:9001"));
    }

    #[test]
    fn only_this_computers_tailnet_names_are_accepted() {
        let hosts = hosts_of(&example());
        assert_eq!(hosts, ["my-laptop", "my-laptop.tail1234.ts.net", "100.101.102.103", "fd7a:115c:a1e0::1"]);
        let re = regex::Regex::new(&format!("(?i)^({})(:[0-9]+)?$", host_pattern(&hosts))).unwrap();
        for ok in ["my-laptop:9001", "MY-LAPTOP.tail1234.ts.net:9001", "100.101.102.103:9001", "[fd7a:115c:a1e0::1]:9001", "my-laptop"] {
            assert!(re.is_match(ok), "{ok}");
        }
        for rebinding in ["evil.example:9001", "my-laptopXtail1234.ts.net", "100.101.102.1033", "localhost:9001", "127.0.0.1:9001"] {
            assert!(!re.is_match(rebinding), "{rebinding}");
        }
        assert_eq!(host_pattern(&[]), "(?!)");
    }

    #[test]
    fn a_service_has_its_own_https_name() {
        let a = app("s1", 9001, Some("vaultwarden".into()), &example());
        assert_eq!(a.service_url.as_deref(), Some("https://vaultwarden.tail1234.ts.net"));
        assert_eq!(a.url, a.service_url);
        assert!(a.ip_url.is_none());
        let mut hosts = hosts_of(&example());
        hosts.push(service_name("vaultwarden", &hosts).unwrap());
        let re = regex::Regex::new(&format!("(?i)^({})(:[0-9]+)?$", host_pattern(&hosts))).unwrap();
        assert!(re.is_match("vaultwarden.tail1234.ts.net"));
        assert!(!re.is_match("other.tail1234.ts.net"));
    }

    #[test]
    fn service_names_are_dns_labels() {
        for ok in ["vaultwarden", "wa-blast", "app2"] {
            assert!(valid_service(ok), "{ok}");
        }
        for bad in ["", "Vault", "-x", "x-", "a.b", "svc:x", "a b", &"x".repeat(64)] {
            assert!(!valid_service(bad), "{bad}");
        }
    }

    #[test]
    fn services_need_a_tag_and_https() {
        let mut st = example();
        assert!(service_problem(&st).unwrap().contains("tagged"));
        st.tags = vec!["tag:server".into()];
        assert!(service_problem(&st).unwrap().contains("HTTPS"));
        st.https = true;
        assert!(service_problem(&st).is_none());
    }
}
