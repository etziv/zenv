//! Local AI routers as Zenv services: one endpoint for many AI providers, plus
//! a dashboard. Two are supported, described by a [`RouterSpec`] each:
//!
//! - OmniRoute (github.com/cheaper-inference/OmniRoute, npm `omniroute`)
//! - 9router (npm `9router`)
//!
//! For both:
//! - Installed per version under `bin\<component>\<version>` from the npm
//!   registry with the user's Node.js (see `runtime_installer::npm_tool`).
//! - Run with that Node.js, bound to the service's address (127.0.0.1 by
//!   default: both listen on every network otherwise), their data in the
//!   service's data folder (`data\omniroute`, `data\9router`).
//! - Auto-update (on by default): a newer release, once it is a week old, is
//!   installed next to the current one, the services move to it (restarting
//!   if they ran) and the old version is removed; if the new one does not
//!   start, the service goes back. The user may install a newer one earlier.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::app_core::Core;
use crate::config_generator::safe_value;
use crate::error::{AppError, AppResult, IoContext};
use crate::paths::to_config_path;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, InstallSource, Installation};
use crate::service_manager::{connect_address, ServiceInput};
use crate::util::now_rfc3339;
use crate::web_server::{Control, DataDirInfo, DataDirState, LogFile, ServerContext, ServiceAdapter, SiteModel, ValidationReport};

/// Next to the installed package: the node.exe it was installed with.
pub const NODE_FILE: &str = "zenv-node.txt";

/// What differs between the routers.
pub struct RouterSpec {
    pub component: ComponentId,
    pub name: &'static str,
    /// npm package name.
    pub package: &'static str,
    /// The file Node.js runs, relative to the version folder.
    pub entry: &'static str,
    /// Node.js options before `entry`.
    pub node_args: &'static [&'static str],
    /// Arguments after `entry`.
    pub args: &'static [&'static str],
    pub default_port: u16,
    pub data_dir_name: &'static str,
    /// Environment variables that take the listen address.
    pub host_env: &'static [&'static str],
    /// Environment variables that take the data folder.
    pub data_env: &'static [&'static str],
    /// The package's own `node_modules` goes on NODE_PATH (9router loads
    /// bundled modules by name).
    pub node_path: bool,
    /// Answers 200 when the router is up.
    pub health_path: &'static str,
    /// Dashboard pages Zenv links to, as (id, label, path); the first one is
    /// the dashboard's home.
    pub pages: &'static [(&'static str, &'static str, &'static str)],
    pub node_requirement: &'static str,
    pub node_supported: fn(&str) -> bool,
    /// Packages whose install scripts must run (npm 11 asks for them in the
    /// project's .npmrc). Empty: install scripts are skipped altogether.
    pub allow_scripts: &'static str,
    pub install_timeout: Duration,
    pub ready_timeout: Duration,
    pub codex: crate::codex::CodexTarget,
}

fn version_parts(version: &str) -> Vec<u32> {
    version.trim_start_matches('v').split('.').filter_map(|p| p.parse().ok()).collect()
}

/// OmniRoute: >=22.22.2 <23 || >=24 <27.
fn omniroute_node(version: &str) -> bool {
    match version_parts(version).as_slice() {
        [22, minor, patch, ..] => (*minor, *patch) >= (22, 2),
        [major, ..] => (24..27).contains(major),
        _ => false,
    }
}

/// 9router: 22+ (its SQLite driver is Node's built-in `node:sqlite`).
fn ninerouter_node(version: &str) -> bool {
    matches!(version_parts(version).as_slice(), [major, ..] if *major >= 22)
}

