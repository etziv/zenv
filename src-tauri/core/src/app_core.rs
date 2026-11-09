use std::sync::{Arc, Weak};
use std::time::Duration;

use serde::Serialize;
use tracing_appender::non_blocking::WorkerGuard;

use crate::error::{AppError, AppResult};
use crate::local_sites::SiteManager;
use crate::paths::AppPaths;
use crate::php_runtime::{PhpRuntimes, RuntimeProcess};
use crate::runtime_installer::Installer;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, Installation};
use crate::service_manager::{Service, ServiceInput, ServiceManager, ServiceStatus};
use crate::storage::Db;
use crate::thumbnails::{Thumbnail, Thumbnails};
use crate::tls::Tls;

/// Composition root: everything the commands operate on.
pub struct Core {
    pub paths: AppPaths,
    pub db: Arc<Db>,
    pub services: Arc<ServiceManager>,
    pub runtimes: Arc<PhpRuntimes>,
    pub sites: SiteManager,
    pub installer: Installer,
    pub tls: Arc<Tls>,
    pub thumbnails: Thumbnails,
    /// Programs behind proxy apps (cargo run, npm run dev, ...).
    pub app_processes: crate::app_process::AppProcesses,
    /// Studio terminals (coding agents and shells in app folders).
    pub studio: crate::studio::Terminals,
    /// Studio's Claude Code chats.
    pub chats: crate::studio_chat::Chats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub schema_version: u32,
    pub os: &'static str,
    pub arch: &'static str,
    pub paths: AppPaths,
}

impl Core {
    pub fn initialize(paths: AppPaths) -> AppResult<Self> {
        paths.ensure()?;
        secure_home(&paths.root);
        let db =Arc::new(Db::open(&paths.db_file, &paths.backups_dir)?);
        crate::layout::fix_references(&db, &paths)?;
        tracing::info!(schema = db.schema_version(), root = %paths.root.display(), "Zenv core initialized");
        let tls = Arc::new(Tls::new(&paths));
        let services = Arc::new(ServiceManager::new(db.clone(), paths.clone(), tls.clone()));
        let runtimes = Arc::new(PhpRuntimes::new(db.clone(), paths.clone()));
        let sites = SiteManager::new(db.clone(), paths.clone(), services.clone(), runtimes.clone());
        spawn_supervisor(Arc::downgrade(&runtimes));
        let installer = Installer::new(db.clone(), paths.clone());
        let thumbnails = Thumbnails::new(&paths);
        let core = Core { services, runtimes, sites, installer, tls, thumbnails, app_processes: Default::default(), studio: Default::default(), chats: Default::default(), db, paths };
        if let Err(e) = core.adopt_detected_commands() {
            tracing::warn!(error = %e, "start commands of existing apps not saved");
        }
        Ok(core)
    }

    /// Starts a web server after regenerating its sites, so PHP workers run
    /// and site configurations point at their current ports.
    pub fn start_service(&self, id: &str) -> AppResult<ServiceStatus> {
        let is_web = self.services.get(id).ok().and_then(|s| crate::web_server::adapter_for(s.component)).is_some_and(|a| a.supports_sites());
        let rivals = self.services.running_rivals(id)?;
        if is_web && !self.services.is_running(id) {
            // Web servers share a port by running behind the one that holds
            // it: each still serves its own apps, the front passes requests on.
            if let Some(front) = rivals.first() {
                let status = self.start_behind(id, &front.id)?;
                self.start_apps_of(id);
                return Ok(status);
            }
        }
        for rival in rivals {
            tracing::info!(stopping = %rival.name, "freeing a shared port");
            self.services.stop(&rival.id)?;
        }
        let has_sites = is_web && self.sites.count_for_service(id)? > 0;
        if has_sites {
            self.sites.reapply(id)?;
        }
        let status = self.services.start(id)?;
        if has_sites {
            self.sites.mark_applied(id, "active")?;
        }
        // The programs behind its proxy apps (cargo run, npm run dev, ...).
        if is_web {
            self.start_apps_of(id);
        }
        Ok(status)
    }

    /// Starts web server `id` on an internal port behind `front`, then lets
    /// `front` pass the requests for `id`'s apps on to it.
    fn start_behind(&self, id: &str, front: &str) -> AppResult<ServiceStatus> {
        let port = crate::port_manager::suggest_free_port()?;
        self.services.host_behind(id, front, port);
        let started = (|| {
            if self.sites.count_for_service(id)? > 0 {
                self.sites.reapply(id)?;
            }
            let status = self.services.start(id)?;
            self.sites.reapply(front)?;
            self.sites.mark_applied(id, "active")?;
            Ok(status)
        })();
        if started.is_err() {
            let _ = self.services.stop(id);
            self.services.unhost(id);
            let _ = self.sites.reapply(front);
        }
        started
    }

    /// Stops a service. A web server holding a port for others hands it to
    /// them (the first takes the port, the rest run behind it again); one
    /// running behind another is dropped from the front's forwarding.
    pub fn stop_service(&self, id: &str) -> AppResult<ServiceStatus> {
        let front = self.services.hosted(id).map(|(f, _)| f);
        let guests = self.services.guests_of(id);
        let status = self.services.stop(id)?;
        if let Some(f) = front.filter(|f| self.services.is_running(f)) {
            self.sites.reapply(&f)?;
        }
        for (guest, _) in guests {
            self.services.stop(&guest)?;
            if let Err(e) = self.start_service(&guest) {
                tracing::warn!(service = %guest, error = %e, "could not move onto the freed port");
            }
        }
        Ok(status)
    }

