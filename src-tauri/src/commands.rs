//! The complete IPC surface. Every command is a narrowly scoped operation on
//! typed input; none accepts a program, argument list or arbitrary file path
//! to execute or read.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use zenv_core::app_core::{AppInfo, Core};
use zenv_core::diagnostics::{self, Check};
use zenv_core::domain_manager::hosts::ApplyOutcome;
use zenv_core::local_sites::detect::DetectedProject;
use zenv_core::php_runtime::{RuntimeLogGroup, RuntimeProcess};
use zenv_core::runtime_installer::{AvailableVersion, InstallJob};
use zenv_core::local_sites::{HostsStatus, SiteInput, SitePage, SiteQuery, SiteView};
use zenv_core::virtual_host::ApplyReport;
use zenv_core::error::{AppError, AppResult};
use zenv_core::log_manager::{self, LogTail};
use zenv_core::port_manager::{self, PortInspection};
use zenv_core::runtime_catalog::{self, ComponentId, ComponentInfo};
use zenv_core::runtime_manager::{self, DiscoveredInstallation, Installation};
use zenv_core::service_manager::{LogSource, Service, ServiceEvent, ServiceInput, ServiceStatus};
use zenv_core::app_services::{NeededService, StartOutcome};
use zenv_core::thumbnails::Thumbnail;
use zenv_core::tailnet::{TailnetApp, TailnetStatus};
use zenv_core::path_env::PathStatus;
use zenv_core::ai_router::{RouterModel, RouterStatus};
use zenv_core::docker_apps::{DockerBinding, DockerContainer};
use zenv_core::codex::CodexStatus;
use zenv_core::app_process::AppProcessStatus;
use zenv_core::editors::{EditorId, Editors};
use zenv_core::project_files::{self, Entry, FileContent, Listing, NewKind};
use zenv_core::studio::{Agent, AgentInfo, EventSink, TerminalEvent};
use zenv_core::studio_chat::{Attachment, Catalog, ChatControl, ChatOptions, ChatSink};
use zenv_core::studio_history::{self, SessionSummary};
use zenv_core::tls::CaStatus;
use zenv_core::web_server::ValidationReport;

type CoreState<'a> = State<'a, Arc<Core>>;

/// Runs blocking work (process spawns, probes, SQLite) off the async runtime.
async fn blocking<T, F>(core: &Arc<Core>, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Core) -> AppResult<T> + Send + 'static,
{
    let core = core.clone();
    tauri::async_runtime::spawn_blocking(move || f(&core))
        .await
        .map_err(|e| AppError::Platform(format!("background task failed: {e}")))?
}

#[tauri::command]
pub async fn app_info(core: CoreState<'_>) -> AppResult<AppInfo> {
    Ok(core.info())
}

#[tauri::command]
pub async fn catalog_list() -> AppResult<Vec<ComponentInfo>> {
    Ok(runtime_catalog::catalog().to_vec())
}

#[tauri::command]
pub async fn installations_list(core: CoreState<'_>, component: Option<ComponentId>) -> AppResult<Vec<Installation>> {
    blocking(&core, move |c| runtime_manager::list(&c.db, component)).await
}

#[tauri::command]
pub async fn installations_discover(
    core: CoreState<'_>,
    component: Option<ComponentId>,
) -> AppResult<Vec<DiscoveredInstallation>> {
    blocking(&core, move |c| runtime_manager::discover(&c.db, component, std::slice::from_ref(&c.paths.bin_dir))).await
}

#[tauri::command]
pub async fn installation_register(core: CoreState<'_>, component: ComponentId, path: String) -> AppResult<Installation> {
    blocking(&core, move |c| runtime_manager::register(&c.db, component, &PathBuf::from(path))).await
}

#[tauri::command]
pub async fn installation_unregister(core: CoreState<'_>, id: String) -> AppResult<()> {
    blocking(&core, move |c| runtime_manager::unregister(&c.db, &id)).await
}

#[tauri::command]
pub async fn installation_set_default(core: CoreState<'_>, id: String) -> AppResult<Installation> {
    blocking(&core, move |c| runtime_manager::set_default(&c.db, &id)).await
}

/// Switches an installation on or off (PHP sites are re-routed immediately).
#[tauri::command]
pub async fn installation_set_enabled(core: CoreState<'_>, id: String, enabled: bool) -> AppResult<Installation> {
    blocking(&core, move |c| c.set_installation_enabled(&id, enabled)).await
}

/// Versions the component's official source offers (verified metadata).
#[tauri::command]
pub async fn installer_available(core: CoreState<'_>, component: ComponentId) -> AppResult<Vec<AvailableVersion>> {
    blocking(&core, move |c| c.installer.available(component)).await
}