pub static OMNIROUTE: RouterSpec = RouterSpec {
    component: ComponentId::Omniroute,
    name: "OmniRoute",
    package: "omniroute",
    entry: "node_modules/omniroute/bin/omniroute.mjs",
    node_args: &[],
    args: &["serve", "--no-open", "--no-tray"],
    default_port: 20128,
    data_dir_name: "omniroute",
    host_env: &["OMNIROUTE_SERVER_HOST"],
    data_env: &["DATA_DIR", "OMNIROUTE_DATA_DIR"],
    node_path: false,
    health_path: "/api/monitoring/health",
    pages: &[
        ("home", "Dashboard", "/dashboard"),
        ("providers", "Providers", "/dashboard/providers"),
        ("combos", "Combos", "/dashboard/combos"),
        ("api-manager", "API keys", "/dashboard/api-manager"),
        ("api-endpoints", "Endpoints", "/dashboard/api-endpoints"),
        ("usage", "Usage", "/dashboard/usage"),
        ("quota", "Quota", "/dashboard/quota"),
        ("costs", "Costs", "/dashboard/costs"),
        ("logs", "Logs", "/dashboard/logs"),
        ("health", "Health", "/dashboard/health"),
    ],
    node_requirement: "Node.js 22.22+ or 24+",
    node_supported: omniroute_node,
    // About 1,200 packages, roughly 2.5 GB installed.
    allow_scripts: "omniroute,keytar,onnxruntime-node,@parcel/watcher,@swc/core,protobufjs,esbuild",
    install_timeout: Duration::from_secs(30 * 60),
    // The first start builds caches.
    ready_timeout: Duration::from_secs(180),
    codex: crate::codex::CodexTarget { id: "omniroute", label: "OmniRoute (Zenv)", env_key: None, model: Some("auto") },
};

pub static NINEROUTER: RouterSpec = RouterSpec {
    component: ComponentId::Ninerouter,
    name: "9router",
    package: "9router",
    // The server itself; the package's `9router` command is an interactive
    // launcher (menus, tray, self-update) Zenv does not need.
    entry: "node_modules/9router/app/custom-server.js",
    node_args: &["--dns-result-order=ipv4first", "--max-old-space-size=6144"],
    args: &[],
    default_port: 20128,
    data_dir_name: "9router",
    host_env: &["HOSTNAME"],
    data_env: &["DATA_DIR"],
    node_path: true,
    health_path: "/api/health",
    pages: &[
        ("home", "Dashboard", "/dashboard"),
        ("providers", "Providers", "/dashboard/providers"),
        ("combos", "Combos", "/dashboard/combos"),
        ("endpoint", "Endpoint & keys", "/dashboard/endpoint"),
        ("cli-tools", "CLI tools", "/dashboard/cli-tools"),
        ("usage", "Usage", "/dashboard/usage"),
        ("quota", "Quota", "/dashboard/quota"),
        ("settings", "Settings", "/dashboard/settings"),
    ],
    node_requirement: "Node.js 22+",
    node_supported: ninerouter_node,
    // Its only script fills %APPDATA%\9router; Zenv's data folder is elsewhere.
    allow_scripts: "",
    install_timeout: Duration::from_secs(10 * 60),
    ready_timeout: Duration::from_secs(60),
    codex: crate::codex::CodexTarget { id: "ninerouter", label: "9router (Zenv)", env_key: Some("NINEROUTER_API_KEY"), model: None },
};

pub fn spec(component: ComponentId) -> Option<&'static RouterSpec> {
    match component {
        ComponentId::Omniroute => Some(&OMNIROUTE),
        ComponentId::Ninerouter => Some(&NINEROUTER),
        _ => None,
    }
}

fn spec_of(component: ComponentId) -> AppResult<&'static RouterSpec> {
    spec(component).ok_or_else(|| AppError::Validation(format!("{component} is not an AI router")))
}

fn node_version(exe: &Path) -> Option<String> {
    let out = crate::runtime_manager::probe::run_capture(exe, &["--version"], Duration::from_secs(10)).ok()?;
    out.text.lines().next().map(|l| l.trim().trim_start_matches('v').to_string()).filter(|v| !v.is_empty())
}

/// The newest node.exe `supported` accepts: Zenv's Node.js versions in
/// `bin\node`, then the one on PATH, then the usual install folder.
pub fn find_node(bin_dir: &Path, supported: fn(&str) -> bool) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(bin_dir.join("node"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("node.exe"))
        .collect();
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join("node.exe")));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join("nodejs").join("node.exe"));
    }
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for exe in candidates.into_iter().filter(|p| p.is_file()) {
        let Some(v) = node_version(&exe).filter(|v| supported(v)) else { continue };
        let key = version_parts(&v);
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, exe));
        }
    }
    best.map(|(_, exe)| exe)
}

