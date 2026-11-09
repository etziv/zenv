//! The program behind a proxy app (a Rust, Go or Node server on its own
//! port). Zenv starts it with the project's command when the app's web server
//! starts or on "Start apps", and stops it with "Stop all" or on exit.
//!
//! Commands are run directly (no shell), in the project folder, and only for
//! known development tools; the chosen language versions come first on PATH.
//! A command detected from the project's files never runs by itself: adding a
//! folder (say, a freshly cloned repository) must not run its code. It runs
//! once the user starts the app or saves the command, and is saved then.
//!
//! Like `env_file` in docker compose, the project's `.env` is passed to the
//! program as environment variables (the file is only read), except the port:
//! the port is the app's own in Zenv, given as `PORT` and as whatever port
//! setting the `.env` uses (`APP_PORT=8101`, `BIND=127.0.0.1:8101`, ...).
//! The address is always this computer only (`HOST=127.0.0.1`, and the host
//! part of `BIND`-like settings): the web server in front is what others
//! reach. A program that ignores these and listens on every network anyway is
//! reported (`exposed`).
//! `.env` loaders (godotenv, dotenvy, dotenv) keep variables already set, so
//! Zenv's port wins over the file's.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::local_sites::{Site, Strategy};
use crate::paths::AppPaths;
use crate::process_manager::{ProcessManager, ProcessSpec, ProcessState};
use crate::runtime_catalog::{self, Category};
use crate::runtime_manager;
use crate::util::now_rfc3339;

/// Programs a start command may run.
pub const ALLOWED: &[&str] =
    &["cargo", "go", "npm", "npx", "pnpm", "yarn", "bun", "node", "deno", "python", "py", "uv", "php", "ruby", "bundle", "rails", "dotnet"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProcessStatus {
    pub site_id: String,
    /// The command Zenv runs (saved, else detected); None when unknown.
    pub command: Option<String>,
    /// The command was detected from the project, not saved: it runs only
    /// when the user starts the app (which approves and saves it).
    pub detected: bool,
    /// What the command runs, when the project says (`vite` for `npm run dev`).
    pub runs: Option<String>,
    pub autostart: bool,
    /// "running", "starting" (process up, port not yet), "stopped" or "exited".
    pub state: &'static str,
    pub exit_code: Option<i32>,
    pub port: Option<u16>,
    /// Something answers on the app's port (Zenv's process or another one).
    pub listening: bool,
    /// How the program gets its port, e.g. ["APP_PORT=8101", "PORT=8101"].
    pub port_vars: Vec<String>,
    /// The port is open to other devices on the network (the program, or a
    /// container's published port, listens on every address).
    pub exposed: bool,
    pub log_file: PathBuf,
    /// The app runs in this container instead of a program Zenv starts.
    pub docker: Option<crate::docker_apps::DockerAppStatus>,
}

/// Settings holding just a port number.
const PORT_KEYS: &[&str] = &["PORT", "APP_PORT", "HTTP_PORT", "SERVER_PORT", "LISTEN_PORT", "WEB_PORT"];
/// Settings holding `host:port` (or `:port`).
const ADDR_KEYS: &[&str] = &["BIND", "ADDR", "LISTEN", "LISTEN_ADDR", "HTTP_ADDR", "SERVER_ADDR", "APP_ADDR", "BIND_ADDR"];
/// Settings holding just the address to listen on.
const HOST_KEYS: &[&str] = &["HOST", "APP_HOST", "SERVER_HOST", "BIND_HOST", "LISTEN_HOST", "HTTP_HOST"];
/// Where app programs listen: only this computer; the web server in front
/// connects to it there.
const APP_HOST: &str = "127.0.0.1";

/// The project's `.env` as (key, value) pairs; the file is only read.
/// Supports `export`, quotes, `#` comments and blank lines.
pub fn read_dotenv(project_root: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(project_root.join(".env")) else { return vec![] };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let value = value.trim();
        let value = if let Some(v) = value.strip_prefix('"').and_then(|v| v.rfind('"').map(|i| &v[..i])) {
            v.replace("\\n", "\n").replace("\\\"", "\"")
        } else if let Some(v) = value.strip_prefix('\'').and_then(|v| v.rfind('\'').map(|i| &v[..i])) {
            v.to_string()
        } else {
            // An unquoted value ends at " #".
            value.split(" #").next().unwrap_or("").trim().to_string()
        };
        out.push((key.to_string(), value));
    }
    out
}