/// Starts downloading and installing a version into the Zenv home.
#[tauri::command]
pub async fn installer_install(core: CoreState<'_>, component: ComponentId, version: String) -> AppResult<String> {
    blocking(&core, move |c| c.installer.start(component, &version)).await
}

#[tauri::command]
pub async fn installer_jobs(core: CoreState<'_>) -> AppResult<Vec<InstallJob>> {
    Ok(core.installer.jobs())
}

/// Deletes a version Zenv installed (external installations are never deleted).
#[tauri::command]
pub async fn installation_uninstall(core: CoreState<'_>, id: String) -> AppResult<()> {
    blocking(&core, move |c| c.installer.uninstall(&id)).await
}

/// Opens a terminal with the chosen versions first on PATH.
#[tauri::command]
pub async fn open_terminal(core: CoreState<'_>, installation_ids: Vec<String>, cwd: Option<String>) -> AppResult<()> {
    blocking(&core, move |c| c.open_terminal(&installation_ids, cwd.as_deref())).await
}

/// PHP-CGI pools and FrankenPHP instances currently running.
#[tauri::command]
pub async fn runtimes_status(core: CoreState<'_>) -> AppResult<Vec<RuntimeProcess>> {
    Ok(core.runtime_status())
}

#[tauri::command]
pub async fn installation_refresh(core: CoreState<'_>, id: String) -> AppResult<Installation> {
    blocking(&core, move |c| runtime_manager::refresh(&c.db, &id)).await
}

#[tauri::command]
pub async fn services_list(core: CoreState<'_>) -> AppResult<Vec<ServiceStatus>> {
    blocking(&core, |c| c.services.statuses()).await
}

#[tauri::command]
pub async fn service_create(core: CoreState<'_>, input: ServiceInput) -> AppResult<Service> {
    blocking(&core, move |c| c.services.create(&input)).await
}

#[tauri::command]
pub async fn service_update(core: CoreState<'_>, id: String, input: ServiceInput) -> AppResult<Service> {
    blocking(&core, move |c| c.update_service(&id, &input)).await
}

/// Returns generated files that were kept because they had been edited.
#[tauri::command]
pub async fn service_delete(core: CoreState<'_>, id: String) -> AppResult<Vec<PathBuf>> {
    blocking(&core, move |c| c.services.delete(&id)).await
}

#[tauri::command]
pub async fn service_validate(core: CoreState<'_>, id: String) -> AppResult<ValidationReport> {
    blocking(&core, move |c| c.services.validate(&id)).await
}

#[tauri::command]
pub async fn service_start(core: CoreState<'_>, id: String) -> AppResult<ServiceStatus> {
    blocking(&core, move |c| c.start_service(&id)).await
}

#[tauri::command]
pub async fn service_stop(core: CoreState<'_>, id: String) -> AppResult<ServiceStatus> {
    blocking(&core, move |c| c.stop_service(&id)).await
}

#[tauri::command]
pub async fn service_restart(core: CoreState<'_>, id: String) -> AppResult<ServiceStatus> {
    blocking(&core, move |c| c.restart_service(&id)).await
}

#[tauri::command]
pub async fn service_reload(core: CoreState<'_>, id: String) -> AppResult<ServiceStatus> {
    blocking(&core, move |c| c.services.reload(&id)).await
}

/// Creates a fresh data folder for a database. Refuses non-empty folders.
#[tauri::command]
pub async fn service_initialize_data(core: CoreState<'_>, id: String) -> AppResult<String> {
    blocking(&core, move |c| c.services.initialize_data_dir(&id)).await
}

/// Protocol-level check that a running service answers.
#[tauri::command]
pub async fn service_probe(core: CoreState<'_>, id: String) -> AppResult<String> {
    blocking(&core, move |c| c.services.probe(&id)).await
}

#[tauri::command]
pub async fn service_events(core: CoreState<'_>, id: String, limit: Option<u32>) -> AppResult<Vec<ServiceEvent>> {
    blocking(&core, move |c| c.services.events(&id, limit.unwrap_or(50))).await
}

#[tauri::command]
pub async fn service_log_sources(core: CoreState<'_>, id: String) -> AppResult<Vec<LogSource>> {
    blocking(&core, move |c| c.services.log_sources(&id)).await
}

#[tauri::command]
pub async fn service_log_read(core: CoreState<'_>, id: String, log_id: String, lines: Option<usize>) -> AppResult<LogTail> {
    blocking(&core, move |c| c.services.read_log(&id, &log_id, lines.unwrap_or(300))).await
}

#[tauri::command]
pub async fn app_log_read(core: CoreState<'_>, lines: Option<usize>) -> AppResult<LogTail> {
    blocking(&core, move |c| log_manager::tail(&c.paths.app_log_file(), lines.unwrap_or(300))).await
}

