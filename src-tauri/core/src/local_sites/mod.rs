//! Local sites: persistent records mapping a domain to a project folder and a
//! serving strategy on one managed web server.
//!
//! Every mutation is applied together with its web-server configuration: if
//! the configuration cannot be validated and activated, the record change is
//! rolled back. Removing a site never touches the project's files.

pub mod compat;
pub mod detect;
pub mod logs;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::config_generator::safe_value;
use crate::diagnostics::{check, Check, CheckStatus};
use crate::domain_manager::hosts::{self, ApplyOutcome, DomainState, HostsEntry, HostsPlan};
use crate::domain_manager::{self, Resolution};
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use crate::php_runtime::{self, Backend, PhpRuntimes};
use crate::port_manager::{self, PortStatus};
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, Installation};
use crate::service_manager::{Health, ServiceManager};
use crate::storage::Db;
use crate::util::{new_id, now_rfc3339};
use crate::virtual_host::{self, ApplyReport};

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $s),+ } }
        }
        impl FromStr for $name {
            type Err = AppError;
            fn from_str(s: &str) -> Result<Self, AppError> {
                match s { $($s => Ok(Self::$variant),)+ other => Err(AppError::Validation(format!("unknown {} '{other}'", stringify!($name)))) }
            }
        }
    };
}

str_enum!(ProjectType {
    Laravel => "laravel", Php => "php", Static => "static", Go => "go",
    Rust => "rust", Python => "python", Ruby => "ruby", Custom => "custom",
});
str_enum!(Strategy { Static => "static", ReverseProxy => "reverse_proxy", Php => "php" });
str_enum!(PhpEngine { Auto => "auto", Fastcgi => "fastcgi", Frankenphp => "frankenphp" });
str_enum!(ConfigStatus { Pending => "pending", Applied => "applied", Failed => "failed", Disabled => "disabled" });

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub project_root: PathBuf,
    pub document_root: PathBuf,
    pub project_type: ProjectType,
    /// `None`: served by whichever managed web server is running.
    pub service_id: Option<String>,
    pub strategy: Strategy,
    pub upstream_port: Option<u16>,
    pub runtime_installation_id: Option<String>,
    pub php_engine: PhpEngine,
    pub https_enabled: bool,
    /// Plain HTTP requests are redirected to HTTPS.
    pub https_redirect: bool,
    pub enabled: bool,
    pub config_status: ConfigStatus,
    pub config_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInput {
    pub name: String,
    pub domain: String,
    pub project_root: String,
    pub document_root: String,
    pub project_type: ProjectType,
    /// `None`: served by whichever managed web server is running.
    #[serde(default)]
    pub service_id: Option<String>,
    pub strategy: Strategy,
    pub upstream_port: Option<u16>,
    pub runtime_installation_id: Option<String>,
    #[serde(default = "default_engine")]
    pub php_engine: PhpEngine,
    #[serde(default)]
    pub https_enabled: bool,
    #[serde(default)]
    pub https_redirect: bool,
    pub enabled: bool,
}