/// The variables that give the program Zenv's `port` on this computer only:
/// every port and address setting the `.env` uses, with Zenv's values, plus
/// `PORT` and `HOST` (and `APP_PORT` when the `.env` names no port).
pub fn port_vars(dotenv: &[(String, String)], port: u16) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, _) in dotenv {
        let upper = k.to_ascii_uppercase();
        if PORT_KEYS.contains(&upper.as_str()) {
            out.push((k.clone(), port.to_string()));
        } else if ADDR_KEYS.contains(&upper.as_str()) {
            out.push((k.clone(), format!("{APP_HOST}:{port}")));
        } else if HOST_KEYS.contains(&upper.as_str()) {
            out.push((k.clone(), APP_HOST.into()));
        }
    }
    let has = |out: &[(String, String)], key: &str| out.iter().any(|(k, _)| k.eq_ignore_ascii_case(key));
    let named = out.iter().any(|(k, _)| !HOST_KEYS.contains(&k.to_ascii_uppercase().as_str()));
    if !has(&out, "PORT") {
        out.push(("PORT".into(), port.to_string()));
    }
    if !named && !has(&out, "APP_PORT") {
        out.push(("APP_PORT".into(), port.to_string()));
    }
    if !has(&out, "HOST") {
        out.push(("HOST".into(), APP_HOST.into()));
    }
    out
}

/// Something listens on `port` on every address (or a network one), so other
/// devices can reach it past the web server.
fn exposed(port: u16) -> bool {
    crate::platform::listening_sockets()
        .map(|all| all.iter().any(|s| s.port == port && !s.address.is_loopback()))
        .unwrap_or(false)
}

/// Splits a command line into words; double or single quotes group words.
pub fn split(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in command.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Checks a start command: a known tool by name (no paths), then arguments.
pub fn parse(command: &str) -> AppResult<(String, Vec<String>)> {
    let mut words = split(command.trim());
    if words.is_empty() {
        return Err(AppError::Validation("the start command is empty".into()));
    }
    let program = words.remove(0);
    let name = program.to_ascii_lowercase();
    let name = name.trim_end_matches(".exe").trim_end_matches(".cmd");
    if program.contains(['/', '\\', ':']) || !ALLOWED.contains(&name) {
        return Err(AppError::Validation(format!(
            "'{program}' is not a development tool Zenv runs; use one of: {}",
            ALLOWED.join(", ")
        )));
    }
    Ok((name.to_string(), words))
}

/// The usual command for a project, from its files.
pub fn detect(project_root: &Path) -> Option<String> {
    if project_root.join("Cargo.toml").is_file() {
        return Some("cargo run".into());
    }
    if project_root.join("go.mod").is_file() {
        return Some(go_command(project_root));
    }
    if let Ok(text) = std::fs::read_to_string(project_root.join("package.json")) {
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let scripts = json.get("scripts")?;
        let runner = if project_root.join("pnpm-lock.yaml").is_file() {
            "pnpm"
        } else if project_root.join("yarn.lock").is_file() {
            "yarn"
        } else if project_root.join("bun.lockb").is_file() || project_root.join("bun.lock").is_file() {
            "bun"
        } else {
            "npm"
        };
        for script in ["dev", "start"] {
            if scripts.get(script).is_some() {
                return Some(match (runner, script) {
                    ("npm", "start") => "npm start".into(),
                    ("npm", s) => format!("npm run {s}"),
                    (r, s) => format!("{r} {s}"),
                });
            }
        }
    }
    None
}

/// `go run .`, or `go run ./cmd/<name>` when the main package lives under
/// `cmd` (`cmd/server` preferred, else the only one).
fn go_command(root: &Path) -> String {
    let is_main = |dir: &Path| {
        std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
            e.path().extension().is_some_and(|x| x == "go")
                && std::fs::read_to_string(e.path()).is_ok_and(|t| t.lines().any(|l| l.trim() == "package main"))
        })
    };
    if is_main(root) {
        return "go run .".into();
    }
    let mains: Vec<String> = std::fs::read_dir(root.join("cmd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && is_main(&e.path()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    match mains.iter().find(|m| ["server", "api", "web", "app"].contains(&m.as_str())).or_else(|| (mains.len() == 1).then(|| &mains[0])) {
        Some(m) => format!("go run ./cmd/{m}"),
        None => "go run .".into(),
    }
}