#[tauri::command]
pub async fn port_inspect(core: CoreState<'_>, port: u16, address: Option<String>) -> AppResult<PortInspection> {
    blocking(&core, move |_| {
        let bind: IpAddr = match address {
            Some(a) => port_manager::parse_bind_address(&a)?,
            None => IpAddr::from([127, 0, 0, 1]),
        };
        if port == 0 {
            return Err(AppError::Validation("port must be between 1 and 65535".into()));
        }
        port_manager::inspect(port, bind)
    })
    .await
}

#[tauri::command]
pub async fn diagnostics_run(core: CoreState<'_>) -> AppResult<Vec<Check>> {
    blocking(&core, diagnostics::run).await
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RevealTarget {
    DataDir,
    LogsDir,
    ServiceConfig { id: String },
    ServiceDocumentRoot { id: String },
    ServiceLogs { id: String },
    ServiceDataDir { id: String },
    InstallationRoot { id: String },
    SiteProjectRoot { id: String },
    SiteConfig { id: String },
    /// One log file, opened in the default app for `.log` files.
    SiteLog { id: String, log: String },
    ServiceLog { id: String, log: String },
    RuntimeLog { id: String, log: String },
    AppLog,
}

/// Opens a Zenv-known location in Explorer. Paths are resolved here from
/// IDs, never taken from the frontend.
#[tauri::command]
pub async fn reveal(app: AppHandle, core: CoreState<'_>, target: RevealTarget) -> AppResult<()> {
    let (path, select) = blocking(&core, move |c| {
        Ok(match target {
            RevealTarget::DataDir => (c.paths.root.clone(), false),
            RevealTarget::LogsDir => (c.paths.logs_dir.clone(), false),
            RevealTarget::ServiceConfig { id } => (c.services.status(&id)?.config_path, true),
            RevealTarget::ServiceDocumentRoot { id } => (c.services.status(&id)?.document_root, false),
            RevealTarget::ServiceDataDir { id } => (
                c.services.get(&id)?.data_dir.ok_or_else(|| AppError::NotFound { entity: "data folder", id: id.clone() })?,
                false,
            ),
            RevealTarget::ServiceLogs { id } => (c.paths.service_logs_dir(&c.services.get(&id)?.id), false),
            RevealTarget::InstallationRoot { id } => (runtime_manager::get(&c.db, &id)?.root_path, false),
            RevealTarget::SiteProjectRoot { id } => (c.sites.get(&id)?.project_root, false),
            RevealTarget::SiteConfig { id } => (c.sites.config_file(&id)?, true),
            RevealTarget::SiteLog { id, log } => (c.sites.log_file(&id, &log)?, false),
            RevealTarget::ServiceLog { id, log } => (c.services.log_file(&id, &log)?, false),
            RevealTarget::RuntimeLog { id, log } => (c.runtimes.read_log(&id, &log, 1)?.path, false),
            RevealTarget::AppLog => (c.paths.app_log_file(), false),
        })
    })
    .await?;
    if !path.exists() {
        return Err(AppError::invalid_path(&path, "does not exist yet"));
    }
    let opener = app.opener();
    let result = if select {
        opener.reveal_item_in_dir(&path)
    } else {
        opener.open_path(path.to_string_lossy(), None::<&str>)
    };
    result.map_err(|e| AppError::Platform(format!("cannot open {}: {e}", path.display())))
}

#[tauri::command]
pub async fn open_service_url(app: AppHandle, core: CoreState<'_>, id: String) -> AppResult<()> {
    let url = blocking(&core, move |c| Ok(c.services.status(&id)?.url)).await?;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Platform(format!("cannot open {url}: {e}")))
}

// ----- local sites -----------------------------------------------------------

#[tauri::command]
pub async fn sites_list(core: CoreState<'_>, query: SiteQuery) -> AppResult<SitePage> {
    blocking(&core, move |c| c.sites.list(&query)).await
}

#[tauri::command]
pub async fn site_get(core: CoreState<'_>, id: String) -> AppResult<SiteView> {
    blocking(&core, move |c| c.sites.view(&id)).await
}

#[tauri::command]
pub async fn site_detect(core: CoreState<'_>, path: String) -> AppResult<DetectedProject> {
    blocking(&core, move |c| c.detect_project(&path)).await
}