fn default_engine() -> PhpEngine {
    PhpEngine::Auto
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteView {
    #[serde(flatten)]
    pub site: Site,
    pub service_name: String,
    pub url: String,
    pub resolution: Resolution,
    pub hosts_state: DomainState,
    pub document_root_exists: bool,
    /// PHP sites: the PHP version and where it runs ("fastcgi" or "frankenphp").
    pub php_version: Option<String>,
    pub php_backend: Option<&'static str>,
    /// Whether FrankenPHP/Caddy/Nginx can serve it, or it needs Apache.
    pub compat: compat::ServerCompat,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteQuery {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePage {
    pub items: Vec<SiteView>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsStatus {
    pub path: PathBuf,
    pub plan: HostsPlan,
}

const COLUMNS: &str = "s.id, s.name, s.domain, s.project_root, s.document_root, s.project_type, s.service_id, \
    s.strategy, s.upstream_port, s.runtime_installation_id, s.https_enabled, s.enabled, s.config_status, \
    s.config_message, s.created_at, s.updated_at, s.php_engine, s.https_redirect";

fn parse<T: FromStr<Err = AppError>>(idx: usize, s: String) -> rusqlite::Result<T> {
    T::from_str(&s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e)))
}

fn site_from_row(r: &Row<'_>) -> rusqlite::Result<Site> {
    Ok(Site {
        id: r.get(0)?,
        name: r.get(1)?,
        domain: r.get(2)?,
        project_root: PathBuf::from(r.get::<_, String>(3)?),
        document_root: PathBuf::from(r.get::<_, String>(4)?),
        project_type: parse(5, r.get(5)?)?,
        service_id: r.get(6)?,
        strategy: parse(7, r.get(7)?)?,
        upstream_port: r.get(8)?,
        runtime_installation_id: r.get(9)?,
        https_enabled: r.get(10)?,
        enabled: r.get(11)?,
        config_status: parse(12, r.get(12)?)?,
        config_message: r.get(13)?,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
        php_engine: parse(16, r.get(16)?)?,
        https_redirect: r.get(17)?,
    })
}

/// Joined service facts needed to present a site.
#[derive(Clone)]
struct ServiceFacts {
    name: String,
    listen_address: String,
    http_port: u16,
    https_port: Option<u16>,
}

impl ServiceFacts {
    fn none() -> Self {
        ServiceFacts { name: "No web server yet".into(), listen_address: "127.0.0.1".into(), http_port: 80, https_port: None }
    }
}

/// The loopback address a hosts entry must point to so the service is reached.
pub fn hosts_ip(listen_address: &str) -> Option<&'static str> {
    match listen_address {
        "0.0.0.0" | "127.0.0.1" => Some("127.0.0.1"),
        "::" | "::1" => Some("::1"),
        _ => None,
    }
}

fn site_url(domain: &str, port: u16) -> String {
    if port == 80 { format!("http://{domain}/") } else { format!("http://{domain}:{port}/") }
}

fn site_url_for(site: &Site, facts: &ServiceFacts) -> String {
    match facts.https_port.filter(|_| site.https_enabled) {
        Some(port) => format!("{}/", crate::web_server::https_origin(&site.domain, port)),
        None => site_url(&site.domain, facts.http_port),
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Validated, normalized input ready to persist.
#[derive(Debug)]
struct Valid {
    name: String,
    domain: String,
    project_root: PathBuf,
    document_root: PathBuf,
    input: SiteInput,
}

pub struct SiteManager {
    db: Arc<Db>,
    paths: AppPaths,
    services: Arc<ServiceManager>,
    runtimes: Arc<PhpRuntimes>,
    /// Site mutations and configuration activation run one at a time.
    lock: Mutex<()>,
}

impl SiteManager {
    pub fn new(db: Arc<Db>, paths: AppPaths, services: Arc<ServiceManager>, runtimes: Arc<PhpRuntimes>) -> Self {
        SiteManager { db, paths, services, runtimes, lock: Mutex::new(()) }
    }

    /// Every site, switched on or off.
    pub fn all_sites(&self) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM sites s ORDER BY s.domain"))?;
            let rows = stmt.query_map([], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Every enabled site.
    pub fn enabled_sites(&self) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM sites s WHERE s.enabled = 1 ORDER BY s.domain"))?;
            let rows = stmt.query_map([], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Enabled PHP sites across every web server.
    pub fn enabled_php_sites(&self) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM sites s WHERE s.enabled = 1 AND s.strategy = 'php' ORDER BY s.domain"
            ))?;
            let rows = stmt.query_map([], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Enabled sites that run on a PHP installation.
    pub fn enabled_sites_using_runtime(&self, installation_id: &str) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM sites s WHERE s.enabled = 1 AND s.runtime_installation_id = ?1"
            ))?;
            let rows = stmt.query_map([installation_id], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Service ids that serve at least one PHP site.
    pub fn services_with_php_sites(&self) -> AppResult<Vec<String>> {
        self.db.read(|c| {
            let mut stmt = c.prepare("SELECT DISTINCT service_id FROM sites WHERE strategy = 'php'")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// The generated configuration file of a site on the server that serves it.
    pub fn config_file(&self, id: &str) -> AppResult<PathBuf> {
        let site = self.get(id)?;
        let service_id = match site.service_id {
            Some(sid) => sid,
            None => self
                .serving_service()?
                .map(|s| s.service.id)
                .ok_or_else(|| AppError::NotFound { entity: "web server", id: "any".into() })?,
        };
        Ok(self.paths.service_generated_dir(&service_id).join("sites").join(format!("{}.conf", site.id)))
    }

    /// Web servers a site's configuration is written to.
    fn targets(&self, service_id: &Option<String>) -> AppResult<Vec<String>> {
        Ok(match service_id {
            // A server behind another one also changes the front's forwarding.
            Some(sid) => std::iter::once(sid.clone()).chain(self.services.hosted(sid).map(|(front, _)| front)).collect(),
            None => self.services.web_servers()?.into_iter().map(|s| s.id).collect(),
        })
    }

    /// The loopback port an app is opened on for the tailnet, if it is.
    pub fn tailnet_port(&self, site_id: &str) -> Option<u16> {
        self.db
            .read(|c| Ok(c.query_row("SELECT port FROM tailnet_apps WHERE site_id = ?1", [site_id], |r| r.get(0)).optional()?))
            .ok()
            .flatten()
    }

    /// Id of the web server "any server" apps are served by now.
    pub fn serving_service_id(&self) -> AppResult<Option<String>> {
        Ok(self.serving_service()?.map(|s| s.service.id))
    }

    /// The web server that currently serves "any server" sites: the running
    /// one, else the first.
    fn serving_service(&self) -> AppResult<Option<crate::service_manager::ServiceStatus>> {
        let all: Vec<_> = self
            .services
            .statuses()?
            .into_iter()
            .filter(|s| s.kind == crate::runtime_catalog::Category::WebServer)
            .collect();
        let running = all.iter().position(|s| s.health == Health::Running);
        Ok(match running {
            Some(i) => all.into_iter().nth(i),
            None => all.into_iter().next(),
        })
    }

    fn auto_facts(&self) -> AppResult<Option<ServiceFacts>> {
        Ok(self.serving_service()?.map(|s| ServiceFacts {
            name: if s.health == Health::Running {
                format!("Any running server ({})", s.service.name)
            } else {
                "Any running server".to_string()
            },
            listen_address: s.service.listen_address,
            http_port: s.service.http_port,
            https_port: s.service.https_port,
        }))
    }

    fn php_context(&self) -> AppResult<PhpContext> {
        Ok(PhpContext {
            php: runtime_manager::list(&self.db, Some(ComponentId::Php))?,
            frankens: php_runtime::frankenphp_installations(&self.db)?,
        })
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ----- queries ----------------------------------------------------------

    pub fn get(&self, id: &str) -> AppResult<Site> {
        self.db.read(|c| {
            c.query_row(&format!("SELECT {COLUMNS} FROM sites s WHERE s.id = ?1"), [id], site_from_row)
                .optional()?
                .ok_or_else(|| AppError::NotFound { entity: "site", id: id.to_string() })
        })
    }

    /// Enabled sites bound to exactly this server (not "any server" ones).
    pub fn enabled_bound_to(&self, service_id: &str) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM sites s WHERE s.service_id = ?1 AND s.enabled = 1 ORDER BY s.domain"))?;
            let rows = stmt.query_map([service_id], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn enabled_for_service(&self, service_id: &str) -> AppResult<Vec<Site>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM sites s WHERE (s.service_id = ?1 OR s.service_id IS NULL) AND s.enabled = 1 ORDER BY s.domain"
            ))?;
            let rows = stmt.query_map([service_id], site_from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn count_for_service(&self, service_id: &str) -> AppResult<u64> {
        self.db.read(|c| {
            Ok(c.query_row("SELECT COUNT(*) FROM sites WHERE service_id = ?1 OR service_id IS NULL", [service_id], |r| {
                r.get::<_, i64>(0)
            })? as u64)
        })
    }

    /// One page of sites, searched by name, domain or project folder.
    pub fn list(&self, query: &SiteQuery) -> AppResult<SitePage> {
        let limit = if query.limit == 0 { 50 } else { query.limit.min(500) };
        let pattern = if query.search.trim().is_empty() {
            String::new()
        } else {
            format!("%{}%", escape_like(query.search.trim()))
        };
        let filter = "(?1 = '' OR s.name LIKE ?1 ESCAPE '\\' OR s.domain LIKE ?1 ESCAPE '\\' OR s.project_root LIKE ?1 ESCAPE '\\')";
        let (rows, total) = self.db.read(|c| {
            let total = c.query_row(&format!("SELECT COUNT(*) FROM sites s WHERE {filter}"), [&pattern], |r| r.get::<_, i64>(0))? as u64;
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS}, v.name, v.listen_address, v.http_port, v.https_port FROM sites s
                 LEFT JOIN services v ON v.id = s.service_id
                 WHERE {filter}
                 ORDER BY s.name COLLATE NOCASE, s.domain LIMIT ?2 OFFSET ?3"
            ))?;
            let rows = stmt
                .query_map(params![pattern, limit, query.offset], |r| {
                    let facts = match r.get::<_, Option<String>>(18)? {
                        Some(name) => Some(ServiceFacts { name, listen_address: r.get(19)?, http_port: r.get(20)?, https_port: r.get(21)? }),
                        None => None,
                    };
                    Ok((site_from_row(r)?, facts))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((rows, total))
        })?;
        let hosts_content = hosts::read_hosts(&hosts::system_hosts_path()).unwrap_or_default();
        let php = self.php_context()?;
        let auto = self.auto_facts()?;
        let items = rows
            .into_iter()
            .map(|(s, f)| {
                let facts = f.or_else(|| auto.clone()).unwrap_or_else(ServiceFacts::none);
                present(s, &facts, &hosts_content, &php)
            })
            .collect();
        Ok(SitePage { items, total, offset: query.offset, limit })
    }

    pub fn view(&self, id: &str) -> AppResult<SiteView> {
        let site = self.get(id)?;
        let facts = match &site.service_id {
            Some(sid) => {
                let svc = self.services.get(sid)?;
                ServiceFacts { name: svc.name, listen_address: svc.listen_address, http_port: svc.http_port, https_port: svc.https_port }
            }
            None => self.auto_facts()?.unwrap_or_else(ServiceFacts::none),
        };
        let hosts_content = hosts::read_hosts(&hosts::system_hosts_path()).unwrap_or_default();
        Ok(present(site, &facts, &hosts_content, &self.php_context()?))
    }

    pub fn detect(&self, project_root: &str) -> AppResult<detect::DetectedProject> {
        detect::detect(Path::new(project_root.trim()))
    }

    // ----- validation -------------------------------------------------------

    fn validate(&self, input: &SiteInput, editing: Option<&str>) -> AppResult<Valid> {
        let name = input.name.trim().to_string();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(AppError::Validation("site name must be 1–100 characters".into()));
        }
        safe_value("site name", &name)?;
        let domain = domain_manager::validate_domain(&input.domain)?;
        let taken: Option<String> = self.db.read(|c| {
            Ok(c.query_row(
                "SELECT name FROM sites WHERE domain = ?1 COLLATE NOCASE AND id <> ?2",
                params![domain, editing.unwrap_or("")],
                |r| r.get(0),
            )
            .optional()?)
        })?;
        if let Some(other) = taken {
            return Err(AppError::Validation(format!("{domain} is already used by site '{other}'")));
        }

        let project_root = existing_dir("project folder", input.project_root.trim(), None)?;
        let document_root = existing_dir("document root", input.document_root.trim(), Some(&project_root))?;
        if !document_root.starts_with(&project_root) {
            return Err(AppError::Validation(format!(
                "the document root must be inside the project folder ({})",
                project_root.display()
            )));
        }
        for p in [&project_root, &document_root] {
            safe_value("path", &p.display().to_string())?;
        }

        let mut input = input.clone();
        if input.service_id.as_deref().is_some_and(str::is_empty) {
            input.service_id = None;
        }
        let candidate_ports: Vec<u16> = match &input.service_id {
            Some(sid) => {
                let svc = self.services.get(sid)?;
                if !crate::web_server::adapter_for(svc.component).is_some_and(|a| a.supports_sites()) {
                    return Err(AppError::Validation(format!("'{}' is not a web server", svc.name)));
                }
                vec![svc.http_port]
            }
            None => self.services.web_servers()?.into_iter().map(|s| s.http_port).collect(),
        };
        if input.runtime_installation_id.as_deref().is_some_and(str::is_empty) {
            input.runtime_installation_id = None;
        }
        if input.strategy != Strategy::Php {
            input.php_engine = PhpEngine::Auto;
        }
        match input.strategy {
            Strategy::Static => input.upstream_port = None,
            Strategy::Php => {
                input.upstream_port = None;
                let id = input
                    .runtime_installation_id
                    .as_deref()
                    .ok_or_else(|| AppError::Validation("choose the PHP version this site runs on".into()))?;
                let php = runtime_manager::get(&self.db, id)?;
                if php.component != ComponentId::Php {
                    return Err(AppError::Validation(format!("{} {} is not a PHP installation", php.component, php.version)));
                }
                if !php.enabled || !php.executable_exists {
                    return Err(AppError::Validation(format!("PHP {} is switched off or missing; choose another version", php.version)));
                }
                let frankens = php_runtime::frankenphp_installations(&self.db)?;
                if let Backend::Fastcgi { php } = php_runtime::select(input.php_engine, &php, &frankens)? {
                    if !php.root_path.join("php-cgi.exe").is_file() {
                        return Err(AppError::Validation(format!(
                            "PHP {} has no php-cgi.exe, which the web server needs to run it",
                            php.version
                        )));
                    }
                }
            }
            Strategy::ReverseProxy => {
                let port = input.upstream_port.filter(|p| *p > 0).ok_or_else(|| {
                    AppError::Validation("a reverse proxy needs the port your app listens on".into())
                })?;
                if candidate_ports.contains(&port) {
                    return Err(AppError::Validation(format!(
                        "port {port} is the web server's own port; the app must listen on a different port"
                    )));
                }
            }
        }
        if let Some(rid) = input.runtime_installation_id.as_deref() {
            runtime_manager::get(&self.db, rid)?;
        }
        if !input.https_enabled {
            input.https_redirect = false;
        } else {
            if !crate::tls::covers(&domain) {
                return Err(AppError::Validation(format!(
                    "HTTPS is available for domains ending in .{}",
                    crate::tls::TLS_SUFFIXES.join(", .")
                )));
            }
            if !self.services.tls().has_ca() {
                return Err(AppError::Conflict(
                    "HTTPS needs Zenv's local certificate authority; create it in Settings first".into(),
                ));
            }
        }
        Ok(Valid { name, domain, project_root, document_root, input })
    }

    // ----- mutations (each one applied or rolled back) ---------------------

    fn insert_row(&self, id: &str, v: &Valid, enabled: bool, status: ConfigStatus) -> AppResult<()> {
        let now = now_rfc3339();
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                 upstream_port, runtime_installation_id, enabled, config_status, created_at, updated_at, php_engine,
                 https_enabled, https_redirect)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14, ?15, ?16)",
                params![
                    id, v.name, v.domain, v.project_root.display().to_string(), v.document_root.display().to_string(),
                    v.input.project_type.as_str(), v.input.service_id, v.input.strategy.as_str(), v.input.upstream_port,
                    v.input.runtime_installation_id, enabled, status.as_str(), now, v.input.php_engine.as_str(),
                    v.input.https_enabled, v.input.https_redirect
                ],
            )?;
            Ok(())
        })
    }

    fn restore_row(&self, s: &Site) -> AppResult<()> {
        self.db.write(|tx| {
            tx.execute("DELETE FROM sites WHERE id = ?1", [&s.id])?;
            tx.execute(
                "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                 upstream_port, runtime_installation_id, https_enabled, enabled, config_status, config_message,
                 created_at, updated_at, php_engine, https_redirect)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    s.id, s.name, s.domain, s.project_root.display().to_string(), s.document_root.display().to_string(),
                    s.project_type.as_str(), s.service_id, s.strategy.as_str(), s.upstream_port, s.runtime_installation_id,
                    s.https_enabled, s.enabled, s.config_status.as_str(), s.config_message, s.created_at, s.updated_at,
                    s.php_engine.as_str(), s.https_redirect
                ],
            )?;
            Ok(())
        })
    }

    fn apply(&self, service_id: &str) -> AppResult<ApplyReport> {
        virtual_host::apply_service(&self.db, &self.paths, &self.services, self, &self.runtimes, service_id)
    }

    /// Applies each service; on the first failure runs `undo` and re-applies
    /// so files match the restored records again.
    fn apply_or_undo(&self, service_ids: &[String], undo: impl FnOnce() -> AppResult<()>) -> AppResult<()> {
        for sid in service_ids {
            if let Err(e) = self.apply(sid) {
                undo()?;
                for sid in service_ids {
                    if let Err(again) = self.apply(sid) {
                        tracing::warn!(service = %sid, error = %again, "re-apply after rollback failed");
                    }
                }
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn create(&self, input: &SiteInput) -> AppResult<SiteView> {
        let _g = self.guard();
        let v = self.validate(input, None)?;
        let id = new_id();
        let enabled = v.input.enabled;
        self.insert_row(&id, &v, enabled, if enabled { ConfigStatus::Pending } else { ConfigStatus::Disabled })?;
        if !enabled {
            // A disabled site has no server configuration to activate.
            return self.view(&id);
        }
        let targets = self.targets(&v.input.service_id)?;
        self.apply_or_undo(&targets, || {
            self.db.write(|tx| {
                tx.execute("DELETE FROM sites WHERE id = ?1", [&id])?;
                Ok(())
            })
        })?;
        self.view(&id)
    }

    pub fn update(&self, id: &str, input: &SiteInput) -> AppResult<SiteView> {
        let _g = self.guard();
        let old = self.get(id)?;
        let v = self.validate(input, Some(id))?;
        self.db.write(|tx| {
            tx.execute(
                "UPDATE sites SET name = ?2, domain = ?3, project_root = ?4, document_root = ?5, project_type = ?6,
                 service_id = ?7, strategy = ?8, upstream_port = ?9, runtime_installation_id = ?10, enabled = ?11,
                 config_status = 'pending', updated_at = ?12, php_engine = ?13, https_enabled = ?14, https_redirect = ?15
                 WHERE id = ?1",
                params![
                    id, v.name, v.domain, v.project_root.display().to_string(), v.document_root.display().to_string(),
                    v.input.project_type.as_str(), v.input.service_id, v.input.strategy.as_str(), v.input.upstream_port,
                    v.input.runtime_installation_id, v.input.enabled, now_rfc3339(), v.input.php_engine.as_str(),
                    v.input.https_enabled, v.input.https_redirect
                ],
            )?;
            Ok(())
        })?;
        if !old.enabled && !v.input.enabled {
            self.db.write(|tx| {
                tx.execute("UPDATE sites SET config_status = 'disabled' WHERE id = ?1", [id])?;
                Ok(())
            })?;
            return self.view(id);
        }
        let mut affected = self.targets(&old.service_id)?;
        for t in self.targets(&v.input.service_id)? {
            if !affected.contains(&t) {
                affected.push(t);
            }
        }
        self.apply_or_undo(&affected, || self.restore_row(&old))?;
        self.view(id)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<SiteView> {
        let _g = self.guard();
        let old = self.get(id)?;
        if old.enabled != enabled {
            self.db.write(|tx| {
                tx.execute(
                    "UPDATE sites SET enabled = ?2, config_status = 'pending', updated_at = ?3 WHERE id = ?1",
                    params![id, enabled, now_rfc3339()],
                )?;
                Ok(())
            })?;
            let targets = self.targets(&old.service_id)?;
            self.apply_or_undo(&targets, || self.restore_row(&old))?;
        }
        self.view(id)
    }

    /// Copies a site's settings under a free `-copy` domain. The copy starts
    /// disabled so it never collides with the original.
    pub fn duplicate(&self, id: &str) -> AppResult<SiteView> {
        let _g = self.guard();
        let old = self.get(id)?;
        let (label, rest) = old.domain.split_once('.').unwrap_or((&old.domain, domain_manager::DEFAULT_TLD));
        let taken = |d: &str| -> AppResult<bool> {
            self.db.read(|c| Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM sites WHERE domain = ?1 COLLATE NOCASE)", [d], |r| r.get(0))?))
        };
        let mut n = 1;
        let domain = loop {
            let candidate = if n == 1 { format!("{label}-copy.{rest}") } else { format!("{label}-copy-{n}.{rest}") };
            if !taken(&candidate)? {
                break candidate;
            }
            n += 1;
        };
        let copy = Site {
            id: new_id(),
            name: format!("{} (copy)", old.name),
            domain,
            enabled: false,
            config_status: ConfigStatus::Disabled,
            config_message: None,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
            ..old
        };
        self.restore_row(&copy)?;
        self.view(&copy.id)
    }

    /// Removes the registration and its server configuration. Project files
    /// are never touched.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let _g = self.guard();
        let old = self.get(id)?;
        self.db.write(|tx| {
            tx.execute("DELETE FROM sites WHERE id = ?1", [id])?;
            Ok(())
        })?;
        if !old.enabled {
            return Ok(());
        }
        let targets = self.targets(&old.service_id)?;
        self.apply_or_undo(&targets, || self.restore_row(&old))
    }

    /// Re-applies a service's sites on request; failures are recorded on the sites.
    pub fn reapply(&self, service_id: &str) -> AppResult<ApplyReport> {
        let _g = self.guard();
        self.apply(service_id).inspect_err(|e| {
            let msg = e.to_string();
            let _ = self.db.write(|tx| {
                tx.execute(
                    "UPDATE sites SET config_status = 'failed', config_message = ?2 WHERE service_id = ?1 AND enabled = 1",
                    params![service_id, msg],
                )?;
                Ok(())
            });
        })
    }

    pub(crate) fn mark_applied(&self, service_id: &str, message: &str) -> AppResult<()> {
        self.db.write(|tx| {
            tx.execute(
                "UPDATE sites SET config_status = CASE enabled WHEN 1 THEN 'applied' ELSE 'disabled' END,
                 config_message = CASE enabled WHEN 1 THEN ?2 ELSE NULL END WHERE service_id = ?1 OR service_id IS NULL",
                params![service_id, message],
            )?;
            Ok(())
        })
    }

    // ----- hosts file -------------------------------------------------------

    pub fn desired_hosts_entries(&self) -> AppResult<Vec<HostsEntry>> {
        let rows: Vec<(String, String)> = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT s.domain, COALESCE(v.listen_address, '127.0.0.1') FROM sites s
                 LEFT JOIN services v ON v.id = s.service_id WHERE s.enabled = 1",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })?;
        Ok(rows
            .into_iter()
            .filter(|(d, _)| domain_manager::resolution(d) == Resolution::HostsFile)
            .filter_map(|(domain, listen)| hosts_ip(&listen).map(|ip| HostsEntry { ip: ip.to_string(), domain }))
            .collect())
    }

    pub fn hosts_status(&self) -> AppResult<HostsStatus> {
        let path = hosts::system_hosts_path();
        let current = hosts::read_hosts(&path)?;
        Ok(HostsStatus { plan: hosts::plan(&current, &self.desired_hosts_entries()?), path })
    }

    pub fn hosts_apply(&self) -> AppResult<ApplyOutcome> {
        hosts::apply(&self.desired_hosts_entries()?, &self.paths.root.join("hosts"))
    }

    // ----- diagnosis --------------------------------------------------------

    fn php_check(&self, site: &Site) -> AppResult<Check> {
        let label = "PHP is running for this site";
        let Some(id) = site.runtime_installation_id.as_deref() else {
            return Ok(check("php", label, CheckStatus::Error, "no PHP version selected"));
        };
        let php = runtime_manager::get(&self.db, id)?;
        let frankens = php_runtime::frankenphp_installations(&self.db)?;
        Ok(match php_runtime::select(site.php_engine, &php, &frankens) {
            Err(e) => check("php", label, CheckStatus::Error, e.to_string()),
            Ok(Backend::Fastcgi { php }) => match self.runtimes.php_ports(&php.id) {
                Some(ports) if ports.iter().all(|p| php_runtime::port_in_use(*p)) => check(
                    "php",
                    label,
                    CheckStatus::Ok,
                    format!(
                        "PHP {} via PHP-CGI: {} workers on 127.0.0.1:{}",
                        php.version,
                        ports.len(),
                        ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")
                    ),
                ),
                Some(_) => check("php", label, CheckStatus::Warning, format!("some PHP {} workers are restarting", php.version)),
                None => check("php", label, CheckStatus::Error, format!("PHP {} workers are not running; start the web server", php.version)),
            },
            Ok(Backend::Frankenphp { franken }) => match self.runtimes.franken_port(&franken.id) {
                Some(port) if php_runtime::port_in_use(port) => check(
                    "php",
                    label,
                    CheckStatus::Ok,
                    format!(
                        "FrankenPHP {} (PHP {}) on 127.0.0.1:{port}",
                        franken.version,
                        php_runtime::embedded_php(&franken).unwrap_or_default()
                    ),
                ),
                _ => check("php", label, CheckStatus::Error, format!("FrankenPHP {} is not running; start the web server", franken.version)),
            },
        })
    }

    pub fn diagnose(&self, id: &str) -> AppResult<Vec<Check>> {
        let view = self.view(id)?;
        let site = &view.site;
        let status = match &site.service_id {
            Some(sid) => self.services.status(sid)?,
            None => match self.serving_service()? {
                Some(s) => s,
                None => {
                    return Ok(vec![check(
                        "server",
                        "A web server is running",
                        CheckStatus::Error,
                        "no web server exists yet; add one on the Web Servers page",
                    )])
                }
            },
        };
        let svc = &status.service;
        let mut out = Vec::new();

        if site.strategy == Strategy::Php && !view.document_root_exists {
            out.push(check(
                "docroot",
                "Document root exists",
                CheckStatus::Error,
                format!("{} is missing; it may have been moved or deleted", site.document_root.display()),
            ));
        }
        out.push(match site.strategy {
            Strategy::Static if view.document_root_exists => {
                check("docroot", "Document root exists", CheckStatus::Ok, site.document_root.display().to_string())
            }
            Strategy::Static => check(
                "docroot",
                "Document root exists",
                CheckStatus::Error,
                format!("{} is missing; it may have been moved or deleted", site.document_root.display()),
            ),
            Strategy::Php => self.php_check(site)?,
            Strategy::ReverseProxy => {
                let port = site.upstream_port.unwrap_or_default();
                match port_manager::inspect(port, "127.0.0.1".parse().expect("ip"))?.status {
                    PortStatus::InUse { owners } => check(
                        "upstream",
                        format!("App listening on 127.0.0.1:{port}"),
                        CheckStatus::Ok,
                        owners.iter().map(|o| format!("{} (PID {})", o.process_name.as_deref().unwrap_or("unknown"), o.pid)).collect::<Vec<_>>().join(", "),
                    ),
                    _ => check(
                        "upstream",
                        format!("App listening on 127.0.0.1:{port}"),
                        CheckStatus::Error,
                        format!("nothing listens on 127.0.0.1:{port}; start your app there, or change the site's app port"),
                    ),
                }
            }
        });

        out.push(match view.hosts_state {
            DomainState::NotNeeded => check("resolution", "Domain resolves to this computer", CheckStatus::Ok,
                format!("{} is a .localhost name: browsers resolve it to 127.0.0.1 without a hosts entry (some command-line tools do not)", site.domain)),
            DomainState::Managed | DomainState::External => {
                let resolved = (site.domain.as_str(), svc.http_port).to_socket_addrs().map(|a| a.collect::<Vec<_>>()).unwrap_or_default();
                if resolved.iter().any(|a| a.ip().is_loopback()) {
                    check("resolution", "Domain resolves to this computer", CheckStatus::Ok, format!("{} → {}", site.domain, resolved[0].ip()))
                } else {
                    check("resolution", "Domain resolves to this computer", CheckStatus::Warning,
                        format!("the hosts file maps {} but Windows does not resolve it yet; try `ipconfig /flushdns`", site.domain))
                }
            }
            DomainState::Missing => check("resolution", "Domain resolves to this computer", CheckStatus::Error,
                format!("{} is not in the hosts file yet; use “Update hosts file” on the Local Sites page", site.domain)),
            DomainState::Conflict => check("resolution", "Domain resolves to this computer", CheckStatus::Error,
                format!("an existing line in the hosts file points {} somewhere else; remove or change that line", site.domain)),
        });

        out.push(match site.config_status {
            ConfigStatus::Applied => check("config", "Server configuration", CheckStatus::Ok, site.config_message.clone().unwrap_or_else(|| "active".into())),
            ConfigStatus::Disabled => check("config", "Server configuration", CheckStatus::Warning, "the site is disabled"),
            ConfigStatus::Pending => check("config", "Server configuration", CheckStatus::Warning, "not applied yet"),
            ConfigStatus::Failed => check("config", "Server configuration", CheckStatus::Error, site.config_message.clone().unwrap_or_default()),
        });

        if status.health != Health::Running {
            out.push(check("server", format!("Web server '{}' is running", svc.name), CheckStatus::Error,
                format!("start '{}' on the Web Servers page", svc.name)));
            return Ok(out);
        }
        out.push(check("server", format!("Web server '{}' is running", svc.name), CheckStatus::Ok, status.url.clone()));
        if site.enabled {
            let target = SocketAddr::new(
                crate::service_manager::connect_address(port_manager::parse_bind_address(&svc.listen_address)?),
                svc.http_port,
            );
            out.push(match http_probe(target, &site.domain) {
                Ok(code) if code < 500 => check("http", "Site answers HTTP requests", CheckStatus::Ok, format!("HTTP {code} for {}", view.url)),
                Ok(code) => check("http", "Site answers HTTP requests", CheckStatus::Error, format!(
                    "HTTP {code}{}",
                    if site.strategy == Strategy::ReverseProxy { ": the web server could not reach your app" } else { "" }
                )),
                Err(e) => check("http", "Site answers HTTP requests", CheckStatus::Error, e),
            });
        }
        Ok(out)
    }
}

/// Installations needed to describe where PHP sites run.
struct PhpContext {
    php: Vec<Installation>,
    frankens: Vec<Installation>,
}

fn present(site: Site, facts: &ServiceFacts, hosts_content: &str, ctx: &PhpContext) -> SiteView {
    let ip = hosts_ip(&facts.listen_address).unwrap_or("127.0.0.1");
    let php = (site.strategy == Strategy::Php)
        .then(|| site.runtime_installation_id.as_deref())
        .flatten()
        .and_then(|id| ctx.php.iter().find(|i| i.id == id));
    let php_backend = php.and_then(|p| php_runtime::select(site.php_engine, p, &ctx.frankens).ok()).map(|b| match b {
        Backend::Fastcgi { .. } => "fastcgi",
        Backend::Frankenphp { .. } => "frankenphp",
    });
    SiteView {
        compat: compat::check(&site),
        php_version: php.map(|p| p.version.clone()),
        php_backend,
        url: site_url_for(&site, facts),
        resolution: domain_manager::resolution(&site.domain),
        hosts_state: hosts::domain_state(hosts_content, &site.domain, ip),
        document_root_exists: site.document_root.is_dir(),
        service_name: facts.name.clone(),
        site,
    }
}

/// Resolves `raw` (relative paths against `base`) to an existing directory.
fn existing_dir(what: &str, raw: &str, base: Option<&Path>) -> AppResult<PathBuf> {
    if raw.is_empty() {
        return Err(AppError::Validation(format!("choose the {what}")));
    }
    let p = PathBuf::from(raw);
    let p = match (p.is_absolute(), base) {
        (true, _) => p,
        (false, Some(b)) => b.join(p),
        (false, None) => return Err(AppError::invalid_path(&p, format!("the {what} must be a full path"))),
    };
    if !p.is_dir() {
        return Err(AppError::invalid_path(&p, format!("the {what} does not exist or is not a folder")));
    }
    dunce::canonicalize(&p).map_err(|e| AppError::invalid_path(&p, e.to_string()))
}

/// Sends `GET /` with the site's Host header and returns the status code.
pub fn http_probe(target: SocketAddr, host: &str) -> Result<u16, String> {
    let mut s = TcpStream::connect_timeout(&target, Duration::from_secs(3)).map_err(|e| format!("cannot connect to {target}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    write!(s, "GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: zenv-diagnostics\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64];
    let n = s.read(&mut buf).map_err(|e| format!("no response: {e}"))?;
    let head = String::from_utf8_lossy(&buf[..n]);
    head.split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("unexpected response: {}", head.lines().next().unwrap_or("")))
}

#[cfg(test)]
pub(crate) mod tests;