/// The node.exe an installation runs with (recorded at install time; found
/// again when that one is gone).
pub fn node_for(spec: &RouterSpec, install: &Installation) -> AppResult<PathBuf> {
    if let Ok(text) = std::fs::read_to_string(install.root_path.join(NODE_FILE)) {
        let p = PathBuf::from(text.trim());
        if p.is_file() {
            return Ok(p);
        }
    }
    let bin = install.root_path.parent().and_then(Path::parent).unwrap_or(&install.root_path);
    find_node(bin, spec.node_supported)
        .ok_or_else(|| AppError::Conflict(format!("{} needs {}; install it in Languages first", spec.name, spec.node_requirement)))
}

/// Writes which node.exe an installation runs with.
pub fn record_node(target: &Path, node: &Path) -> AppResult<()> {
    std::fs::write(target.join(NODE_FILE), node.display().to_string()).ctx(|| format!("writing {}", target.join(NODE_FILE).display()))
}

pub struct RouterAdapter(pub &'static RouterSpec);

pub static OMNIROUTE_ADAPTER: RouterAdapter = RouterAdapter(&OMNIROUTE);
pub static NINEROUTER_ADAPTER: RouterAdapter = RouterAdapter(&NINEROUTER);

impl ServiceAdapter for RouterAdapter {
    fn component(&self) -> ComponentId {
        self.0.component
    }

    fn supports_sites(&self) -> bool {
        false
    }

    fn needs_data_dir(&self) -> bool {
        true
    }

    fn default_port(&self) -> u16 {
        self.0.default_port
    }

    fn default_data_dir_name(&self, _service_id: &str) -> String {
        self.0.data_dir_name.into()
    }

    fn inspect_data_dir(&self, _install: &Installation, dir: &Path) -> DataDirInfo {
        let name = self.0.name;
        match std::fs::read_dir(dir).map(|mut entries| entries.next().is_none()) {
            Err(_) => DataDirInfo::new(DataDirState::Missing, format!("created when {name} first starts")),
            Ok(true) => DataDirInfo::new(DataDirState::Empty, format!("filled when {name} first starts")),
            Ok(false) => DataDirInfo::new(DataDirState::Initialized, format!("{name}'s settings, keys and history")),
        }
    }

    fn main_config_name(&self) -> &'static str {
        match self.0.component {
            ComponentId::Ninerouter => "9router.env",
            _ => "omniroute.env",
        }
    }

    /// Not read by the router: the same settings go to the process environment.
    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        let mut out = format!(
            "# Generated by Zenv for {} {} (service \"{}\"). For reference only:\n# Zenv passes these settings to the process.\n",
            self.0.name,
            install.version,
            safe_value("service name", &ctx.service_name)?
        );
        for (k, v) in self.launch_env(install, ctx)? {
            if k != "PATH" {
                out.push_str(&format!("{}={}\n", k.to_string_lossy(), v.to_string_lossy()));
            }
        }
        Ok(out)
    }

    fn render_site(&self, _: &Installation, _: &ServerContext, _: &SiteModel) -> AppResult<String> {
        Err(AppError::Unsupported(format!("{} does not serve websites", self.0.name)))
    }

    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        let node = node_for(self.0, install);
        Ok(ValidationReport {
            ok: node.is_ok() && install.executable_path.is_file(),
            output: match node {
                Ok(n) => format!("runs with {}", n.display()),
                Err(e) => e.to_string(),
            },
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_program(&self, install: &Installation) -> AppResult<PathBuf> {
        node_for(self.0, install)
    }

    fn launch_args(&self, install: &Installation, _config: &Path) -> Vec<OsString> {
        let mut args: Vec<OsString> = self.0.node_args.iter().map(OsString::from).collect();
        args.push(install.executable_path.clone().into());
        args.extend(self.0.args.iter().map(OsString::from));
        args
    }

    fn launch_env(&self, install: &Installation, ctx: &ServerContext) -> AppResult<Vec<(OsString, OsString)>> {
        let data = ctx.data_dir.as_deref().ok_or_else(|| AppError::Validation("this service has no data folder".into()))?;
        let data = to_config_path(data);
        let node = node_for(self.0, install)?;
        let node_dir = node.parent().map(Path::to_path_buf).unwrap_or_default();
        let inherited = |name: &str| std::env::var_os(name).map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(node_dir).chain(inherited("PATH"))).unwrap_or_default();
        let mut env: Vec<(OsString, OsString)> = vec![("PORT".into(), ctx.port.to_string().into())];
        // Only this computer unless the service says otherwise: the routers'
        // own default is every network.
        env.extend(self.0.host_env.iter().map(|k| (OsString::from(k), OsString::from(ctx.listen_address.to_string()))));
        env.extend(self.0.data_env.iter().map(|k| (OsString::from(k), OsString::from(&data))));
        if self.0.node_path {
            // Runtime modules the router installs in its data folder, then the bundled ones.
            let bundled = install.executable_path.parent().map(|app| app.join("node_modules")).unwrap_or_default();
            let dirs = [PathBuf::from(&data).join("runtime").join("node_modules"), bundled];
            env.push(("NODE_PATH".into(), std::env::join_paths(dirs.into_iter().chain(inherited("NODE_PATH"))).unwrap_or_default()));
        }
        env.push(("PATH".into(), path));
        Ok(env)
    }

    fn ready_timeout(&self) -> Duration {
        self.0.ready_timeout
    }

    fn request_graceful_stop(&self, _ctl: &Control<'_>) -> AppResult<()> {
        Err(AppError::Unsupported(format!("{} is stopped by ending its process", self.0.name)))
    }

    fn request_reload(&self, _ctl: &Control<'_>) -> AppResult<()> {
        Err(AppError::Unsupported(format!("{} applies changes from its dashboard", self.0.name)))
    }

    fn log_files(&self, _ctx: &ServerContext) -> Vec<LogFile> {
        vec![]
    }

    fn probe(&self, addr: std::net::SocketAddr) -> Result<String, String> {
        use std::io::{Read, Write};
        let name = self.0.name;
        let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(10))).ok();
        write!(s, "GET {} HTTP/1.0\r\nHost: {addr}\r\n\r\n", self.0.health_path).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        let _ = s.take(8192).read_to_end(&mut buf);
        let text = String::from_utf8_lossy(&buf);
        match text.split_whitespace().nth(1) {
            Some("200") => Ok(format!("{name} is healthy")),
            Some(code) => Err(format!("{name} answered {code}")),
            None => Err("no answer".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterStatus {
    pub component: ComponentId,
    /// Newest installed version.
    pub installed: Option<String>,
    /// Newest release (known after a check).
    pub latest: Option<String>,
    /// When auto-update may install `latest` (RFC 3339); none once it may.
    pub auto_from: Option<String>,
    pub auto_update: bool,
    pub last_checked: Option<String>,
    /// Checking or installing an update right now.
    pub updating: bool,
    pub last_error: Option<String>,
    /// Dashboard pages Zenv links to.
    pub pages: Vec<RouterPage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterPage {
    pub id: &'static str,
    pub label: &'static str,
}

/// One model (or combo) a router offers on `/v1/models`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouterModel {
    pub id: String,
    /// Provider prefix or "combo".
    pub owned_by: Option<String>,
    pub combo: bool,
    pub context_window: Option<u64>,
    pub max_output: Option<u64>,
}

#[derive(serde::Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(serde::Deserialize)]
struct ModelEntry {
    id: String,
    owned_by: Option<String>,
    context_length: Option<u64>,
    /// OmniRoute's name for it; 9router says `max_completion_tokens`.
    max_output_tokens: Option<u64>,
    max_completion_tokens: Option<u64>,
}

/// The routers' `/v1/models` answer, combos first, then by name.
pub fn parse_models(json: &str) -> AppResult<Vec<RouterModel>> {
    let list: ModelList = serde_json::from_str(json).map_err(|e| AppError::Platform(format!("unexpected model list: {e}")))?;
    let mut models: Vec<RouterModel> = list
        .data
        .into_iter()
        .map(|m| RouterModel {
            combo: m.owned_by.as_deref() == Some("combo"),
            owned_by: m.owned_by,
            context_window: m.context_length.filter(|n| *n > 0),
            max_output: m.max_output_tokens.or(m.max_completion_tokens).filter(|n| *n > 0),
            id: m.id,
        })
        .collect();
    models.sort_by(|a, b| b.combo.cmp(&a.combo).then_with(|| a.id.cmp(&b.id)));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

#[derive(Default)]
struct UpdateState {
    updating: bool,
    latest: Option<String>,
    auto_from: Option<String>,
    last_error: Option<String>,
}

static STATE: LazyLock<Mutex<HashMap<ComponentId, UpdateState>>> = LazyLock::new(Default::default);

fn with_state<T>(component: ComponentId, f: impl FnOnce(&mut UpdateState) -> T) -> T {
    let mut all = STATE.lock().unwrap_or_else(|p| p.into_inner());
    f(all.entry(component).or_default())
}

/// `http://127.0.0.1:20128`, `http://[::1]:20128`: the address a client
/// connects to for a service bound to `bind`.
fn origin(bind: std::net::IpAddr, port: u16) -> String {
    format!("http://{}", std::net::SocketAddr::new(connect_address(bind), port))
}

fn version_key(v: &str) -> Vec<u32> {
    v.split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

fn auto_update_key(c: ComponentId) -> String {
    format!("{c}.auto_update")
}

fn last_check_key(c: ComponentId) -> String {
    format!("{c}.last_check")
}

impl Core {
    pub(crate) fn setting(&self, key: &str) -> AppResult<Option<String>> {
        self.db.read(|c| Ok(c.query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |r| r.get(0)).optional()?))
    }

    pub(crate) fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
                params![key, value, now_rfc3339()],
            )?;
            Ok(())
        })
    }

    fn router_installs(&self, component: ComponentId) -> AppResult<Vec<Installation>> {
        let mut list = runtime_manager::list(&self.db, Some(component))?;
        list.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
        Ok(list)
    }

    pub fn router_status(&self, component: ComponentId) -> AppResult<RouterStatus> {
        let spec = spec_of(component)?;
        let (updating, latest, auto_from, last_error) =
            with_state(component, |st| (st.updating, st.latest.clone(), st.auto_from.clone(), st.last_error.clone()));
        Ok(RouterStatus {
            component,
            installed: self.router_installs(component)?.first().map(|i| i.version.clone()),
            latest,
            auto_from,
            auto_update: self.setting(&auto_update_key(component))?.as_deref() != Some("0"),
            last_checked: self.setting(&last_check_key(component))?,
            updating,
            last_error,
            pages: spec.pages.iter().map(|(id, label, _)| RouterPage { id, label }).collect(),
        })
    }

    pub fn router_set_auto_update(&self, component: ComponentId, on: bool) -> AppResult<RouterStatus> {
        spec_of(component)?;
        self.set_setting(&auto_update_key(component), if on { "1" } else { "0" })?;
        self.router_status(component)
    }

    /// The router of an AI-router service.
    pub fn router_spec_of_service(&self, service_id: &str) -> AppResult<&'static RouterSpec> {
        let svc = self.services.get(service_id)?;
        spec(svc.component).ok_or_else(|| AppError::Validation(format!("{} is not an AI router", svc.name)))
    }

    /// The URL of a router service's dashboard page (one of its `pages`; the
    /// home page when `page` is none or unknown).
    pub fn router_url(&self, service_id: &str, page: Option<&str>) -> AppResult<String> {
        let spec = self.router_spec_of_service(service_id)?;
        let path = spec.pages.iter().find(|(id, _, _)| Some(*id) == page).unwrap_or(&spec.pages[0]).2;
        Ok(format!("{}{path}", self.router_origin(service_id)?))
    }

    /// The models and combos a running router service offers, asked with the
    /// API key Codex uses for it (when there is one).
    pub fn router_models(&self, service_id: &str) -> AppResult<Vec<RouterModel>> {
        let spec = self.router_spec_of_service(service_id)?;
        if !self.services.is_running(service_id) {
            return Err(AppError::Conflict(format!("start {} first", spec.name)));
        }
        let url = format!("{}/v1/models", self.router_origin(service_id)?);
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).build().into();
        let mut req = agent.get(&url);
        if let Some(key) = crate::codex::api_key(&spec.codex)? {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        let mut res = match req.call() {
            Ok(res) => res,
            Err(ureq::Error::StatusCode(401 | 403)) => {
                return Err(AppError::Conflict(format!(
                    "{} asks for an API key: create one in its dashboard, then put it in Codex's config (experimental_bearer_token) or in {}",
                    spec.name,
                    spec.codex.env_key.unwrap_or("the variable env_key names")
                )))
            }
            Err(e) => return Err(AppError::Platform(format!("cannot read {}'s models: {e}", spec.name))),
        };
        let text = res
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| AppError::Platform(format!("cannot read {}'s models: {e}", spec.name)))?;
        parse_models(&text)
    }

    /// `http://<address>:<port>` of a router service (`http://[::1]:20128` for IPv6).
    pub fn router_origin(&self, service_id: &str) -> AppResult<String> {
        let svc = self.services.get(service_id)?;
        let bind = crate::port_manager::parse_bind_address(&svc.listen_address)?;
        Ok(origin(bind, svc.http_port))
    }

    /// Installs a newer release and moves the router's services to it: the
    /// newest one when `now` (the user asked for it), else the newest that is
    /// at least a week old. Does nothing until the router is installed once.
    /// Returns the status afterwards.
    pub fn router_update(&self, component: ComponentId, now: bool) -> AppResult<RouterStatus> {
        let spec = spec_of(component)?;
        let busy = with_state(component, |st| {
            let busy = st.updating;
            if !busy {
                st.updating = true;
                st.last_error = None;
            }
            busy
        });
        if busy {
            return self.router_status(component);
        }
        let result = self.router_update_inner(spec, now);
        with_state(component, |st| {
            st.updating = false;
            if let Err(e) = &result {
                st.last_error = Some(e.to_string());
            }
        });
        let _ = self.set_setting(&last_check_key(component), &now_rfc3339());
        result?;
        self.router_status(component)
    }

    fn router_update_inner(&self, spec: &RouterSpec, now: bool) -> AppResult<()> {
        use crate::runtime_installer::npm_tool;
        let component = spec.component;
        let installs = self.router_installs(component)?;
        let Some(current) = installs.first().cloned() else { return Ok(()) };
        let releases = npm_tool::releases(spec)?;
        let newest = releases.first().ok_or_else(|| AppError::Platform(format!("the npm registry lists no {} release", spec.name)))?;
        let settled = npm_tool::newest_settled(&releases, time::OffsetDateTime::now_utc());
        with_state(component, |st| {
            st.latest = Some(newest.version.clone());
            st.auto_from = (settled != Some(newest))
                .then(|| (newest.published + npm_tool::SETTLE).format(&time::format_description::well_known::Rfc3339).ok())
                .flatten();
        });
        let target = if now { Some(newest) } else { settled };
        let Some(latest) = target.map(|r| r.version.clone()).filter(|v| version_key(v) > version_key(&current.version)) else {
            return Ok(());
        };
        tracing::info!(router = spec.name, from = %current.version, to = %latest, "updating");
        let new = self.installer.install_blocking(component, &latest)?;

        for svc in self.services.list()?.into_iter().filter(|s| s.component == component) {
            let old_install = svc.installation_id.clone();
            let was_running = self.services.is_running(&svc.id);
            let input = |installation_id: String| ServiceInput {
                name: svc.name.clone(),
                installation_id,
                listen_address: svc.listen_address.clone(),
                http_port: svc.http_port,
                autostart: svc.autostart,
                data_dir: svc.data_dir.as_ref().map(|d| d.display().to_string()),
                https_port: svc.https_port,
            };
            if was_running {
                self.services.stop(&svc.id)?;
            }
            self.services.update(&svc.id, &input(new.id.clone()))?;
            if was_running {
                if let Err(e) = self.start_service(&svc.id) {
                    // Back to the version that worked.
                    let _ = self.services.update(&svc.id, &input(old_install));
                    let _ = self.start_service(&svc.id);
                    return Err(AppError::Platform(format!("{} {latest} did not start ({e}); {} stays on {}", spec.name, svc.name, current.version)));
                }
            }
        }
        // Remove versions no service uses any more (Zenv's own installs only).
        let used: Vec<String> = self.services.list()?.into_iter().map(|s| s.installation_id).collect();
        for old in self.router_installs(component)?.into_iter().filter(|i| i.id != new.id && i.source == InstallSource::Managed && !used.contains(&i.id)) {
            if let Err(e) = self.installer.uninstall(&old.id) {
                tracing::warn!(router = spec.name, version = %old.version, error = %e, "old version not removed");
            }
        }
        Ok(())
    }

    /// Routers to check for updates now: installed, with auto-update on.
    pub fn routers_due_for_update(&self) -> Vec<ComponentId> {
        [ComponentId::Omniroute, ComponentId::Ninerouter]
            .into_iter()
            .filter(|c| {
                let on = self.setting(&auto_update_key(*c)).ok().flatten().as_deref() != Some("0");
                on && !self.router_installs(*c).map(|l| l.is_empty()).unwrap_or(true)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_server::tests_support::{ctx, fake_install};

    #[test]
    fn supported_node_versions() {
        for (v, ok) in [("22.22.2", true), ("22.22.1", false), ("22.23.0", true), ("23.1.0", false), ("24.22.0", true), ("26.12.0", true), ("27.0.0", false), ("20.18.0", false)] {
            assert_eq!(omniroute_node(v), ok, "omniroute {v}");
        }
        for (v, ok) in [("22.0.0", true), ("24.22.1", true), ("27.1.0", true), ("20.18.0", false), ("18.20.0", false)] {
            assert_eq!(ninerouter_node(v), ok, "9router {v}");
        }
    }

    #[test]
    fn ipv6_origins_have_brackets() {
        assert_eq!(origin("127.0.0.1".parse().unwrap(), 20128), "http://127.0.0.1:20128");
        assert_eq!(origin("::1".parse().unwrap(), 20128), "http://[::1]:20128");
        assert_eq!(origin("::".parse().unwrap(), 20128), "http://[::1]:20128");
        assert_eq!(origin("0.0.0.0".parse().unwrap(), 20128), "http://127.0.0.1:20128");
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_key("3.8.10") > version_key("3.8.9"));
        assert!(version_key("0.5.110") > version_key("0.5.104"));
    }

    #[test]
    fn model_lists_of_both_routers() {
        // Trimmed answers of OmniRoute and 9router.
        let omni = r#"{"object":"list","data":[
            {"id":"deepseek/v4","owned_by":"deepseek","context_length":1000000,"max_output_tokens":384000},
            {"id":"gpx","owned_by":"combo","context_length":null,"max_output_tokens":null},
            {"id":"auto/best-coding","owned_by":"combo","context_length":1000000,"max_output_tokens":384000}]}"#;
        let models = parse_models(omni).unwrap();
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["auto/best-coding", "gpx", "deepseek/v4"]);
        assert!(models[1].combo && models[1].context_window.is_none());
        let nine = r#"{"object":"list","data":[{"id":"main","object":"model","owned_by":"combo","capabilities":{},"context_length":200000,"max_completion_tokens":32000}]}"#;
        let m = &parse_models(nine).unwrap()[0];
        assert_eq!((m.combo, m.context_window, m.max_output), (true, Some(200_000), Some(32_000)));
        assert!(parse_models("<html>").is_err());
    }

    #[test]
    fn every_router_has_a_home_page_first() {
        for s in [&OMNIROUTE, &NINEROUTER] {
            assert_eq!(s.pages[0].0, "home", "{}", s.name);
            assert!(s.pages.iter().all(|(_, _, p)| p.starts_with("/dashboard")), "{}", s.name);
        }
    }

    #[test]
    fn ninerouter_runs_its_server_with_zenv_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bin").join("ninerouter").join("0.5.104");
        std::fs::create_dir_all(&root).unwrap();
        // A node.exe recorded at install time (any existing file will do here).
        let node = dir.path().join("node.exe");
        std::fs::write(&node, "").unwrap();
        record_node(&root, &node).unwrap();
        let install = fake_install(ComponentId::Ninerouter, "0.5.104", &root);
        let data = dir.path().join("data").join("9router");
        let adapter = RouterAdapter(&NINEROUTER);
        let env = adapter.launch_env(&install, &ctx(dir.path(), Some(data.clone()))).unwrap();
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.to_string_lossy().into_owned());
        assert_eq!(get("HOSTNAME").as_deref(), Some("127.0.0.1"));
        assert_eq!(get("DATA_DIR").map(PathBuf::from), Some(PathBuf::from(to_config_path(&data))));
        let node_path = get("NODE_PATH").unwrap();
        let first = std::env::split_paths(&node_path).next().unwrap();
        assert!(first.ends_with("runtime/node_modules") || first.ends_with("runtime\\node_modules"), "{node_path}");
        let args = adapter.launch_args(&install, Path::new("x"));
        assert_eq!(args[0], "--dns-result-order=ipv4first");
        assert_eq!(PathBuf::from(&args[2]), install.executable_path);
    }
}