/// The package.json script a runner command (`npm run dev`, `pnpm start`, ...)
/// runs, as written in the project.
pub fn script_body(project_root: &Path, command: &str) -> Option<String> {
    let (program, args) = parse(command).ok()?;
    if !["npm", "pnpm", "yarn", "bun"].contains(&program.as_str()) {
        return None;
    }
    let name = match args.first().map(String::as_str)? {
        "run" | "run-script" => args.get(1)?.as_str(),
        other => other,
    };
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(project_root.join("package.json")).ok()?).ok()?;
    json.get("scripts")?.get(name)?.as_str().map(str::to_string)
}

/// First port from `from` that no app or service in Zenv uses and nothing
/// listens on, skipping `exclude_site`'s own port.
pub fn free_port(core: &Core, from: u16, exclude_site: Option<&str>) -> AppResult<u16> {
    let mut taken = std::collections::HashSet::new();
    for s in core.sites.all_sites()? {
        if Some(s.id.as_str()) != exclude_site {
            taken.extend(s.upstream_port);
        }
    }
    for svc in core.services.list()? {
        taken.insert(svc.http_port);
        taken.extend(svc.https_port);
    }
    let localhost = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let start = from.max(1024);
    (start..=start.saturating_add(2000))
        .find(|p| {
            !taken.contains(p)
                && matches!(crate::port_manager::inspect(*p, localhost).map(|i| i.status), Ok(crate::port_manager::PortStatus::Free))
                && matches!(
                    crate::port_manager::inspect(*p, std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)).map(|i| i.status),
                    Ok(crate::port_manager::PortStatus::Free)
                )
        })
        .ok_or_else(|| AppError::Conflict(format!("no free port from {start}")))
}

/// Where Zenv starts looking for ports for new apps.
pub const APP_PORT_BASE: u16 = 8100;

/// The usual port for a kind of project (Go 8080, Rust and Python 8000,
/// Node and Ruby 3000); Zenv takes the nearest free one from there.
pub fn preferred_port(project_type: &str) -> u16 {
    match project_type {
        "go" => 8080,
        "rust" | "python" => 8000,
        "node" | "ruby" => 3000,
        _ => APP_PORT_BASE,
    }
}

pub fn log_file(paths: &AppPaths, site_id: &str) -> PathBuf {
    paths.logs_dir.join("apps").join(format!("{site_id}.log"))
}

fn key(site_id: &str) -> String {
    format!("app:{site_id}")
}

fn listening(port: u16) -> bool {
    port != 0 && TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(200)).is_ok()
}

/// The program on `path` (`npm` is `npm.cmd` on Windows).
fn resolve(program: &str, path: &OsString) -> Option<PathBuf> {
    std::env::split_paths(path).find_map(|dir| {
        ["exe", "cmd", "bat", "com"].iter().map(|ext| dir.join(format!("{program}.{ext}"))).find(|p| p.is_file())
    })
}

#[derive(Default)]
pub struct AppProcesses {
    processes: ProcessManager,
}

impl AppProcesses {
    pub fn is_running(&self, site_id: &str) -> bool {
        self.processes.is_running(&key(site_id))
    }

    pub fn stop(&self, site_id: &str) -> AppResult<bool> {
        let was = self.is_running(site_id);
        self.processes.stop(&key(site_id), None, Duration::ZERO)?;
        Ok(was)
    }

    pub fn stop_all(&self) -> usize {
        let keys = self.processes.running_keys();
        for k in &keys {
            let _ = self.processes.stop(k, None, Duration::ZERO);
        }
        keys.len()
    }
}