    pub fn restart_service(&self, id: &str) -> AppResult<ServiceStatus> {
        if self.services.is_running(id) {
            self.services.stop(id)?;
        }
        self.start_service(id)
    }

    /// Updates a web server and regenerates the sites it serves, since their
    /// virtual hosts embed its address, port and version-specific features.
    pub fn update_service(&self, id: &str, input: &ServiceInput) -> AppResult<Service> {
        let service = self.services.update(id, input)?;
        let is_web = crate::web_server::adapter_for(service.component).is_some_and(|a| a.supports_sites());
        if is_web && self.sites.count_for_service(id)? > 0 {
            self.sites.reapply(id)?;
        }
        Ok(service)
    }

    /// Switches an installation on or off. PHP versions still used by enabled
    /// sites cannot be switched off; switching FrankenPHP re-routes PHP sites
    /// (engine "auto") between FrankenPHP and PHP-CGI immediately.
    pub fn set_installation_enabled(&self, id: &str, enabled: bool) -> AppResult<Installation> {
        let inst = runtime_manager::get(&self.db, id)?;
        // Languages are always available; each site picks its version.
        if !enabled && crate::runtime_catalog::info(inst.component).category == crate::runtime_catalog::Category::Language {
            return Err(AppError::Validation(format!(
                "{} versions are always available; remove {} from the list instead",
                crate::runtime_catalog::info(inst.component).name,
                inst.version
            )));
        }
        let updated = runtime_manager::set_enabled(&self.db, id, enabled)?;
        if matches!(inst.component, ComponentId::Php | ComponentId::Frankenphp) {
            for service_id in self.sites.services_with_php_sites()? {
                self.sites.reapply(&service_id)?;
            }
        }
        Ok(updated)
    }

    /// Takes a fresh screenshot of an app for the Apps grid. Needs the app
    /// enabled and a web server running.
    pub fn capture_thumbnail(&self, site_id: &str) -> AppResult<Thumbnail> {
        let view = self.sites.view(site_id)?;
        if !view.site.enabled {
            return Err(AppError::Conflict(format!("{} is switched off", view.site.domain)));
        }
        let serving = match &view.site.service_id {
            Some(id) => self.services.is_running(id),
            None => self.services.web_servers()?.iter().any(|s| self.services.is_running(&s.id)),
        };
        if !serving {
            return Err(AppError::Conflict("start a web server to preview apps".into()));
        }
        self.thumbnails.capture(site_id, &view.url)
    }

    /// Opens a terminal in `cwd` where the chosen versions come first on PATH.
    pub fn open_terminal(&self, installation_ids: &[String], cwd: Option<&str>) -> AppResult<()> {
        let installs = installation_ids
            .iter()
            .map(|id| runtime_manager::get(&self.db, id))
            .collect::<AppResult<Vec<_>>>()?;
        let dir = match cwd.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => std::path::PathBuf::from(c),
            None => self.paths.root.clone(),
        };
        if !dir.is_dir() {
            return Err(AppError::invalid_path(&dir, "is not an existing folder"));
        }
        let env = crate::runtime_env::build(&installs, std::env::var_os("PATH"));
        let banner = installs.iter().map(|i| format!("{} {}", i.component, i.version)).collect::<Vec<_>>().join(", ");
        crate::platform::open_console(&dir, &env, &format!("Zenv terminal: {banner}"))
    }

    pub fn runtime_status(&self) -> Vec<RuntimeProcess> {
        self.runtimes.status()
    }

    pub fn start_autostart_services(&self) {
        let Ok(services) = self.services.list() else { return };
        for s in services.into_iter().filter(|s| s.autostart) {
            if let Err(e) = self.start_service(&s.id) {
                tracing::warn!(service = %s.name, error = %e, "autostart failed");
            }
        }
    }

    /// Stops app programs, web servers (gracefully) and PHP runtimes. Called on app exit.
    pub fn shutdown(&self) {
        self.studio.close_all();
        self.chats.close_all();
        self.app_processes.stop_all();
        self.services.shutdown_all();
        self.runtimes.shutdown_all();
    }

    pub fn info(&self) -> AppInfo {
        AppInfo {
            version: env!("CARGO_PKG_VERSION"),
            schema_version: self.db.schema_version(),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            paths: self.paths.clone(),
        }
    }
}

/// Limits the home to this user, SYSTEM and Administrators (a folder made on
/// `C:\` lets every account change it). Changing a large existing home takes
/// a while, so it happens in the background.
fn secure_home(root: &std::path::Path) {
    let root = root.to_path_buf();
    std::thread::Builder::new()
        .name("secure-home".into())
        .spawn(move || match crate::platform::secure_folder(&root) {
            Ok(crate::platform::FolderAccess::Private) => {}
            Ok(access) => tracing::error!(home = %root.display(), ?access, "other accounts on this computer can still reach the Zenv home"),
            Err(e) => tracing::error!(home = %root.display(), error = %e, "cannot limit access to the Zenv home"),
        })
        .ok();
}

/// Restarts exited php-cgi workers every two seconds while the core lives.
fn spawn_supervisor(runtimes: Weak<PhpRuntimes>) {
    std::thread::Builder::new()
        .name("php-supervisor".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            match runtimes.upgrade() {
                Some(rt) => rt.supervise(),
                None => return,
            }
        })
        .ok();
}

/// File logging to `<data>/logs/zenv.log`. Secrets must never be logged;
/// nothing Zenv handles in this milestone is secret.
pub fn init_logging(paths: &AppPaths) -> Option<WorkerGuard> {
    use tracing_subscriber::{fmt, EnvFilter};
    let appender = tracing_appender::rolling::never(&paths.logs_dir, "zenv.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("ZENV_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).try_init().ok()?;
    Some(guard)
}
