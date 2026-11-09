//! Which managed services the apps need, so they can all be started at once.
//!
//! - Web servers: an app's chosen web server, or (for "any running") the one
//!   already running, else the default (Apache when there is one).
//! - Databases and caches: ports named in each app's `.env` (`DB_PORT=3307`,
//!   `REDIS_PORT=6379`, `DATABASE_URL=mysql://…:3306/…`), matched against the
//!   ports of Zenv's services. The file is only read, never changed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::app_core::Core;
use crate::error::AppResult;
use crate::runtime_catalog::{self, Category, ComponentId};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeededService {
    pub service_id: String,
    pub name: String,
    pub kind: Category,
    pub running: bool,
    /// Which apps need it, e.g. "shop.test (DB_PORT=3307)".
    pub used_by: Vec<String>,
    /// Some app can only run on this one (Apache for `.htaccess` rules).
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOutcome {
    pub service_id: String,
    pub name: String,
    /// "started", "already_running", "stopped" or "failed".
    pub result: &'static str,
    pub error: Option<String>,
}

/// Ports an app's `.env` refers to, with the setting that names each one.
pub fn env_ports(project_root: &Path) -> Vec<(u16, String)> {
    let Ok(text) = std::fs::read_to_string(project_root.join(".env")) else { return vec![] };
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim().trim_start_matches("export ").trim();
        let value = value.trim().trim_matches(['"', '\'']);
        if key.ends_with("_PORT") {
            if let Ok(port) = value.parse::<u16>() {
                out.push((port, format!("{key}={port}")));
            }
        } else if let Some(rest) = value.split_once("://").map(|(_, r)| r) {
            // scheme://user:pass@host:port/db
            let host_port = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
            let host_port = host_port.split(['/', '?']).next().unwrap_or_default();
            if let Some(port) = host_port.rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok()) {
                out.push((port, format!("{key} (port {port})")));
            }
        }
    }
    out
}

fn is_local(listen: &str) -> bool {
    matches!(listen, "127.0.0.1" | "0.0.0.0" | "::1" | "::" | "localhost")
}

impl Core {
    /// The services the enabled apps need, web servers last.
    pub fn services_for_apps(&self) -> AppResult<Vec<NeededService>> {
        let services = self.services.list()?;
        let sites = self.sites.enabled_sites()?;
        let mut needed: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut required: std::collections::BTreeSet<String> = Default::default();

        let web: Vec<_> = services
            .iter()
            .filter(|s| runtime_catalog::info(s.component).category == Category::WebServer)
            .collect();
        // "Any running" apps go to the web server already running, else the
        // one other apps chose most often (so one server serves them all),
        // else Apache, else the first.
        let mut chosen: BTreeMap<&str, usize> = BTreeMap::new();
        for s in sites.iter().filter_map(|s| s.service_id.as_deref()) {
            *chosen.entry(s).or_default() += 1;
        }
        let most_chosen = chosen.iter().max_by_key(|(_, n)| **n).map(|(id, _)| *id);
        let apache = web.iter().find(|s| s.component == ComponentId::Apache).map(|s| s.id.clone());
        // Apps with Apache-only `.htaccess` rules can only run on Apache; when
        // one of them follows "any running", Apache becomes the front server
        // (apps that suit FrankenPHP still run their PHP in it, behind Apache).
        let needs_apache = |site: &crate::local_sites::Site| !crate::local_sites::compat::check(site).frankenphp_ready;
        let apache_front = apache.is_some() && sites.iter().any(|s| s.service_id.is_none() && needs_apache(s));
        let fallback_web = if apache_front {
            apache.clone()
        } else {
            web.iter()
                .find(|s| self.services.is_running(&s.id))
                .or_else(|| web.iter().find(|s| Some(s.id.as_str()) == most_chosen))
                .or_else(|| web.iter().find(|s| s.component == ComponentId::Apache))
                .or_else(|| web.first())
                .map(|s| s.id.clone())
        };

        for site in &sites {
            let server = site.service_id.clone().or_else(|| fallback_web.clone());
            if let Some(id) = server {
                if site.service_id.is_none() && needs_apache(site) && Some(&id) == apache.as_ref() {
                    required.insert(id.clone());
                }
                needed.entry(id).or_default().push(site.domain.clone());
            }
            for (port, setting) in env_ports(&site.project_root) {
                for svc in services.iter().filter(|s| {
                    s.http_port == port
                        && is_local(&s.listen_address)
                        && matches!(runtime_catalog::info(s.component).category, Category::Database | Category::Cache)
                }) {
                    needed.entry(svc.id.clone()).or_default().push(format!("{} ({setting})", site.domain));
                }
            }
        }

        let mut out: Vec<NeededService> = services
            .iter()
            .filter_map(|s| {
                let used_by = needed.remove(&s.id)?;
                Some(NeededService {
                    service_id: s.id.clone(),
                    name: s.name.clone(),
                    kind: runtime_catalog::info(s.component).category,
                    running: self.services.is_running(&s.id),
                    required: required.contains(&s.id),
                    used_by,
                })
            })
            .collect();
        out.sort_by_key(|n| n.kind == Category::WebServer);
        Ok(out)
    }