impl Core {
    /// Saved command and autostart for an app, if any.
    fn saved_command(&self, site_id: &str) -> AppResult<Option<(Option<String>, bool)>> {
        self.db.read(|c| {
            Ok(c.query_row("SELECT command, autostart FROM app_commands WHERE site_id = ?1", [site_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?)
        })
    }

    /// (command, detected, autostart). A detected command never starts by
    /// itself, so its autostart is off until it is saved.
    fn app_command(&self, site: &Site) -> AppResult<(Option<String>, bool, bool)> {
        let saved = self.saved_command(&site.id)?;
        let autostart = saved.as_ref().is_none_or(|(_, a)| *a);
        Ok(match saved.and_then(|(c, _)| c) {
            Some(c) => (Some(c), false, autostart),
            None => (detect(&site.project_root), true, false),
        })
    }

    fn save_command(&self, site_id: &str, command: Option<&str>, autostart: bool) -> AppResult<()> {
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO app_commands (site_id, command, autostart, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(site_id) DO UPDATE SET command = ?2, autostart = ?3, updated_at = ?4, adopt_detected = 0",
                params![site_id, command, autostart, now_rfc3339()],
            )?;
            Ok(())
        })
    }

    /// Apps added before detected commands needed approval already ran theirs
    /// with the web server; their detected command is saved once, as it is
    /// today, so they keep doing so.
    pub fn adopt_detected_commands(&self) -> AppResult<()> {
        let pending: Vec<(String, bool)> = self.db.read(|c| {
            let mut stmt = c.prepare("SELECT site_id, autostart FROM app_commands WHERE adopt_detected = 1")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;
        for (site_id, autostart) in pending {
            let command = self.sites.get(&site_id).ok().and_then(|s| detect(&s.project_root));
            self.save_command(&site_id, command.as_deref(), autostart)?;
            if let Some(c) = command {
                tracing::info!(site = %site_id, command = %c, "kept the detected start command of an existing app");
            }
        }
        Ok(())
    }

    pub fn app_process_status(&self, site_id: &str) -> AppResult<AppProcessStatus> {
        let site = self.sites.get(site_id)?;
        if let Some(binding) = self.docker_binding(site_id)? {
            return Ok(self.docker_process_status(&site, &binding));
        }
        let (command, detected, autostart) = self.app_command(&site)?;
        let runs = command.as_deref().and_then(|c| script_body(&site.project_root, c));
        let port = (site.strategy == Strategy::ReverseProxy).then_some(site.upstream_port).flatten();
        let listening = port.is_some_and(listening);
        let (state, exit_code) = match self.app_processes.processes.state(&key(site_id)) {
            ProcessState::Running { .. } if listening || port.is_none() => ("running", None),
            ProcessState::Running { .. } => ("starting", None),
            ProcessState::Exited { exit } => ("exited", exit.code),
            ProcessState::NotStarted => ("stopped", None),
        };
        let port_vars = port
            .map(|p| port_vars(&read_dotenv(&site.project_root), p).into_iter().map(|(k, v)| format!("{k}={v}")).collect())
            .unwrap_or_default();
        let exposed = listening && port.is_some_and(exposed);
        Ok(AppProcessStatus {
            site_id: site_id.to_string(),
            command,
            detected,
            runs,
            autostart,
            state,
            exit_code,
            port,
            listening,
            port_vars,
            exposed,
            log_file: log_file(&self.paths, site_id),
            docker: None,
        })
    }

    /// A Docker app's status in the same terms as a program's.
    fn docker_process_status(&self, site: &Site, binding: &crate::docker_apps::DockerBinding) -> AppProcessStatus {
        let docker = self.docker_app_status(binding);
        let port = site.upstream_port;
        let listening = port.is_some_and(listening);
        let (state, exit_code) = match docker.state.as_str() {
            "running" | "restarting" if listening => ("running", None),
            "running" | "restarting" => ("starting", None),
            "exited" | "dead" => ("exited", docker.exit_code),
            _ => ("stopped", None),
        };
        let command = match &binding.compose {
            Some(c) => format!("docker compose -p {} up -d {}", c.project, c.service),
            None => format!("docker start {}", binding.container),
        };
        AppProcessStatus {
            site_id: site.id.clone(),
            command: Some(command),
            detected: false,
            runs: None,
            autostart: binding.autostart,
            state,
            exit_code,
            port,
            listening,
            port_vars: vec![],
            exposed: listening && port.is_some_and(exposed),
            log_file: log_file(&self.paths, &site.id),
            docker: Some(docker),
        }
    }

    /// Saves an app's start command (None: use the detected one).
    pub fn set_app_command(&self, site_id: &str, command: Option<&str>, autostart: bool) -> AppResult<AppProcessStatus> {
        self.sites.get(site_id)?;
        if self.docker_binding(site_id)?.is_some() {
            // A container has no command; only "start with the web server".
            self.set_docker_autostart(site_id, autostart)?;
            return self.app_process_status(site_id);
        }
        let command = command.map(str::trim).filter(|c| !c.is_empty());
        if let Some(c) = command {
            parse(c)?;
        }
        self.save_command(site_id, command, autostart)?;
        self.app_process_status(site_id)
    }

    /// Language versions for an app's process: each language's default.
    pub(crate) fn app_env(&self) -> AppResult<Vec<(OsString, OsString)>> {
        let mut chosen = BTreeMap::new();
        for inst in runtime_manager::list(&self.db, None)? {
            if runtime_catalog::info(inst.component).category == Category::Language && inst.executable_exists {
                // Listed with the default first per component.
                chosen.entry(inst.component.as_str()).or_insert(inst);
            }
        }
        let installs: Vec<_> = chosen.into_values().collect();
        Ok(crate::runtime_env::build(&installs, std::env::var_os("PATH")))
    }

    /// Starts an app's program because the user asked: a detected command is
    /// approved by that and saved (it then also starts with the web server).
    pub fn start_app_process_by_user(&self, site_id: &str) -> AppResult<bool> {
        let site = self.sites.get(site_id)?;
        if site.strategy == Strategy::ReverseProxy && self.docker_binding(site_id)?.is_none() {
            if let (Some(command), true, _) = self.app_command(&site)? {
                let autostart = self.saved_command(site_id)?.is_none_or(|(_, a)| a);
                self.save_command(site_id, Some(&command), autostart)?;
                tracing::info!(domain = %site.domain, %command, "detected start command approved");
            }
        }
        self.start_app_process(site_id)
    }

    /// Starts an app's program unless it already runs (or its port already
    /// answers, e.g. started from a terminal). Returns false when nothing
    /// was started. A detected command must be approved first.
    pub fn start_app_process(&self, site_id: &str) -> AppResult<bool> {
        let site = self.sites.get(site_id)?;
        if site.strategy != Strategy::ReverseProxy {
            return Err(AppError::Validation(format!("{} is not served from its own program", site.domain)));
        }
        if let Some(binding) = self.docker_binding(site_id)? {
            return self.start_docker_app(&binding);
        }
        if self.app_processes.is_running(site_id) || site.upstream_port.is_some_and(listening) {
            return Ok(false);
        }
        let (command, detected, _) = self.app_command(&site)?;
        let command = command.ok_or_else(|| {
            AppError::Validation(format!("set a start command for {} (for example `cargo run` or `npm run dev`)", site.domain))
        })?;
        if detected {
            return Err(AppError::Validation(format!(
                "{} runs `{command}` only after you approve it: press Start in the app's panel or save the command",
                site.domain
            )));
        }
        let (program, args) = parse(&command)?;
        let mut env = self.app_env()?;
        let path = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.clone()).unwrap_or_default();
        // The project's .env (never PATH), then Zenv's port over it.
        let dotenv = read_dotenv(&site.project_root);
        let port = site.upstream_port.unwrap_or_default();
        for (k, v) in dotenv.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("PATH")).cloned().chain(port_vars(&dotenv, port)) {
            env.push((OsString::from(k), OsString::from(v)));
        }
        let exe = resolve(&program, &path)
            .ok_or_else(|| AppError::Validation(format!("{program} was not found; install it in Languages first")))?;
        let spec = ProcessSpec {
            program: exe,
            args: args.into_iter().map(OsString::from).collect(),
            cwd: site.project_root.clone(),
            env,
            log_file: log_file(&self.paths, site_id),
        };
        self.app_processes.processes.spawn(&key(site_id), &spec)?;
        tracing::info!(domain = %site.domain, %command, "app process started");
        Ok(true)
    }

