//! Virtual-host activation for one web server.
//!
//! 1. Render every enabled site with the server's own serializer into a
//!    staging folder (alongside any non-Zenv `.conf` files already included).
//! 2. Validate a main config that includes the staging folder.
//! 3. Only then sync the real site files through the managed-file ledger
//!    (backups for anything replaced or removed).
//! 4. Reload a running server and confirm it survived; otherwise restore the
//!    previous files and bring the server back.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config_generator::managed_files::{self, Owner};
use crate::error::{AppError, AppResult, IoContext};
use crate::local_sites::{Site, SiteManager, Strategy};
use crate::php_runtime::{self, Backend, FrankenSite, PhpRuntimes};
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager;
use crate::paths::{to_config_path, AppPaths};
use crate::service_manager::ServiceManager;
use crate::storage::Db;
use crate::port_manager;
use crate::web_server::{ServerContext, SiteKind, SiteModel, SiteTls};

pub const OWNER_KIND: &str = "site";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub service_id: String,
    pub sites: usize,
    pub changed_files: usize,
    pub removed_files: usize,
    pub running: bool,
    pub reloaded: bool,
}

pub fn site_model(site: &Site, ctx: &ServerContext, kind: SiteKind, tls: Option<SiteTls>) -> SiteModel {
    SiteModel {
        id: site.id.clone(),
        name: site.name.clone(),
        domain: site.domain.clone(),
        logs_dir: ctx.logs_dir.join("sites"),
        kind,
        tls,
        tailnet_port: None,
        tailnet_hosts: vec![],
        tailnet_only: false,
    }
}

/// PHP backends for every enabled PHP site (all web servers), plus the sites
/// each FrankenPHP installation must serve.
pub struct PhpPlan {
    pub backends: HashMap<String, Backend>,
    pub franken_sites: HashMap<String, (runtime_manager::Installation, Vec<FrankenSite>)>,
    pub needed_php: HashSet<String>,
}

pub fn plan_php(db: &Db, sites: &SiteManager) -> AppResult<PhpPlan> {
    let frankens = php_runtime::frankenphp_installations(db)?;
    let mut plan = PhpPlan { backends: HashMap::new(), franken_sites: HashMap::new(), needed_php: HashSet::new() };
    for site in sites.enabled_php_sites()? {
        let php = runtime_manager::get(db, site.runtime_installation_id.as_deref().unwrap_or_default())?;
        let backend = php_runtime::select(site.php_engine, &php, &frankens)?;
        match &backend {
            Backend::Fastcgi { php } => {
                plan.needed_php.insert(php.id.clone());
            }
            Backend::Frankenphp { franken } => {
                plan.franken_sites.entry(franken.id.clone()).or_insert_with(|| (franken.clone(), vec![])).1.push(FrankenSite {
                    id: site.id.clone(),
                    name: site.name.clone(),
                    domain: site.domain.clone(),
                    document_root: to_config_path(&site.document_root),
                    tailnet: sites.tailnet_port(&site.id).is_some(),
                });
            }
        }
        plan.backends.insert(site.id.clone(), backend);
    }
    Ok(plan)
}

fn site_kind(site: &Site, plan: &PhpPlan, runtimes: &PhpRuntimes, franken_ports: &HashMap<String, u16>) -> AppResult<SiteKind> {
    Ok(match site.strategy {
        Strategy::Static => SiteKind::Static { document_root: site.document_root.clone() },
        Strategy::ReverseProxy => SiteKind::ReverseProxy {
            upstream: SocketAddr::from(([127, 0, 0, 1], site.upstream_port.unwrap_or_default())),
        },
        Strategy::Php => match plan.backends.get(&site.id) {
            Some(Backend::Fastcgi { php }) => {
                SiteKind::PhpFastcgi { document_root: site.document_root.clone(), ports: runtimes.ensure_pool(php)? }
            }
            Some(Backend::Frankenphp { franken }) => SiteKind::ReverseProxy {
                upstream: SocketAddr::from(([127, 0, 0, 1], franken_ports.get(&franken.id).copied().unwrap_or_default())),
            },
            None => return Err(AppError::Validation(format!("site {} has no PHP version", site.domain))),
        },
    })
}

fn site_file(dir: &Path, site_id: &str) -> PathBuf {
    dir.join(format!("{site_id}.conf"))
}

fn conf_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("conf")))
                .collect()
        })
        .unwrap_or_default()
}

