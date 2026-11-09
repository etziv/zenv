//! Managed service instances: persistent records plus lifecycle orchestration
//! (validate → port check → spawn → readiness → events).

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::config_generator::managed_files::{self, Owner};
use crate::error::{AppError, AppResult, IoContext};
use crate::log_manager::{self, LogTail};
use crate::paths::AppPaths;
use crate::port_manager;
use crate::process_manager::{ProcessManager, ProcessSpec, ProcessState, StopMethod};
use crate::runtime_catalog::{self, ComponentId};
use crate::runtime_manager::{self, Installation};
use crate::storage::Db;
use crate::tls::Tls;
use crate::util::{new_id, now_rfc3339};
use crate::runtime_catalog::Category;
use crate::web_server::{
    self, ConnectionInfo, Control, DataDirInfo, DataDirState, HttpsContext, LogFile, ServerContext, ServiceAdapter, ValidationReport,
};

const STOP_GRACE: Duration = Duration::from_secs(10);
const OWNER_KIND: &str = "service";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub component: ComponentId,
    pub name: String,
    pub installation_id: String,
    pub listen_address: String,
    pub http_port: u16,
    pub autostart: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Databases and caches: the folder holding their data. Never deleted.
    pub data_dir: Option<PathBuf>,
    /// Web servers: the TLS port, used while a site it serves has HTTPS on.
    pub https_port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInput {
    pub name: String,
    pub installation_id: String,
    pub listen_address: String,
    pub http_port: u16,
    pub autostart: bool,
    /// Databases and caches: a custom data folder (default: inside Zenv's data).
    #[serde(default)]
    pub data_dir: Option<String>,
    /// Web servers: the TLS port (default 443).
    #[serde(default)]
    pub https_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Running,
    Stopped,
    /// Exited without Zenv asking it to.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub service: Service,
    pub installation: Option<Installation>,
    pub health: Health,
    pub process: ProcessState,
    pub url: String,
    pub config_path: PathBuf,
    pub document_root: PathBuf,
    pub kind: Category,
    pub data_dir_info: Option<DataDirInfo>,
    pub connection: Option<ConnectionInfo>,
    /// Name of the web server holding this one's port and passing its apps'
    /// requests on (both run at once).
    pub behind: Option<String>,
    /// Its data folder must be created by Zenv before the first start
    /// (MySQL, PostgreSQL); other servers fill theirs themselves.
    pub needs_initialization: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEvent {
    pub id: i64,
    pub level: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSource {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub exists: bool,
    pub size_bytes: u64,
}

pub struct ServiceManager {
    db: Arc<Db>,
    paths: AppPaths,
    tls: Arc<Tls>,
    processes: ProcessManager,
    /// Whether each running web server was started with its HTTPS listener.
    active_https: Mutex<HashMap<String, bool>>,
    /// Services Zenv stopped on purpose (so their exit is not a failure).
    expected_stops: Mutex<HashSet<String>>,
    /// Unexpected exits already written to the event log.
    reported_failures: Mutex<HashSet<String>>,
    /// Serializes lifecycle operations per service (start/stop/reload races).
    op_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Web servers running behind another one that holds their port: guest
    /// id -> (front id, internal port on 127.0.0.1).
    hosted: Mutex<HashMap<String, (String, u16)>>,
}

const COLUMNS: &str =
    "id, component, name, installation_id, listen_address, http_port, autostart, created_at, updated_at, data_dir, https_port";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Service> {
    let component: String = row.get(1)?;
    Ok(Service {
        id: row.get(0)?,
        component: ComponentId::from_str(&component).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        name: row.get(2)?,
        installation_id: row.get(3)?,
        listen_address: row.get(4)?,
        http_port: row.get(5)?,
        autostart: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        data_dir: row.get::<_, Option<String>>(9)?.map(PathBuf::from),
        https_port: row.get(10)?,
    })
}

pub fn connect_address(bind: IpAddr) -> IpAddr {
    match bind {
        IpAddr::V4(a) if a.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(a) if a.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other,
    }
}

impl ServiceManager {
    pub fn new(db: Arc<Db>, paths: AppPaths, tls: Arc<Tls>) -> Self {
        ServiceManager {
            db,
            paths,
            tls,
            processes: ProcessManager::new(),
            active_https: Mutex::default(),
            expected_stops: Mutex::default(),
            reported_failures: Mutex::default(),
            op_locks: Mutex::default(),
            hosted: Mutex::default(),
        }
    }

    // ----- records -------------------------------------------------------

    pub fn list(&self) -> AppResult<Vec<Service>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM services ORDER BY name COLLATE NOCASE"))?;
            let rows = stmt.query_map([], from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    pub fn get(&self, id: &str) -> AppResult<Service> {
        self.db.read(|c| {
            c.query_row(&format!("SELECT {COLUMNS} FROM services WHERE id = ?1"), [id], from_row)
                .optional()?
                .ok_or_else(|| AppError::NotFound { entity: "service", id: id.to_string() })
        })
    }

    /// Resolves the data folder for services that need one.
    fn resolve_data_dir(&self, component: ComponentId, id: &str, input: &ServiceInput) -> AppResult<Option<String>> {
        let Some(adapter) = web_server::adapter_for(component) else { return Ok(None) };
        if !adapter.needs_data_dir() {
            return Ok(None);
        }
        let dir = match input.data_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            Some(d) => {
                let p = PathBuf::from(d);
                if !p.is_absolute() {
                    return Err(AppError::invalid_path(&p, "the data folder must be a full path"));
                }
                p
            }
            None => self.paths.root.join("data").join(adapter.default_data_dir_name(id)),
        };
        crate::config_generator::safe_value("data folder", &dir.display().to_string())?;
        Ok(Some(dir.display().to_string()))
    }

    /// Web servers always have a TLS port (443 unless chosen otherwise).
    fn resolve_https_port(&self, component: ComponentId, input: &ServiceInput) -> Option<u16> {
        web_server::adapter_for(component).filter(|a| a.supports_sites()).map(|_| input.https_port.unwrap_or(443))
    }

    fn validate_input(&self, input: &ServiceInput, editing: Option<&str>) -> AppResult<(String, ComponentId)> {
        let name = input.name.trim().to_string();
        if name.is_empty() || name.chars().count() > 64 {
            return Err(AppError::Validation("service name must be 1–64 characters".into()));
        }
        crate::config_generator::safe_value("service name", &name)?;
        if input.http_port == 0 || input.https_port == Some(0) {
            return Err(AppError::Validation("port must be between 1 and 65535".into()));
        }
        if input.https_port == Some(input.http_port) {
            return Err(AppError::Validation("the HTTP and HTTPS ports must differ".into()));
        }
        port_manager::parse_bind_address(input.listen_address.trim())?;

        let install = runtime_manager::get(&self.db, &input.installation_id)?;
        if web_server::adapter_for(install.component).is_none() {
            let info = runtime_catalog::info(install.component);
            return Err(AppError::Unsupported(format!(
                "service management for {} is planned for milestone {}",
                info.name, info.planned_milestone
            )));
        }

        for other in self.list()? {
            if Some(other.id.as_str()) == editing {
                continue;
            }
            if other.name.eq_ignore_ascii_case(&name) {
                return Err(AppError::Validation(format!("a service named '{name}' already exists")));
            }
        }
        Ok((name, install.component))
    }

    pub fn create(&self, input: &ServiceInput) -> AppResult<Service> {
        let (name, component) = self.validate_input(input, None)?;
        let id = new_id();
        let now = now_rfc3339();
        let data_dir = self.resolve_data_dir(component, &id, input)?;
        let https_port = self.resolve_https_port(component, input);
        self.db.write(|tx| {
            tx.execute(
                &format!("INSERT INTO services ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10)"),
                params![
                    id,
                    component.as_str(),
                    name,
                    input.installation_id,
                    input.listen_address.trim(),
                    input.http_port,
                    input.autostart,
                    now,
                    data_dir,
                    https_port
                ],
            )?;
            Ok(())
        })?;
        if web_server::adapter_for(component).is_some_and(|a| a.supports_sites()) {
            self.ensure_document_root(&id)?;
        }
        self.event(&id, "info", format!("service created ({}:{})", input.listen_address.trim(), input.http_port));
        self.get(&id)
    }

    pub fn update(&self, id: &str, input: &ServiceInput) -> AppResult<Service> {
        let current = self.get(id)?;
        if self.processes.is_running(id) {
            return Err(AppError::Conflict("stop the service before changing its settings".into()));
        }
        let (name, component) = self.validate_input(input, Some(id))?;
        if component != current.component {
            return Err(AppError::Validation(format!(
                "the installation must be a {} installation",
                runtime_catalog::info(current.component).name
            )));
        }
        let data_dir = self.resolve_data_dir(component, id, input)?;
        let https_port = self.resolve_https_port(component, input);
        self.db.write(|tx| {
            tx.execute(
                "UPDATE services SET name = ?2, installation_id = ?3, listen_address = ?4, http_port = ?5,
                 autostart = ?6, updated_at = ?7, data_dir = ?8, https_port = ?9 WHERE id = ?1",
                params![
                    id,
                    name,
                    input.installation_id,
                    input.listen_address.trim(),
                    input.http_port,
                    input.autostart,
                    now_rfc3339(),
                    data_dir,
                    https_port
                ],
            )?;
            Ok(())
        })?;
        self.event(id, "info", "settings updated".into());
        self.get(id)
    }

    /// Deletes the record and Zenv-generated config. Never touches the
    /// installation; keeps logs and any generated file the user edited.
    pub fn delete(&self, id: &str) -> AppResult<Vec<PathBuf>> {
        self.get(id)?;
        if self.processes.is_running(id) {
            return Err(AppError::Conflict("stop the service before deleting it".into()));
        }
        let sites: i64 =
            self.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM sites WHERE service_id = ?1", [id], |r| r.get(0))?))?;
        if sites > 0 {
            return Err(AppError::Conflict(format!(
                "{sites} local site(s) are served by this web server; move them to another server or delete them first"
            )));
        }
        let kept = managed_files::release_owner(&self.db, Owner { kind: OWNER_KIND, id })?;
        self.db.write(|tx| {
            tx.execute("DELETE FROM services WHERE id = ?1", [id])?;
            Ok(())
        })?;
        Ok(kept)
    }

    // ----- configuration ---------------------------------------------------

    pub fn context(&self, service: &Service) -> AppResult<ServerContext> {
        // Behind another server: an internal port on loopback; the front
        // server terminates HTTPS for its apps.
        if let Some((_, port)) = self.hosted(&service.id) {
            return Ok(ServerContext {
                listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port,
                https: None,
                ..self.own_context(service)?
            });
        }
        self.own_context(service)
    }

    fn own_context(&self, service: &Service) -> AppResult<ServerContext> {
        Ok(ServerContext {
            service_id: service.id.clone(),
            service_name: service.name.clone(),
            listen_address: port_manager::parse_bind_address(&service.listen_address)?,
            port: service.http_port,
            document_root: self.document_root(&service.id),
            logs_dir: self.paths.service_logs_dir(&service.id),
            sites_dir: self.paths.service_generated_dir(&service.id).join("sites"),
            temp_dir: self.paths.service_generated_dir(&service.id).join("temp"),
            data_dir: service.data_dir.clone(),
            https: self.https_context(service)?,
        })
    }

    /// HTTPS is served while the CA exists and an enabled site this server
    /// serves has HTTPS on; otherwise the TLS port stays free.
    fn https_context(&self, service: &Service) -> AppResult<Option<HttpsContext>> {
        let Some(port) = service.https_port else { return Ok(None) };
        if !self.tls.has_ca() {
            return Ok(None);
        }
        // Its own apps, "any server" apps and those of servers behind it.
        let mut serves = self.guests_of(&service.id).into_iter().map(|(id, _)| id).collect::<HashSet<_>>();
        serves.insert(service.id.clone());
        let bound: Vec<Option<String>> = self.db.read(|c| {
            let mut stmt = c.prepare("SELECT service_id FROM sites WHERE enabled = 1 AND https_enabled = 1")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })?;
        if !bound.iter().any(|b| b.as_ref().is_none_or(|id| serves.contains(id))) {
            return Ok(None);
        }
        let dir = self.paths.generated_dir.join("tls").join("sites");
        Ok(Some(HttpsContext { port, default_cert: dir.join("localhost.pem"), default_key: dir.join("localhost-key.pem") }))
    }

    /// The certificate for the HTTPS default server (unknown names).
    fn ensure_default_cert(&self, ctx: &ServerContext) -> AppResult<()> {
        if ctx.https.is_some() {
            self.tls.ensure_cert(&["localhost".into(), "127.0.0.1".into(), "::1".into()])?;
        }
        Ok(())
    }

    /// Whether the running server's active configuration includes HTTPS.
    pub fn active_https(&self, id: &str) -> bool {
        self.active_https.lock().unwrap_or_else(|p| p.into_inner()).get(id).copied().unwrap_or(false)
    }

    fn document_root(&self, id: &str) -> PathBuf {
        self.paths.dev_dir.join(id)
    }

    fn config_path(&self, service: &Service, adapter: &dyn ServiceAdapter) -> PathBuf {
        self.paths.service_generated_dir(&service.id).join(adapter.main_config_name())
    }

    /// Creates the default document root with a landing page, never
    /// overwriting an existing one.
    fn ensure_document_root(&self, id: &str) -> AppResult<()> {
        let root = self.document_root(id);
        std::fs::create_dir_all(&root).ctx(|| format!("creating {}", root.display()))?;
        let index = root.join("index.html");
        if !index.exists() {
            std::fs::write(&index, DEFAULT_INDEX).ctx(|| format!("writing {}", index.display()))?;
        }
        Ok(())
    }

    pub fn load(&self, id: &str) -> AppResult<(Service, Installation, &'static dyn ServiceAdapter)> {
        let service = self.get(id)?;
        let install = runtime_manager::get(&self.db, &service.installation_id)?;
        if !install.executable_exists {
            return Err(AppError::invalid_path(
                &install.executable_path,
                "the registered executable no longer exists; re-register the installation",
            ));
        }
        let adapter = web_server::adapter_for(service.component)
            .ok_or_else(|| AppError::Unsupported(format!("no adapter for {}", service.component)))?;
        Ok((service, install, adapter))
    }

    /// Renders and validates into a staged file without touching the active config.
    fn stage(
        &self,
        service: &Service,
        install: &Installation,
        adapter: &dyn ServiceAdapter,
    ) -> AppResult<(String, ValidationReport)> {
        let ctx = self.context(service)?;
        for dir in [&ctx.sites_dir, &ctx.logs_dir, &ctx.temp_dir] {
            std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
        }
        if adapter.supports_sites() {
            self.ensure_document_root(&service.id)?;
        }
        if let (Some(dir), false) = (&ctx.data_dir, adapter.needs_initialization()) {
            std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
        }

        self.ensure_default_cert(&ctx)?;
        let rendered = adapter.render_main_config(install, &ctx)?;
        let staged = self.config_path(service, adapter).with_extension("staged.conf");
        managed_files::write_managed(
            &self.db,
            &self.paths.backups_dir,
            &staged,
            &rendered,
            Owner { kind: OWNER_KIND, id: &service.id },
        )?;
        let report = adapter.validate(install, &staged)?;
        Ok((rendered, report))
    }

    /// Checks the main configuration with `sites_dir` included instead of the
    /// live sites folder (used to validate site changes before activation).
    pub fn check_with_sites_dir(
        &self,
        service: &Service,
        install: &Installation,
        adapter: &dyn ServiceAdapter,
        sites_dir: &std::path::Path,
    ) -> AppResult<ValidationReport> {
        let mut ctx = self.context(service)?;
        ctx.sites_dir = sites_dir.to_path_buf();
        for dir in [&ctx.logs_dir, &ctx.temp_dir] {
            std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
        }
        self.ensure_document_root(&service.id)?;
        self.ensure_default_cert(&ctx)?;
        let rendered = adapter.render_main_config(install, &ctx)?;
        let check_file = self.config_path(service, adapter).with_extension("sites-check.conf");
        managed_files::write_managed(
            &self.db,
            &self.paths.backups_dir,
            &check_file,
            &rendered,
            Owner { kind: OWNER_KIND, id: &service.id },
        )?;
        adapter.validate(install, &check_file)
    }

    /// Dry run: generate and check the configuration, report the result.
    pub fn validate(&self, id: &str) -> AppResult<ValidationReport> {
        let (service, install, adapter) = self.load(id)?;
        let (_, report) = self.stage(&service, &install, adapter)?;
        self.event(
            id,
            if report.ok { "info" } else { "warning" },
            if report.ok { "configuration check passed".into() } else { format!("configuration check failed: {}", first_line(&report.output)) },
        );
        Ok(report)
    }

    /// Validates, then atomically activates the configuration.
    fn activate(&self, service: &Service, install: &Installation, adapter: &dyn ServiceAdapter) -> AppResult<PathBuf> {
        let (rendered, report) = self.stage(service, install, adapter)?;
        if !report.ok {
            return Err(AppError::ConfigInvalid {
                message: first_line(&report.output).to_string(),
                output: report.output,
            });
        }
        let active = self.config_path(service, adapter);
        let outcome = managed_files::write_managed(
            &self.db,
            &self.paths.backups_dir,
            &active,
            &rendered,
            Owner { kind: OWNER_KIND, id: &service.id },
        )?;
        if let Some(backup) = outcome.backup {
            tracing::info!(service = %service.id, backup = %backup.display(), "previous config backed up");
        }
        Ok(active)
    }

    // ----- lifecycle -------------------------------------------------------

    fn op_lock(&self, id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.op_locks.lock().unwrap_or_else(|p| p.into_inner());
        locks.entry(id.to_string()).or_default().clone()
    }

    pub fn start(&self, id: &str) -> AppResult<ServiceStatus> {
        let lock = self.op_lock(id);
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let result = self.start_inner(id);
        match &result {
            Ok(_) => self.event(id, "info", "started".into()),
            Err(e) => self.event(id, "error", format!("start failed: {e}")),
        }
        result?;
        self.status(id)
    }

    fn start_inner(&self, id: &str) -> AppResult<()> {
        let (service, install, adapter) = self.load(id)?;
        if self.processes.is_running(id) {
            return Err(AppError::Conflict(format!("'{}' is already running", service.name)));
        }
        self.check_data_dir(&service, &install, adapter)?;
        let ctx = self.context(&service)?;
        let bind = ctx.listen_address;
        port_manager::ensure_available(ctx.port, bind)?;
        if let Some(https) = &ctx.https {
            port_manager::ensure_available(https.port, bind)?;
        }

        let config = self.activate(&service, &install, adapter)?;
        let process_log = ctx.logs_dir.join("process.log");
        let spec = ProcessSpec {
            program: adapter.launch_program(&install)?,
            args: adapter.launch_args(&install, &config),
            cwd: install.root_path.clone(),
            env: adapter.launch_env(&install, &ctx)?,
            log_file: process_log.clone(),
        };

        self.expected_stops.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
        self.reported_failures.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
        self.processes.spawn(id, &spec)?;
        self.active_https.lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string(), ctx.https.is_some());

        let mut diag_files = vec![process_log];
        diag_files.extend(adapter.log_files(&ctx).into_iter().map(|l| l.path));
        let target = SocketAddr::new(connect_address(bind), ctx.port);
        let ready_timeout = adapter.ready_timeout();
        let deadline = Instant::now() + ready_timeout;
        loop {
            if !self.processes.is_running(id) {
                let code = match self.processes.state(id) {
                    ProcessState::Exited { exit } => exit.code,
                    _ => None,
                };
                return Err(AppError::StartFailed {
                    message: format!("{} exited during startup (exit code {code:?})", service.name),
                    log_tail: log_manager::diagnostic_tail(&diag_files, 15),
                });
            }
            if TcpStream::connect_timeout(&target, Duration::from_millis(300)).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.mark_expected_stop(id);
                let _ = self.processes.stop(id, None, Duration::ZERO);
                return Err(AppError::StartFailed {
                    message: format!(
                        "{} did not accept connections on {target} within {}s",
                        service.name,
                        ready_timeout.as_secs()
                    ),
                    log_tail: log_manager::diagnostic_tail(&diag_files, 15),
                });
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn mark_expected_stop(&self, id: &str) {
        self.expected_stops.lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string());
    }

    pub fn stop(&self, id: &str) -> AppResult<ServiceStatus> {
        let lock = self.op_lock(id);
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        self.get(id)?;
        let control = self.load(id).ok().and_then(|(s, install, adapter)| {
            let ctx = self.context(&s).ok()?;
            Some((self.config_path(&s, adapter), install, adapter, ctx))
        });
        self.mark_expected_stop(id);
        let graceful = |pid: u32| match &control {
            Some((config, install, adapter, ctx)) => adapter.request_graceful_stop(&Control { install, ctx, config, pid }),
            None => Err(AppError::Unsupported("no graceful stop".into())),
        };
        let grace = control.as_ref().map_or(STOP_GRACE, |(_, _, adapter, _)| adapter.graceful_timeout());
        let method = self.processes.stop(id, Some(&graceful), grace)?;
        self.hosted_map().remove(id);
        let msg = match method {
            StopMethod::Graceful => "stopped gracefully",
            StopMethod::Terminated => "stopped (process tree terminated after graceful stop did not finish)",
            StopMethod::AlreadyExited => "was not running",
        };
        self.event(id, "info", msg.into());
        self.status(id)
    }

    pub fn restart(&self, id: &str) -> AppResult<ServiceStatus> {
        if self.processes.is_running(id) {
            self.stop(id)?;
        }
        self.start(id)
    }

    /// Validates and activates new configuration, then asks the running server
    /// to reload it. On validation failure the running server is untouched.
    pub fn reload(&self, id: &str) -> AppResult<ServiceStatus> {
        let lock = self.op_lock(id);
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let (service, install, adapter) = self.load(id)?;
        let pid = match self.processes.state(id) {
            ProcessState::Running { pid, .. } => pid,
            _ => return Err(AppError::Conflict(format!("'{}' is not running", service.name))),
        };
        let config = match self.activate(&service, &install, adapter) {
            Ok(config) => config,
            Err(e) => {
                self.event(id, "error", format!("reload aborted: {e}"));
                return Err(e);
            }
        };
        let ctx = self.context(&service)?;
        adapter.request_reload(&Control { install: &install, ctx: &ctx, config: &config, pid })?;
        self.event(id, "info", "configuration reloaded".into());
        self.status(id)
    }

    /// Other running web servers that would collide with `id` on its port.
    /// Web servers may share a port; only one of them can run at a time.
    pub fn running_rivals(&self, id: &str) -> AppResult<Vec<Service>> {
        let me = self.get(id)?;
        let bind = port_manager::parse_bind_address(&me.listen_address)?;
        let mut out = Vec::new();
        for other in self.list()? {
            let shares_port = other.http_port == me.http_port
                || (other.https_port.is_some() && (other.https_port == me.https_port || other.https_port == Some(me.http_port)))
                || me.https_port == Some(other.http_port);
            // A server behind another one does not hold its port.
            if other.id == me.id || !shares_port || !self.processes.is_running(&other.id) || self.hosted(&other.id).is_some() {
                continue;
            }
            let other_bind = port_manager::parse_bind_address(&other.listen_address)?;
            let overlap = other_bind == bind || other_bind.is_unspecified() || bind.is_unspecified();
            if overlap && other_bind.is_ipv4() == bind.is_ipv4() {
                out.push(other);
            }
        }
        Ok(out)
    }

    /// Refuses to start on a data folder that is not this server's data.
    fn check_data_dir(&self, service: &Service, install: &Installation, adapter: &dyn ServiceAdapter) -> AppResult<()> {
        let Some(dir) = service.data_dir.as_deref().filter(|_| adapter.needs_data_dir()) else { return Ok(()) };
        let info = adapter.inspect_data_dir(install, dir);
        match info.state {
            DataDirState::Initialized => Ok(()),
            DataDirState::Missing | DataDirState::Empty if !adapter.needs_initialization() => Ok(()),
            DataDirState::Missing | DataDirState::Empty => Err(AppError::Conflict(format!(
                "the data folder {} is not initialized yet; initialize it first",
                dir.display()
            ))),
            DataDirState::Foreign | DataDirState::Incompatible => Err(AppError::Validation(format!(
                "Zenv will not use {}: {}",
                dir.display(),
                info.detail
            ))),
        }
    }

    /// Creates a fresh data folder for a stopped database, only if the
    /// folder is missing or empty. Existing data is never touched.
    pub fn initialize_data_dir(&self, id: &str) -> AppResult<String> {
        let lock = self.op_lock(id);
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let (service, install, adapter) = self.load(id)?;
        if self.processes.is_running(id) {
            return Err(AppError::Conflict("stop the service before initializing its data folder".into()));
        }
        let ctx = self.context(&service)?;
        let dir = ctx.data_dir.clone().ok_or_else(|| AppError::Unsupported("this service has no data folder".into()))?;
        let info = adapter.inspect_data_dir(&install, &dir);
        if !matches!(info.state, DataDirState::Missing | DataDirState::Empty) {
            return Err(AppError::Conflict(format!("{} is already in use ({}); it was left untouched", dir.display(), info.detail)));
        }
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent).ctx(|| format!("creating {}", parent.display()))?;
        }
        let result = adapter.initialize_data_dir(&install, &ctx);
        match &result {
            Ok(_) => self.event(id, "info", format!("data folder initialized at {}", dir.display())),
            Err(e) => self.event(id, "error", format!("data folder initialization failed: {e}")),
        }
        result
    }

    /// Protocol-level connectivity check of a running service.
    pub fn probe(&self, id: &str) -> AppResult<String> {
        let service = self.get(id)?;
        let adapter = web_server::adapter_for(service.component)
            .ok_or_else(|| AppError::Unsupported(format!("no adapter for {}", service.component)))?;
        let addr = SocketAddr::new(connect_address(port_manager::parse_bind_address(&service.listen_address)?), service.http_port);
        adapter.probe(addr).map_err(|e| AppError::Platform(format!("{} did not answer as expected: {e}", service.name)))
    }

    /// Web servers only (the services that can host local sites).
    pub fn tls(&self) -> &Arc<Tls> {
        &self.tls
    }

    // ----- sharing a port ------------------------------------------------------

    fn hosted_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, u16)>> {
        self.hosted.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Runs `guest` (when next started) on `port` at 127.0.0.1, behind `front`.
    pub fn host_behind(&self, guest: &str, front: &str, port: u16) {
        self.hosted_map().insert(guest.to_string(), (front.to_string(), port));
    }

    pub fn unhost(&self, guest: &str) {
        self.hosted_map().remove(guest);
    }

    /// The front server and internal port of a server running behind one.
    pub fn hosted(&self, id: &str) -> Option<(String, u16)> {
        self.hosted_map().get(id).cloned()
    }

    /// Servers running behind `front`, with their internal ports.
    pub fn guests_of(&self, front: &str) -> Vec<(String, u16)> {
        let mut out: Vec<_> = self.hosted_map().iter().filter(|(_, (f, _))| f == front).map(|(g, (_, p))| (g.clone(), *p)).collect();
        out.sort();
        out
    }

    pub fn web_servers(&self) -> AppResult<Vec<Service>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| web_server::adapter_for(s.component).is_some_and(|a| a.supports_sites()))
            .collect())
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.processes.is_running(id)
    }

    /// Asks a running server to reload, then confirms it is still running and
    /// accepting connections a moment later.
    pub fn reload_checked(&self, id: &str) -> AppResult<()> {
        let lock = self.op_lock(id);
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let (service, install, adapter) = self.load(id)?;
        let ProcessState::Running { pid, .. } = self.processes.state(id) else {
            return Err(AppError::Conflict(format!("'{}' is not running", service.name)));
        };
        let config = self.config_path(&service, adapter);
        let ctx = self.context(&service)?;
        adapter.request_reload(&Control { install: &install, ctx: &ctx, config: &config, pid })?;
        let target = SocketAddr::new(connect_address(ctx.listen_address), ctx.port);
        // The configuration was validated beforehand; this only has to catch a
        // server that dies while swapping its worker processes.
        std::thread::sleep(Duration::from_millis(600));
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if !self.processes.is_running(id) {
                let mut files = vec![ctx.logs_dir.join("process.log")];
                files.extend(adapter.log_files(&ctx).into_iter().map(|l| l.path));
                return Err(AppError::StartFailed {
                    message: format!("'{}' exited while reloading its configuration", service.name),
                    log_tail: log_manager::diagnostic_tail(&files, 15),
                });
            }
            if TcpStream::connect_timeout(&target, Duration::from_millis(300)).is_ok() {
                self.event(id, "info", "configuration reloaded (sites changed)".into());
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AppError::StartFailed {
                    message: format!("'{}' stopped accepting connections after reloading", service.name),
                    log_tail: vec![],
                });
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    pub fn status(&self, id: &str) -> AppResult<ServiceStatus> {
        let service = self.get(id)?;
        self.status_of(service)
    }

    pub fn statuses(&self) -> AppResult<Vec<ServiceStatus>> {
        self.list()?.into_iter().map(|s| self.status_of(s)).collect()
    }

    fn status_of(&self, service: Service) -> AppResult<ServiceStatus> {
        let installation = runtime_manager::get(&self.db, &service.installation_id).ok();
        let process = self.processes.state(&service.id);
        let health = match &process {
            ProcessState::Running { .. } => Health::Running,
            ProcessState::NotStarted => Health::Stopped,
            ProcessState::Exited { exit } => {
                let expected = self.expected_stops.lock().unwrap_or_else(|p| p.into_inner()).contains(&service.id);
                if expected {
                    Health::Stopped
                } else {
                    let newly = self.reported_failures.lock().unwrap_or_else(|p| p.into_inner()).insert(service.id.clone());
                    if newly {
                        self.event(&service.id, "error", format!("exited unexpectedly (exit code {:?})", exit.code));
                    }
                    Health::Failed
                }
            }
        };
        let host = match port_manager::parse_bind_address(&service.listen_address).map(connect_address) {
            Ok(IpAddr::V6(a)) => format!("[{a}]"),
            Ok(IpAddr::V4(a)) if a.is_loopback() => "localhost".to_string(),
            Ok(a) => a.to_string(),
            Err(_) => "localhost".to_string(),
        };
        let adapter = web_server::adapter_for(service.component);
        let config_path = adapter.map(|a| self.config_path(&service, a)).unwrap_or_default();
        let ctx = self.context(&service).ok();
        let connection = adapter.zip(ctx.as_ref()).and_then(|(a, c)| a.connection_info(c));
        let data_dir_info = match (adapter, &installation, &service.data_dir) {
            (Some(a), Some(inst), Some(dir)) if a.needs_data_dir() => Some(a.inspect_data_dir(inst, dir)),
            _ => None,
        };
        Ok(ServiceStatus {
            url: connection.as_ref().map_or_else(|| format!("http://{host}:{}/", service.http_port), |c| c.url.clone()),
            kind: runtime_catalog::info(service.component).category,
            data_dir_info,
            connection,
            behind: self.hosted(&service.id).and_then(|(front, _)| self.get(&front).ok()).map(|f| f.name),
            needs_initialization: adapter.is_some_and(|a| a.needs_initialization()),
            document_root: self.document_root(&service.id),
            config_path,
            installation,
            health,
            process,
            service,
        })
    }

    // ----- logs & events ----------------------------------------------------

    fn log_files(&self, service: &Service) -> AppResult<Vec<LogFile>> {
        let ctx = self.context(service)?;
        let mut files = vec![LogFile {
            id: "process",
            label: "Process output (stdout/stderr)",
            path: ctx.logs_dir.join("process.log"),
        }];
        if let Some(a) = web_server::adapter_for(service.component) {
            files.extend(a.log_files(&ctx));
        }
        Ok(files)
    }

    pub fn log_sources(&self, id: &str) -> AppResult<Vec<LogSource>> {
        let service = self.get(id)?;
        Ok(self
            .log_files(&service)?
            .into_iter()
            .map(|f| {
                let meta = std::fs::metadata(&f.path).ok();
                LogSource {
                    id: f.id.to_string(),
                    label: f.label.to_string(),
                    exists: meta.is_some(),
                    size_bytes: meta.map_or(0, |m| m.len()),
                    path: f.path,
                }
            })
            .collect())
    }

    /// The file behind one of the service's allow-listed logs.
    pub fn log_file(&self, id: &str, log_id: &str) -> AppResult<PathBuf> {
        let service = self.get(id)?;
        self.log_files(&service)?
            .into_iter()
            .find(|f| f.id == log_id)
            .map(|f| f.path)
            .ok_or_else(|| AppError::NotFound { entity: "log", id: log_id.to_string() })
    }

    /// Reads one of the service's allow-listed log files.
    pub fn read_log(&self, id: &str, log_id: &str, lines: usize) -> AppResult<LogTail> {
        let service = self.get(id)?;
        let file = self
            .log_files(&service)?
            .into_iter()
            .find(|f| f.id == log_id)
            .ok_or_else(|| AppError::NotFound { entity: "log", id: log_id.to_string() })?;
        log_manager::tail(&file.path, lines)
    }

    pub fn events(&self, id: &str, limit: u32) -> AppResult<Vec<ServiceEvent>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, level, message, created_at FROM service_events
                 WHERE service_id = ?1 ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![id, limit.clamp(1, 500)], |r| {
                Ok(ServiceEvent { id: r.get(0)?, level: r.get(1)?, message: r.get(2)?, created_at: r.get(3)? })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    fn event(&self, id: &str, level: &str, message: String) {
        let result = self.db.write(|tx| {
            tx.execute(
                "INSERT INTO service_events (service_id, level, message, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, level, message, now_rfc3339()],
            )?;
            Ok(())
        });
        if let Err(e) = result {
            tracing::warn!(service = id, error = %e, "could not record service event");
        }
    }

    // ----- app lifecycle ----------------------------------------------------

    pub fn start_autostart_services(&self) {
        let Ok(services) = self.list() else { return };
        for s in services.into_iter().filter(|s| s.autostart) {
            if let Err(e) = self.start(&s.id) {
                tracing::warn!(service = %s.name, error = %e, "autostart failed");
            }
        }
    }

    /// Stops every running service in parallel (graceful first).
    pub fn shutdown_all(&self) {
        let keys = self.processes.running_keys();
        std::thread::scope(|scope| {
            for key in &keys {
                scope.spawn(move || {
                    if let Err(e) = self.stop(key) {
                        tracing::warn!(service = %key, error = %e, "stop on exit failed");
                    }
                });
            }
        });
    }
}

fn first_line(s: &str) -> &str {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no output")
}

const DEFAULT_INDEX: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Zenv</title>
<style>body{font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#1f2937}code{background:#f3f4f6;padding:.1rem .3rem;border-radius:.25rem}</style>
</head>
<body>
<h1>It works — served by Zenv</h1>
<p data-zenv="default-index">This is the default document root of a Zenv-managed web server.
Local sites with their own domains arrive in a later Zenv release.</p>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_manager::{probe, InstallSource};

    fn setup() -> (tempfile::TempDir, ServiceManager, Installation) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        paths.ensure().unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let install = runtime_manager::insert(
            &db,
            ComponentId::Apache,
            &probe::Located { root: r"C:\Apache24".into(), executable: r"C:\Apache24\bin\httpd.exe".into() },
            &probe::VersionInfo { version: "2.4.67".into(), detail: String::new() },
            InstallSource::External,
        )
        .unwrap();
        (dir, ServiceManager::new(db, paths.clone(), Arc::new(Tls::new(&paths))), install)
    }

    fn input(name: &str, install: &Installation, addr: &str, port: u16) -> ServiceInput {
        ServiceInput {
            name: name.into(),
            installation_id: install.id.clone(),
            listen_address: addr.into(),
            http_port: port,
            autostart: false,
            data_dir: None,
            https_port: None,
        }
    }

    #[test]
    fn create_writes_default_page_and_event() {
        let (_dir, sm, install) = setup();
        let s = sm.create(&input("Main", &install, "127.0.0.1", 8080)).unwrap();
        let index = sm.document_root(&s.id).join("index.html");
        assert!(std::fs::read_to_string(index).unwrap().contains("zenv"));
        assert_eq!(sm.events(&s.id, 10).unwrap().len(), 1);
        let status = sm.status(&s.id).unwrap();
        assert_eq!(status.health, Health::Stopped);
        assert_eq!(status.url, "http://localhost:8080/");
    }

    #[test]
    fn servers_may_share_a_port_but_not_a_name() {
        let (_dir, sm, install) = setup();
        sm.create(&input("A", &install, "127.0.0.1", 8080)).unwrap();
        // Apache and Nginx on the same port is allowed; only one runs at a time.
        sm.create(&input("B", &install, "0.0.0.0", 8080)).unwrap();
        let dup_name = sm.create(&input("a", &install, "127.0.0.1", 8081)).unwrap_err();
        assert!(dup_name.to_string().contains("already exists"));
    }

    #[test]
    fn rejects_bad_input() {
        let (_dir, sm, install) = setup();
        assert!(sm.create(&input("", &install, "127.0.0.1", 80)).is_err());
        assert!(sm.create(&input("x", &install, "localhost", 80)).is_err(), "must be an IP");
        assert!(sm.create(&input("x", &install, "127.0.0.1", 0)).is_err());
        let mut bad = input("x", &install, "127.0.0.1", 80);
        bad.installation_id = "missing".into();
        assert!(matches!(sm.create(&bad), Err(AppError::NotFound { .. })));
    }

    #[test]
    fn services_need_an_adapter() {
        let (_dir, sm, _) = setup();
        let php = runtime_manager::insert(
            &sm.db,
            ComponentId::Php,
            &probe::Located { root: r"C:\php".into(), executable: r"C:\php\php.exe".into() },
            &probe::VersionInfo { version: "8.4.21".into(), detail: String::new() },
            InstallSource::External,
        )
        .unwrap();
        let err = sm.create(&input("php", &php, "127.0.0.1", 9000)).unwrap_err();
        assert!(matches!(err, AppError::Unsupported(_)));
    }

    #[test]
    fn missing_executable_blocks_start_with_clear_error() {
        let (_dir, sm, install) = setup();
        let s = sm.create(&input("Main", &install, "127.0.0.1", 8080)).unwrap();
        let err = sm.start(&s.id).unwrap_err();
        assert!(matches!(err, AppError::InvalidPath { .. }), "{err}");
        let events = sm.events(&s.id, 10).unwrap();
        assert!(events[0].message.starts_with("start failed"));
    }

    #[test]
    fn delete_removes_record_but_keeps_installation() {
        let (_dir, sm, install) = setup();
        let s = sm.create(&input("Main", &install, "127.0.0.1", 8080)).unwrap();
        sm.delete(&s.id).unwrap();
        assert!(matches!(sm.get(&s.id), Err(AppError::NotFound { .. })));
        assert!(runtime_manager::get(&sm.db, &install.id).is_ok());
    }

    #[test]
    fn unknown_log_ids_are_rejected() {
        let (_dir, sm, install) = setup();
        let s = sm.create(&input("Main", &install, "127.0.0.1", 8080)).unwrap();
        let ids: Vec<_> = sm.log_sources(&s.id).unwrap().into_iter().map(|l| l.id).collect();
        assert_eq!(ids, vec!["process", "error", "access"]);
        assert!(matches!(sm.read_log(&s.id, "../../secrets", 10), Err(AppError::NotFound { .. })));
    }
}