    pub fn stop_app_process(&self, site_id: &str) -> AppResult<AppProcessStatus> {
        if let Some(binding) = self.docker_binding(site_id)? {
            self.stop_docker_app(&binding)?;
            return self.app_process_status(site_id);
        }
        self.app_processes.stop(site_id)?;
        self.app_process_status(site_id)
    }

    /// Proxy apps with autostart and a command, served by `service_id`
    /// (bound to it or to any server), or all of them.
    pub fn autostart_apps(&self, service_id: Option<&str>) -> AppResult<Vec<Site>> {
        let mut out = Vec::new();
        for site in self.sites.enabled_sites()?.into_iter().filter(|s| s.strategy == Strategy::ReverseProxy) {
            if service_id.is_some_and(|id| site.service_id.as_deref().is_some_and(|s| s != id)) {
                continue;
            }
            let wanted = match self.docker_binding(&site.id)? {
                Some(binding) => binding.autostart,
                None => {
                    let (command, _, autostart) = self.app_command(&site)?;
                    autostart && command.is_some()
                }
            };
            if wanted {
                out.push(site);
            }
        }
        Ok(out)
    }

    /// Detects a project; proxy apps get a free port of their own.
    pub fn detect_project(&self, path: &str) -> AppResult<crate::local_sites::detect::DetectedProject> {
        let mut d = self.sites.detect(path)?;
        if d.strategy == Strategy::ReverseProxy {
            d.upstream_port = Some(free_port(self, preferred_port(d.project_type.as_str()), None)?);
        }
        Ok(d)
    }