pub fn apply_service(
    db: &Db,
    paths: &AppPaths,
    services: &ServiceManager,
    sites: &SiteManager,
    runtimes: &PhpRuntimes,
    service_id: &str,
) -> AppResult<ApplyReport> {
    let (service, install, adapter) = services.load(service_id)?;
    if !adapter.supports_sites() {
        return Ok(ApplyReport { service_id: service_id.to_string(), sites: 0, changed_files: 0, removed_files: 0, running: false, reloaded: false });
    }
    let ctx = services.context(&service)?;
    let mut enabled = sites.enabled_for_service(service_id)?;
    // Web servers running behind this one (sharing its port) get their own
    // apps' requests passed on, host name intact.
    let mut forward: HashMap<String, u16> = HashMap::new();
    let guests = services.guests_of(service_id);
    for (guest, port) in &guests {
        for site in sites.enabled_bound_to(guest)? {
            forward.insert(site.id.clone(), *port);
            enabled.push(site);
        }
    }
    // "Any server" apps with Apache-only .htaccess rules go to an Apache
    // running behind a non-Apache front.
    let apache_guest = guests.iter().find(|(g, _)| services.get(g).is_ok_and(|s| s.component == ComponentId::Apache)).map(|(_, p)| *p);
    if let (Some(port), false) = (apache_guest, service.component == ComponentId::Apache) {
        for site in enabled.iter().filter(|s| s.service_id.is_none() && !crate::local_sites::compat::check(s).frankenphp_ready) {
            forward.insert(site.id.clone(), port);
        }
    }
    // Behind another server, which terminates HTTPS for these apps.
    let behind = services.hosted(service_id).is_some();
    // The server "any server" apps are served by now (the running one, else the first).
    let any_server = sites.serving_service_id()?;
    // Adding or dropping the HTTPS listener needs a restart instead of a
    // reload; check the TLS port is free before anything changes.
    let running = services.is_running(service_id);
    let https_changes = running && ctx.https.is_some() != services.active_https(service_id);
    if let (true, Some(https)) = (https_changes, &ctx.https) {
        port_manager::ensure_available(https.port, ctx.listen_address)?;
    }
    let real_dir = ctx.sites_dir.clone();
    let staged_dir = paths.service_generated_dir(service_id).join("sites.staged");
    for d in [&real_dir, &ctx.logs_dir.join("sites")] {
        std::fs::create_dir_all(d).ctx(|| format!("creating {}", d.display()))?;
    }
    match std::fs::remove_dir_all(&staged_dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).ctx(|| format!("clearing {}", staged_dir.display()))
        }
        _ => {}
    }
    std::fs::create_dir_all(&staged_dir).ctx(|| format!("creating {}", staged_dir.display()))?;

    // 0. PHP backends: start/refresh the php-cgi pools and FrankenPHP
    //    instances the enabled sites need, so their ports are known.
    let php_plan = plan_php(db, sites)?;
    let mut franken_ports = HashMap::new();
    for (id, (franken, list)) in &php_plan.franken_sites {
        franken_ports.insert(id.clone(), runtimes.ensure_franken(franken, list)?);
    }

    // 1. Render into staging.
    let mut desired: Vec<(PathBuf, String, String)> = Vec::with_capacity(enabled.len());
    let mut tailnet_hosts: Option<Vec<String>> = None;
    for site in &enabled {
        let forwarded = forward.contains_key(&site.id);
        let kind = match forward.get(&site.id) {
            Some(port) => SiteKind::ReverseProxy { upstream: SocketAddr::from(([127, 0, 0, 1], *port)) },
            None => site_kind(site, &php_plan, runtimes, &franken_ports)?,
        };
        let tls = if site.https_enabled && !behind {
            if ctx.https.is_none() {
                return Err(AppError::Conflict(format!(
                    "{} uses HTTPS: create Zenv's local certificate authority in Settings and give '{}' an HTTPS port",
                    site.domain, service.name
                )));
            }
            let files = services.tls().ensure_cert(std::slice::from_ref(&site.domain))?;
            Some(SiteTls { cert: files.cert, key: files.key, redirect: site.https_redirect })
        } else {
            None
        };
        // Only the server that answers for the app on its own port listens
        // for its tailnet visitors (never one running behind another).
        let answers = !behind
            && (forward.contains_key(&site.id)
                || site.service_id.as_deref() == Some(service_id)
                || (site.service_id.is_none() && any_server.as_deref() == Some(service_id)));
        let tailnet_port = if answers { sites.tailnet_port(&site.id) } else { None };
        let hosts = match tailnet_port {
            Some(_) => {
                let machine = tailnet_hosts.get_or_insert_with(|| crate::tailnet::known_hosts(db)).clone();
                crate::tailnet::hosts_for(db, &site.id, &machine)
            }
            None => vec![],
        };
        let text = if forwarded && tailnet_port.is_some() {
            // The server this app is forwarded to picks apps by domain, which
            // tailnet visitors do not use (they come as desktop-x:9001), so
            // this server serves them the app itself, the same way: php-cgi,
            // its FrankenPHP backend (routed by X-Zenv-Site) or its program.
            let mut text = adapter.render_site(&install, &ctx, &site_model(site, &ctx, kind, tls))?;
            let direct = SiteModel {
                tailnet_port,
                tailnet_hosts: hosts,
                tailnet_only: true,
                ..site_model(site, &ctx, site_kind(site, &php_plan, runtimes, &franken_ports)?, None)
            };
            text.push('\n');
            text.push_str(&adapter.render_site(&install, &ctx, &direct)?);
            text
        } else {
            adapter.render_site(&install, &ctx, &SiteModel { tailnet_port, tailnet_hosts: hosts, ..site_model(site, &ctx, kind, tls) })?
        };
        std::fs::write(site_file(&staged_dir, &site.id), &text).ctx(|| format!("staging site {}", site.domain))?;
        desired.push((site_file(&real_dir, &site.id), text, site.id.clone()));
    }
    let managed_here: HashSet<PathBuf> = managed_files::paths_under(db, OWNER_KIND, &real_dir)?.into_iter().collect();
    for foreign in conf_files(&real_dir).into_iter().filter(|p| !managed_here.contains(p)) {
        let target = staged_dir.join(foreign.file_name().unwrap_or_default());
        std::fs::copy(&foreign, &target).ctx(|| format!("staging {}", foreign.display()))?;
    }

    // 2. Validate with the server's own checker.
    let report = services.check_with_sites_dir(&service, &install, adapter, &staged_dir)?;
    if !report.ok {
        let output = report.output.replace(&to_config_path(&staged_dir), &to_config_path(&real_dir));
        // Nginx prints an alert about its log file before the real error.
        let lines = || output.lines().map(str::trim).filter(|l| !l.is_empty());
        let message = lines().find(|l| l.contains("[emerg]")).or_else(|| lines().next()).unwrap_or("check failed").to_string();
        return Err(AppError::ConfigInvalid { message, output });
    }
    let _ = std::fs::remove_dir_all(&staged_dir);

    // 3. Sync real files, remembering what was there for rollback.
    let desired_paths: HashSet<PathBuf> = desired.iter().map(|(p, _, _)| p.clone()).collect();
    let snapshot: Vec<(PathBuf, Option<String>)> = desired_paths
        .iter()
        .chain(managed_here.iter())
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|p| (p.clone(), std::fs::read_to_string(p).ok()))
        .collect();
    let mut changed_files = 0;
    for (path, text, site_id) in &desired {
        if managed_files::write_managed(db, &paths.backups_dir, path, text, Owner { kind: OWNER_KIND, id: site_id })?.changed {
            changed_files += 1;
        }
    }
    let mut removed_files = 0;
    for stale in managed_here.iter().filter(|p| !desired_paths.contains(*p)) {
        managed_files::remove_managed(db, &paths.backups_dir, stale)?;
        removed_files += 1;
    }

    // 4. Reload when something changed, and verify.
    let mut reloaded = false;
    if running && (changed_files + removed_files > 0 || https_changes) {
        let outcome = if https_changes { services.restart(service_id).map(|_| ()) } else { services.reload_checked(service_id) };
        if let Err(e) = outcome {
            restore(db, paths, &snapshot)?;
            if !services.is_running(service_id) {
                if let Err(start_err) = services.start(service_id) {
                    tracing::error!(service = service_id, error = %start_err, "restart after rollback failed");
                }
            }
            return Err(AppError::StartFailed {
                message: format!(
                    "'{}' did not survive reloading the new site configuration, so the previous configuration was restored: {e}",
                    service.name
                ),
                log_tail: vec![],
            });
        }
        reloaded = true;
    }

    // Stop PHP workers and FrankenPHP instances no enabled site uses any more.
    runtimes.retain(&php_plan.needed_php, &php_plan.franken_sites.keys().cloned().collect());

    sites.mark_applied(
        service_id,
        if running { "active" } else { "saved; takes effect when the web server starts" },
    )?;
    Ok(ApplyReport { service_id: service_id.to_string(), sites: enabled.len(), changed_files, removed_files, running, reloaded })
}

fn restore(db: &Db, paths: &AppPaths, snapshot: &[(PathBuf, Option<String>)]) -> AppResult<()> {
    for (path, previous) in snapshot {
        match previous {
            Some(text) => {
                let site_id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                managed_files::write_managed(db, &paths.backups_dir, path, text, Owner { kind: OWNER_KIND, id: &site_id })?;
            }
            None => managed_files::remove_managed(db, &paths.backups_dir, path)?,
        }
    }
    Ok(())
}