#[tauri::command]
pub async fn site_create(core: CoreState<'_>, input: SiteInput) -> AppResult<SiteView> {
    blocking(&core, move |c| {
        let view = c.sites.create(&input)?;
        c.sync_app_process(&view.site.id, false);
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn site_update(core: CoreState<'_>, id: String, input: SiteInput) -> AppResult<SiteView> {
    blocking(&core, move |c| {
        let before = c.sites.get(&id)?;
        let view = c.sites.update(&id, &input)?;
        let moved = before.upstream_port != view.site.upstream_port || before.project_root != view.site.project_root;
        c.sync_app_process(&id, moved);
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn site_set_enabled(core: CoreState<'_>, id: String, enabled: bool) -> AppResult<SiteView> {
    blocking(&core, move |c| {
        if !enabled {
            c.app_processes.stop(&id)?;
        }
        let view = c.sites.set_enabled(&id, enabled)?;
        c.sync_app_process(&id, false);
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn site_duplicate(core: CoreState<'_>, id: String) -> AppResult<SiteView> {
    blocking(&core, move |c| c.sites.duplicate(&id)).await
}

/// Removes the registration and its server configuration; never project files.
#[tauri::command]
pub async fn site_delete(core: CoreState<'_>, id: String) -> AppResult<()> {
    blocking(&core, move |c| {
        c.app_processes.stop(&id)?;
        c.tailnet_close(&id)?;
        c.sites.delete(&id)?;
        c.thumbnails.remove(&id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn site_diagnose(core: CoreState<'_>, id: String) -> AppResult<Vec<Check>> {
    blocking(&core, move |c| c.sites.diagnose(&id)).await
}

#[tauri::command]
pub async fn site_log_sources(core: CoreState<'_>, id: String) -> AppResult<Vec<LogSource>> {
    blocking(&core, move |c| c.sites.log_sources(&id)).await
}

#[tauri::command]
pub async fn runtime_log_groups(core: CoreState<'_>) -> AppResult<Vec<RuntimeLogGroup>> {
    blocking(&core, |c| c.runtimes.log_groups()).await
}

#[tauri::command]
pub async fn runtime_log_read(core: CoreState<'_>, id: String, log_id: String, lines: Option<usize>) -> AppResult<LogTail> {
    blocking(&core, move |c| c.runtimes.read_log(&id, &log_id, lines.unwrap_or(300))).await
}

#[tauri::command]
pub async fn site_log_read(core: CoreState<'_>, id: String, kind: String, lines: Option<usize>) -> AppResult<LogTail> {
    blocking(&core, move |c| {
        if kind == "process" {
            // A Docker app's output is the container's log.
            if let Err(e) = c.refresh_docker_log(&id) {
                tracing::debug!(error = %e, "container log not read");
            }
        }
        c.sites.read_log(&id, &kind, lines.unwrap_or(300))
    })
    .await
}

// ----- Docker apps ----------------------------------------------------------------

/// Containers on this computer (running first), for adding one as an app.
#[tauri::command]
pub async fn docker_containers(core: CoreState<'_>) -> AppResult<Vec<DockerContainer>> {
    blocking(&core, |c| c.docker_containers()).await
}

/// Starts Docker Desktop (when it is not running) and waits for it.
#[tauri::command]
pub async fn docker_engine_start(core: CoreState<'_>) -> AppResult<bool> {
    blocking(&core, |_| zenv_core::docker_apps::ensure_engine()).await
}

/// Adds an app served from a container (a reverse proxy to its published port).
#[tauri::command]
pub async fn docker_app_create(core: CoreState<'_>, input: SiteInput, binding: DockerBinding) -> AppResult<SiteView> {
    blocking(&core, move |c| {
        let view = c.create_docker_app(&input, &binding)?;
        c.sync_app_process(&view.site.id, false);
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn sites_reapply(core: CoreState<'_>, service_id: String) -> AppResult<ApplyReport> {
    blocking(&core, move |c| c.sites.reapply(&service_id)).await
}

#[tauri::command]
pub async fn open_site_url(app: AppHandle, core: CoreState<'_>, id: String) -> AppResult<()> {
    let url = blocking(&core, move |c| Ok(c.sites.view(&id)?.url)).await?;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Platform(format!("cannot open {url}: {e}")))
}

// ----- hosts file --------------------------------------------------------------

#[tauri::command]
pub async fn hosts_status(core: CoreState<'_>) -> AppResult<HostsStatus> {
    blocking(&core, |c| c.sites.hosts_status()).await
}

/// Writes Zenv's hosts block. Only ever called from an explicit user action;
/// Windows shows a UAC prompt when Zenv is not elevated.
#[tauri::command]
pub async fn hosts_apply(core: CoreState<'_>) -> AppResult<ApplyOutcome> {
    blocking(&core, |c| c.sites.hosts_apply()).await
}

// ----- HTTPS ------------------------------------------------------------------

#[tauri::command]
pub async fn tls_status(core: CoreState<'_>) -> AppResult<CaStatus> {
    blocking(&core, |c| c.tls.status()).await
}

/// Creates Zenv's local certificate authority. It is not trusted until the
/// user asks for that separately.
#[tauri::command]
pub async fn tls_create_ca(core: CoreState<'_>) -> AppResult<CaStatus> {
    blocking(&core, |c| c.tls.create_ca()).await
}

/// Adds the CA to the current user's trusted roots; Windows asks the user to
/// confirm. Only ever called from an explicit user action.
#[tauri::command]
pub async fn tls_trust(core: CoreState<'_>) -> AppResult<CaStatus> {
    blocking(&core, |c| c.tls.trust()).await
}

#[tauri::command]
pub async fn tls_untrust(core: CoreState<'_>) -> AppResult<CaStatus> {
    blocking(&core, |c| c.tls.untrust()).await
}

// ----- app previews ------------------------------------------------------------

#[tauri::command]
pub async fn site_thumbnail(core: CoreState<'_>, id: String) -> AppResult<Option<Thumbnail>> {
    blocking(&core, move |c| c.thumbnails.get(&id)).await
}

/// Renders the app in a headless browser and stores the screenshot.
#[tauri::command]
pub async fn site_thumbnail_capture(core: CoreState<'_>, id: String) -> AppResult<Thumbnail> {
    blocking(&core, move |c| c.capture_thumbnail(&id)).await
}

/// Logo of a language or server, from its own installation (data URL).
#[tauri::command]
pub async fn component_icon(core: CoreState<'_>, component: ComponentId) -> AppResult<Option<String>> {
    blocking(&core, move |c| zenv_core::icons::component_icon(&c.db, &c.paths, component)).await
}

// ----- AI routers (OmniRoute, 9router) ---------------------------------------------

fn router(component: ComponentId) -> AppResult<&'static zenv_core::ai_router::RouterSpec> {
    zenv_core::ai_router::spec(component).ok_or_else(|| AppError::Validation(format!("{component} is not an AI router")))
}

#[tauri::command]
pub async fn router_status(core: CoreState<'_>, component: ComponentId) -> AppResult<RouterStatus> {
    blocking(&core, move |c| c.router_status(component)).await
}

/// Checks for a newer release and moves the router's services to it: the
/// newest one when `now`, else one that is at least a week old.
#[tauri::command]
pub async fn router_update(core: CoreState<'_>, component: ComponentId, now: bool) -> AppResult<RouterStatus> {
    blocking(&core, move |c| c.router_update(component, now)).await
}

#[tauri::command]
pub async fn router_set_auto_update(core: CoreState<'_>, component: ComponentId, on: bool) -> AppResult<RouterStatus> {
    blocking(&core, move |c| c.router_set_auto_update(component, on)).await
}

/// Opens a router's dashboard (or one of its pages) in a Zenv window.
#[tauri::command]
pub async fn router_open(app: AppHandle, core: CoreState<'_>, service_id: String, page: Option<String>) -> AppResult<()> {
    let (spec, url) = blocking(&core, move |c| Ok((c.router_spec_of_service(&service_id)?, c.router_url(&service_id, page.as_deref())?))).await?;
    crate::router_window::show(&app, spec, &url)
}

/// Whether Codex (~/.codex/config.toml) knows the router, and uses it by default.
#[tauri::command]
pub async fn codex_status(core: CoreState<'_>, component: ComponentId) -> AppResult<CodexStatus> {
    let spec = router(component)?;
    blocking(&core, move |_| zenv_core::codex::status(&spec.codex)).await
}

/// Adds the router (this service's address) to Codex as a provider and profile.
#[tauri::command]
pub async fn codex_setup(core: CoreState<'_>, service_id: String, make_default: bool) -> AppResult<CodexStatus> {
    blocking(&core, move |c| {
        let spec = c.router_spec_of_service(&service_id)?;
        zenv_core::codex::setup(&spec.codex, &format!("{}/v1", c.router_origin(&service_id)?), make_default)
    })
    .await
}

/// The config.toml text for the router, to paste by hand.
#[tauri::command]
pub async fn codex_snippet(core: CoreState<'_>, service_id: String) -> AppResult<String> {
    blocking(&core, move |c| {
        let spec = c.router_spec_of_service(&service_id)?;
        Ok(zenv_core::codex::snippet(&spec.codex, &format!("{}/v1", c.router_origin(&service_id)?)))
    })
    .await
}

/// The models and combos a running router offers (for choosing Codex's model).
#[tauri::command]
pub async fn router_models(core: CoreState<'_>, service_id: String) -> AppResult<Vec<RouterModel>> {
    blocking(&core, move |c| c.router_models(&service_id)).await
}

/// Points Codex's profile for the router at a model (and, when the router is
/// the default, the context limits).
#[tauri::command]
pub async fn codex_set_model(
    core: CoreState<'_>,
    component: ComponentId,
    model: String,
    context_window: Option<u64>,
    max_output: Option<u64>,
) -> AppResult<CodexStatus> {
    let spec = router(component)?;
    blocking(&core, move |_| zenv_core::codex::set_model(&spec.codex, &model, context_window, max_output)).await
}

#[tauri::command]
pub async fn codex_set_default(core: CoreState<'_>, component: ComponentId, on: bool) -> AppResult<CodexStatus> {
    let spec = router(component)?;
    blocking(&core, move |_| zenv_core::codex::use_by_default(&spec.codex, on)).await
}

// ----- PATH ------------------------------------------------------------------------

#[tauri::command]
pub async fn path_status(core: CoreState<'_>, component: ComponentId) -> AppResult<PathStatus> {
    blocking(&core, move |c| c.path_status(component)).await
}

/// Puts one version on the user PATH (refused when another folder holds the same programs).
#[tauri::command]
pub async fn path_add(core: CoreState<'_>, installation_id: String) -> AppResult<PathStatus> {
    blocking(&core, move |c| c.path_add(&installation_id)).await
}

#[tauri::command]
pub async fn path_remove(core: CoreState<'_>, component: ComponentId) -> AppResult<PathStatus> {
    blocking(&core, move |c| c.path_remove(component)).await
}

// ----- Tailscale -----------------------------------------------------------------

#[tauri::command]
pub async fn tailnet_status(core: CoreState<'_>) -> AppResult<TailnetStatus> {
    blocking(&core, |c| Ok(c.tailnet_status())).await
}

#[tauri::command]
pub async fn tailnet_apps(core: CoreState<'_>) -> AppResult<Vec<TailnetApp>> {
    blocking(&core, |c| c.tailnet_apps()).await
}

/// Opens an app to the devices on the user's Tailscale network.
#[tauri::command]
pub async fn tailnet_open(core: CoreState<'_>, site_id: String, service: Option<String>) -> AppResult<TailnetApp> {
    blocking(&core, move |c| c.tailnet_open(&site_id, service.as_deref())).await
}

/// Opens an app's tailnet address in the browser.
#[tauri::command]
pub async fn tailnet_open_url(app: AppHandle, core: CoreState<'_>, site_id: String) -> AppResult<()> {
    let url = blocking(&core, move |c| {
        c.tailnet_apps()?
            .into_iter()
            .find(|a| a.site_id == site_id)
            .and_then(|a| a.url)
            .ok_or_else(|| AppError::Conflict("this app is not open on the tailnet".into()))
    })
    .await?;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Platform(format!("cannot open {url}: {e}")))
}

#[tauri::command]
pub async fn tailnet_close(core: CoreState<'_>, site_id: String) -> AppResult<()> {
    blocking(&core, move |c| c.tailnet_close(&site_id)).await
}

// ----- services for apps ---------------------------------------------------------

// ----- programs behind proxy apps --------------------------------------------------

#[tauri::command]
pub async fn app_process_status(core: CoreState<'_>, site_id: String) -> AppResult<AppProcessStatus> {
    blocking(&core, move |c| c.app_process_status(&site_id)).await
}

#[tauri::command]
pub async fn app_process_start(core: CoreState<'_>, site_id: String) -> AppResult<AppProcessStatus> {
    blocking(&core, move |c| {
        c.start_app_process_by_user(&site_id)?;
        c.app_process_status(&site_id)
    })
    .await
}

#[tauri::command]
pub async fn app_process_stop(core: CoreState<'_>, site_id: String) -> AppResult<AppProcessStatus> {
    blocking(&core, move |c| c.stop_app_process(&site_id)).await
}

/// The best free port for a kind of project: the nearest free one from its usual port.
#[tauri::command]
pub async fn app_port_suggest(
    core: CoreState<'_>,
    project_type: Option<String>,
    exclude_site_id: Option<String>,
) -> AppResult<u16> {
    blocking(&core, move |c| {
        let from = zenv_core::app_process::preferred_port(project_type.as_deref().unwrap_or_default());
        zenv_core::app_process::free_port(c, from, exclude_site_id.as_deref())
    })
    .await
}

/// Saves an app's start command (empty: use the detected one).
#[tauri::command]
pub async fn app_command_set(core: CoreState<'_>, site_id: String, command: Option<String>, autostart: bool) -> AppResult<AppProcessStatus> {
    blocking(&core, move |c| c.set_app_command(&site_id, command.as_deref(), autostart)).await
}

// ----- tray panel ----------------------------------------------------------------

/// From the tray panel: show the main window.
#[tauri::command]
pub fn panel_open_main(app: AppHandle) {
    crate::tray::show_main(&app);
}

/// The panel's content height (logical px), so the window fits it.
#[tauri::command]
pub fn panel_resize(app: AppHandle, height: f64) {
    crate::tray::resize_panel(&app, height);
}

#[tauri::command]
pub fn panel_hide(app: AppHandle) {
    crate::tray::hide_panel(&app);
}

/// Quits Zenv; running services are stopped on the way out.
#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}

/// Stops every running service (web servers first).
#[tauri::command]
pub async fn stop_all_services(core: CoreState<'_>) -> AppResult<Vec<StartOutcome>> {
    blocking(&core, |c| c.stop_all_services()).await
}

#[tauri::command]
pub async fn services_for_apps(core: CoreState<'_>) -> AppResult<Vec<NeededService>> {
    blocking(&core, |c| c.services_for_apps()).await
}

/// Starts every service the enabled apps need (databases first, then web servers).
#[tauri::command]
pub async fn start_services_for_apps(core: CoreState<'_>) -> AppResult<Vec<StartOutcome>> {
    blocking(&core, |c| c.start_services_for_apps()).await
}

// ----- Studio (coding agents in an app's folder) -----------------------------

#[tauri::command]
pub async fn studio_open(app: AppHandle, core: CoreState<'_>, site_id: String) -> AppResult<()> {
    let id = site_id.clone();
    let site = blocking(&core, move |c| c.sites.get(&id)).await?;
    crate::studio_window::show(&app, &site.id, &site.domain)
}

/// Claude Code, Codex and a shell, and which are installed.
#[tauri::command]
pub async fn studio_agents(core: CoreState<'_>, site_id: String) -> AppResult<Vec<AgentInfo>> {
    blocking(&core, move |c| c.studio_agents(&site_id)).await
}

/// Starts `agent` in a terminal in the app's project folder. Output arrives
/// on `events` as raw bytes; the end as `{"exit": code}`.
#[tauri::command]
pub async fn terminal_open(
    window: tauri::Window,
    core: CoreState<'_>,
    site_id: String,
    agent: Agent,
    screen: Option<String>,
    cols: u16,
    rows: u16,
    events: Channel<InvokeResponseBody>,
) -> AppResult<String> {
    let owner = studio_owner(&window)?;
    let sink: EventSink = std::sync::Arc::new(move |event| {
        let _ = match event {
            TerminalEvent::Output(bytes) => events.send(InvokeResponseBody::Raw(bytes)),
            TerminalEvent::Exit(code) => {
                let code = code.map_or_else(|| "null".to_string(), |c| c.to_string());
                events.send(InvokeResponseBody::Json(format!(r#"{{"exit":{code}}}"#)))
            },
        };
    });
    blocking(&core, move |c| c.studio_open_terminal(&owner, &site_id, agent, screen.as_deref(), (cols, rows), sink)).await
}

#[tauri::command]
pub async fn terminal_write(window: tauri::Window, core: CoreState<'_>, id: String, data: String) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.studio.write(&owner, &id, data.as_bytes())).await
}

#[tauri::command]
pub async fn terminal_resize(window: tauri::Window, core: CoreState<'_>, id: String, cols: u16, rows: u16) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.studio.resize(&owner, &id, cols, rows)).await
}

#[tauri::command]
pub async fn terminal_close(window: tauri::Window, core: CoreState<'_>, id: String) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.studio.close(&owner, &id)).await
}

/// Starts Claude Code for a chat (`resume`: an earlier session id) with the
/// chosen model, effort and permission mode. Its messages arrive on `events`
/// as JSON lines; the end as `{"type":"zenv_exit"}`.
#[tauri::command]
pub async fn chat_open(
    window: tauri::Window,
    core: CoreState<'_>,
    site_id: String,
    resume: Option<String>,
    options: ChatOptions,
    events: Channel<InvokeResponseBody>,
) -> AppResult<String> {
    let owner = studio_owner(&window)?;
    let sink: ChatSink = std::sync::Arc::new(move |line| {
        let _ = events.send(InvokeResponseBody::Json(line));
    });
    blocking(&core, move |c| c.studio_open_chat(&owner, &site_id, resume.as_deref(), &options, sink)).await
}

/// A prompt, with files pasted or dropped into it.
#[tauri::command]
pub async fn chat_send(window: tauri::Window, core: CoreState<'_>, id: String, text: String, attachments: Vec<Attachment>) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.studio_chat_send(&owner, &id, &text, &attachments)).await
}

/// Allows or denies the tool use Claude Code asked permission for; `always`
/// takes its suggested rule or mode too.
#[tauri::command]
pub async fn chat_answer(window: tauri::Window, core: CoreState<'_>, id: String, request_id: String, allow: bool, always: bool) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.chats.answer(&owner, &id, &request_id, allow, always)).await
}

/// MCP status, or switching a server off/on or reconnecting it; returns the
/// request id its answer carries on the chat's lines.
#[tauri::command]
pub async fn chat_control(window: tauri::Window, core: CoreState<'_>, id: String, control: ChatControl) -> AppResult<String> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.chats.control(&owner, &id, &control)).await
}

/// Claude Code's documentation (a fixed address).
#[tauri::command]
pub async fn open_claude_docs(app: AppHandle) -> AppResult<()> {
    app.opener()
        .open_url("https://code.claude.com/docs", None::<&str>)
        .map_err(|e| AppError::Platform(format!("cannot open the docs: {e}")))
}

#[tauri::command]
pub async fn chat_interrupt(window: tauri::Window, core: CoreState<'_>, id: String) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.chats.interrupt(&owner, &id)).await
}

#[tauri::command]
pub async fn chat_close(window: tauri::Window, core: CoreState<'_>, id: String) -> AppResult<()> {
    let owner = studio_owner(&window)?;
    blocking(&core, move |c| c.chats.close(&owner, &id)).await
}

/// The models and slash commands Claude Code offers (asked once, no tokens used).
#[tauri::command]
pub async fn chat_catalog(window: tauri::Window, core: CoreState<'_>, site_id: String) -> AppResult<Catalog> {
    studio_owner(&window)?;
    blocking(&core, move |c| c.studio_chat_catalog(&site_id)).await
}

/// Claude Code's saved sessions for the app's project folder, newest first.
#[tauri::command]
pub async fn chat_sessions(window: tauri::Window, core: CoreState<'_>, site_id: String) -> AppResult<Vec<SessionSummary>> {
    studio_owner(&window)?;
    blocking(&core, move |c| Ok(studio_history::sessions(&c.project_root(&site_id)?))).await
}

/// A saved session's messages, as the chat's lines.
#[tauri::command]
pub async fn chat_history(window: tauri::Window, core: CoreState<'_>, site_id: String, session_id: String) -> AppResult<Vec<String>> {
    studio_owner(&window)?;
    blocking(&core, move |c| studio_history::transcript(&c.project_root(&site_id)?, &session_id)).await
}

/// Terminals belong to the Studio window that opened them.
fn studio_owner(window: &tauri::Window) -> AppResult<String> {
    let label = window.label();
    if !crate::studio_window::is_studio(label) {
        return Err(AppError::Validation("terminals are only available in a Studio window".into()));
    }
    Ok(label.to_string())
}

// ----- Editors (VS Code, Cursor, PhpStorm, ...) --------------------------------

#[tauri::command]
pub async fn editors_list(core: CoreState<'_>) -> AppResult<Editors> {
    blocking(&core, |c| c.editors()).await
}

/// Opens the app's folder in `editor` (None: the preferred one).
#[tauri::command]
pub async fn site_open_in_editor(core: CoreState<'_>, site_id: String, editor: Option<EditorId>) -> AppResult<EditorId> {
    blocking(&core, move |c| c.open_in_editor(&site_id, editor)).await
}

// ----- Project files (Studio's editor) ----------------------------------------
// Paths are relative to the app's project root and checked to stay inside it.

#[tauri::command]
pub async fn project_list(core: CoreState<'_>, site_id: String, dir: String) -> AppResult<Listing> {
    blocking(&core, move |c| project_files::list(&c.project_root(&site_id)?, &dir)).await
}

#[tauri::command]
pub async fn project_read(core: CoreState<'_>, site_id: String, path: String) -> AppResult<FileContent> {
    blocking(&core, move |c| project_files::read(&c.project_root(&site_id)?, &path)).await
}

/// Saves a file; `expected` (its time when opened) guards against outside edits.
#[tauri::command]
pub async fn project_write(core: CoreState<'_>, site_id: String, path: String, content: String, expected: Option<u64>) -> AppResult<u64> {
    blocking(&core, move |c| project_files::write(&c.project_root(&site_id)?, &path, &content, expected)).await
}

#[tauri::command]
pub async fn project_create(core: CoreState<'_>, site_id: String, path: String, kind: NewKind) -> AppResult<Entry> {
    blocking(&core, move |c| project_files::create(&c.project_root(&site_id)?, &path, kind)).await
}

#[tauri::command]
pub async fn project_stat(core: CoreState<'_>, site_id: String, paths: Vec<String>) -> AppResult<Vec<Option<u64>>> {
    blocking(&core, move |c| Ok(project_files::stat(&c.project_root(&site_id)?, &paths))).await
}

#[tauri::command]
pub async fn project_search(core: CoreState<'_>, site_id: String, query: String) -> AppResult<Vec<Entry>> {
    blocking(&core, move |c| project_files::search(&c.project_root(&site_id)?, &query)).await
}

/// Go, Rust and Python text formatted by gofmt, rustfmt or ruff/black.
#[tauri::command]
pub async fn project_format(core: CoreState<'_>, site_id: String, path: String, content: String) -> AppResult<String> {
    blocking(&core, move |c| c.format_code(&site_id, &path, &content)).await
}

/// Installs a missing formatter ("ruff" or "rustfmt") for Studio's editor.
#[tauri::command]
pub async fn formatter_install(core: CoreState<'_>, site_id: String, tool: String) -> AppResult<String> {
    blocking(&core, move |c| c.install_formatter(&site_id, &tool)).await
}