    /// After an app was added, changed or switched on or off: its program runs
    /// exactly when the app is a proxy app that is on, has autostart and a
    /// command, and a web server serving it is running (otherwise programs
    /// start with the web server). `restart` restarts a running program
    /// (its port or folder changed).
    pub fn sync_app_process(&self, site_id: &str, restart: bool) {
        let Ok(site) = self.sites.get(site_id) else { return };
        if let Ok(Some(binding)) = self.docker_binding(site_id) {
            // A container is left alone when its app is switched off: other
            // things may use it. It is started like a program otherwise.
            let serving = match &site.service_id {
                Some(id) => self.services.is_running(id),
                None => self.services.web_servers().is_ok_and(|w| w.iter().any(|s| self.services.is_running(&s.id))),
            };
            if site.enabled && binding.autostart && serving {
                if let Err(e) = self.start_docker_app(&binding) {
                    tracing::warn!(domain = %site.domain, error = %e, "container did not start");
                }
            }
            return;
        }
        if site.strategy != Strategy::ReverseProxy || !site.enabled {
            let _ = self.app_processes.stop(site_id);
            return;
        }
        if restart && self.app_processes.is_running(site_id) {
            let _ = self.app_processes.stop(site_id);
        }
        let serving = match &site.service_id {
            Some(id) => self.services.is_running(id),
            None => self.services.web_servers().is_ok_and(|w| w.iter().any(|s| self.services.is_running(&s.id))),
        };
        let wanted = self.app_command(&site).is_ok_and(|(command, _, autostart)| autostart && command.is_some());
        if serving && wanted {
            if let Err(e) = self.start_app_process(site_id) {
                tracing::warn!(domain = %site.domain, error = %e, "app process did not start");
            }
        }
    }