    /// Starts everything the apps need: databases and caches first, then the
    /// web servers (which also start the apps' PHP workers). Web servers on
    /// the same port all run: the first holds the port and passes the others'
    /// apps on, so the one Apache-only apps need (else the busiest) goes first.
    pub fn start_services_for_apps(&self) -> AppResult<Vec<StartOutcome>> {
        let mut needed = self.services_for_apps()?;
        needed.sort_by_key(|n| (n.kind == Category::WebServer, !n.required, std::cmp::Reverse(n.used_by.len())));
        let mut out = Vec::new();
        for n in needed {
            let outcome = if self.services.is_running(&n.service_id) {
                StartOutcome { service_id: n.service_id, name: n.name, result: "already_running", error: None }
            } else {
                match self.start_service(&n.service_id) {
                    Ok(_) => StartOutcome { service_id: n.service_id, name: n.name, result: "started", error: None },
                    Err(e) => StartOutcome { service_id: n.service_id, name: n.name, result: "failed", error: Some(e.to_string()) },
                }
            };
            out.push(outcome);
        }
        // Then the programs behind proxy apps.
        for site in self.autostart_apps(None)? {
            let (result, error) = match self.start_app_process(&site.id) {
                Ok(true) => ("started", None),
                Ok(false) => ("already_running", None),
                Err(e) => ("failed", Some(e.to_string())),
            };
            out.push(StartOutcome { service_id: site.id, name: site.domain, result, error });
        }
        Ok(out)
    }

    /// Stops every running service: web servers first (with the apps' PHP
    /// workers and programs, which depend on them), then databases
    /// and caches.
    /// Databases stop gracefully, as with their own switch.
    pub fn stop_all_services(&self) -> AppResult<Vec<StartOutcome>> {
        let running: Vec<_> = self.services.list()?.into_iter().filter(|s| self.services.is_running(&s.id)).collect();
        let (web, rest): (Vec<_>, Vec<_>) =
            running.into_iter().partition(|s| runtime_catalog::info(s.component).category == Category::WebServer);
        let stop_group = |group: Vec<crate::service_manager::Service>| -> Vec<StartOutcome> {
            std::thread::scope(|scope| {
                let handles: Vec<_> = group
                    .into_iter()
                    .map(|s| {
                        scope.spawn(move || match self.services.stop(&s.id) {
                            Ok(_) => StartOutcome { service_id: s.id, name: s.name, result: "stopped", error: None },
                            Err(e) => StartOutcome { service_id: s.id, name: s.name, result: "failed", error: Some(e.to_string()) },
                        })
                    })
                    .collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            })
        };
        let mut out = stop_group(web);
        self.app_processes.stop_all();
        self.stop_started_containers();
        self.runtimes.shutdown_all();
        out.extend(stop_group(rest));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ports_from_an_env_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "APP_NAME=Shop\nDB_PORT=3307\n# REDIS_PORT=9999\nREDIS_PORT=\"6379\"\nMAIL_PORT=abc\n\
             DATABASE_URL=postgres://user:p%40ss@127.0.0.1:5433/shop?sslmode=disable\nAPP_URL=http://shop.test\n",
        )
        .unwrap();
        let ports = env_ports(dir.path());
        assert_eq!(
            ports,
            vec![(3307, "DB_PORT=3307".into()), (6379, "REDIS_PORT=6379".into()), (5433, "DATABASE_URL (port 5433)".into())]
        );
        assert!(env_ports(&dir.path().join("missing")).is_empty());
    }
}