    /// Starts the programs of the apps a web server serves (after it starts).
    pub fn start_apps_of(&self, service_id: &str) {
        let Ok(sites) = self.autostart_apps(Some(service_id)) else { return };
        for site in sites {
            if let Err(e) = self.start_app_process(&site.id) {
                tracing::warn!(domain = %site.domain, error = %e, "app process did not start");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_known_tools_with_arguments() {
        assert_eq!(parse("cargo run --release").unwrap(), ("cargo".into(), vec!["run".into(), "--release".into()]));
        assert_eq!(parse("npm.cmd run dev -- --port \"5173\"").unwrap().1, vec!["run", "dev", "--", "--port", "5173"]);
        assert_eq!(split("go run . ''"), vec!["go", "run", ".", ""]);
        for bad in ["", "cmd /c del x", "powershell -c x", r"C:\tools\cargo.exe run", "../cargo run", "cargo&calc"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_port_comes_from_zenv_and_the_rest_from_dotenv() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "# comment\nAPP_PORT=8090\nexport DB_HOST=127.0.0.1\nDB_PASS=\"p#ss word\"\nNAME=x # note\nBIND=127.0.0.1:8090\n",
        )
        .unwrap();
        let env = read_dotenv(dir.path());
        assert!(env.contains(&("DB_HOST".into(), "127.0.0.1".into())));
        assert!(env.contains(&("DB_PASS".into(), "p#ss word".into())));
        assert!(env.contains(&("NAME".into(), "x".into())));
        assert_eq!(
            port_vars(&env, 8101),
            vec![
                ("APP_PORT".into(), "8101".into()),
                ("BIND".into(), "127.0.0.1:8101".into()),
                ("PORT".into(), "8101".into()),
                ("HOST".into(), "127.0.0.1".into())
            ]
        );
        assert_eq!(
            port_vars(&[], 8101),
            vec![("PORT".into(), "8101".into()), ("APP_PORT".into(), "8101".into()), ("HOST".into(), "127.0.0.1".into())]
        );
        // Every network becomes this computer only.
        assert_eq!(port_vars(&[("ADDR".into(), ":3000".into())], 8101)[0], ("ADDR".into(), "127.0.0.1:8101".into()));
        assert_eq!(port_vars(&[("BIND".into(), "[::]:3000".into())], 8101)[0], ("BIND".into(), "127.0.0.1:8101".into()));
        let host = port_vars(&[("APP_HOST".into(), "0.0.0.0".into())], 8101);
        assert_eq!(host[0], ("APP_HOST".into(), "127.0.0.1".into()));
        assert!(host.contains(&("APP_PORT".into(), "8101".into())), "a host alone names no port: {host:?}");
    }

    #[test]
    fn go_main_packages_under_cmd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module x").unwrap();
        for (d, pkg) in [("cmd/server", "main"), ("cmd/worker", "main"), ("internal/config", "config")] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
            std::fs::write(dir.path().join(d).join("main.go"), format!("package {pkg}\n")).unwrap();
        }
        assert_eq!(detect(dir.path()).as_deref(), Some("go run ./cmd/server"));
        std::fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("go run ."));
    }

    #[test]
    fn detected_commands_wait_for_approval_except_on_existing_apps() {
        use crate::local_sites::{PhpEngine, ProjectType, SiteInput};
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let core = Core::initialize(AppPaths::new(home.path())).unwrap();
        let root = project.path().display().to_string();
        let input = SiteInput {
            name: "a".into(),
            domain: "a.localhost".into(),
            project_root: root.clone(),
            document_root: root,
            project_type: ProjectType::Custom,
            service_id: None,
            strategy: Strategy::ReverseProxy,
            upstream_port: Some(18_123),
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        };
        let id = core.sites.create(&input).unwrap().site.id;
        let s = core.app_process_status(&id).unwrap();
        assert_eq!((s.command.as_deref(), s.detected, s.autostart, s.runs.as_deref()), (Some("npm run dev"), true, false, Some("vite")));
        assert!(core.autostart_apps(None).unwrap().is_empty(), "a new folder's code never runs by itself");
        assert!(core.start_app_process(&id).is_err());

        // An app from before approvals existed (migration 13 marks it).
        core.db
            .write(|tx| {
                tx.execute("INSERT INTO app_commands (site_id, command, autostart, updated_at, adopt_detected) VALUES (?1, NULL, 1, 'x', 1)", [&id])?;
                Ok(())
            })
            .unwrap();
        core.adopt_detected_commands().unwrap();
        let s = core.app_process_status(&id).unwrap();
        assert_eq!((s.command.as_deref(), s.detected, s.autostart), (Some("npm run dev"), false, true));
        assert_eq!(core.autostart_apps(None).unwrap().len(), 1);
    }

    #[test]
    fn shows_what_a_runner_command_runs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"scripts":{"dev":"vite --host","start":"node server.js"}}"#).unwrap();
        assert_eq!(script_body(dir.path(), "npm run dev").as_deref(), Some("vite --host"));
        assert_eq!(script_body(dir.path(), "npm start").as_deref(), Some("node server.js"));
        assert_eq!(script_body(dir.path(), "pnpm dev").as_deref(), Some("vite --host"));
        assert_eq!(script_body(dir.path(), "npm run nope"), None);
        assert_eq!(script_body(dir.path(), "cargo run"), None);
    }

    #[test]
    fn detects_the_usual_command() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect(dir.path()), None);
        std::fs::write(dir.path().join("package.json"), r#"{"scripts":{"start":"node server.js"}}"#).unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("npm start"));
        std::fs::write(dir.path().join("package.json"), r#"{"scripts":{"dev":"vite","start":"x"}}"#).unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("pnpm dev"));
        std::fs::write(dir.path().join("go.mod"), "module x").unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("go run ."));
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("cargo run"));
    }
}
