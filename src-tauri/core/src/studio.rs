//! Studio: a coding agent (Claude Code, Codex) or a shell in a real terminal
//! inside Zenv, opened in an app's project folder next to its live preview.
//!
//! Terminals are pseudo consoles (ConPTY on Windows). Only known programs
//! run: the agent's own executable found on PATH, or PowerShell; the folder
//! is always the app's project root. The app's PHP version and each
//! language's default come first on PATH, as for the app's own program.
//! Claude Code also gets a short description of how Zenv serves the app
//! (passed on its command line; nothing is written into the project).

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::process_manager::ProcessState;
use crate::runtime_catalog::{self, Category};
use crate::runtime_manager;

/// Open terminals across all Studio windows.
const MAX_SESSIONS: usize = 16;

/// Claude Code's own screens Studio may open in a terminal (`claude /mcp`, ...).
const CLAUDE_SCREENS: [&str; 14] = [
    "/mcp", "/hooks", "/permissions", "/memory", "/plugin", "/status", "/config", "/agents", "/sandbox", "/login", "/logout", "/usage", "/doctor", "/output-style",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Agent {
    ClaudeCode,
    Codex,
    Shell,
}

impl Agent {
    pub const ALL: [Agent; 3] = [Agent::ClaudeCode, Agent::Codex, Agent::Shell];

    pub fn name(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Shell => "PowerShell",
        }
    }

    pub(crate) fn install_hint(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "npm install -g @anthropic-ai/claude-code",
            Agent::Codex => "npm install -g @openai/codex",
            Agent::Shell => "",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: Agent,
    pub name: &'static str,
    /// The program Zenv runs; None when it is not installed.
    pub program: Option<PathBuf>,
    pub install_hint: &'static str,
}

/// How a program is started: `program args...`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Launch {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    /// The program is the agent itself (not cmd.exe running a script), so
    /// arguments reach it exactly as given.
    pub(crate) direct: bool,
}

/// What a terminal reports to its window.
pub enum TerminalEvent {
    Output(Vec<u8>),
    Exit(Option<u32>),
}

pub type EventSink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

struct Session {
    /// The window that opened it; only that window may use or close it.
    owner: String,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl Terminals {
    fn open(&self, owner: &str, launch: &Launch, cwd: &Path, env: &[(OsString, OsString)], size: (u16, u16), sink: EventSink) -> AppResult<String> {
        if self.sessions.lock().unwrap().len() >= MAX_SESSIONS {
            return Err(AppError::Validation(format!("at most {MAX_SESSIONS} terminals can be open; close one first")));
        }
        let (cols, rows) = size;
        let pty = native_pty_system()
            .openpty(PtySize { rows: rows.clamp(5, 500), cols: cols.clamp(20, 1000), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| AppError::Platform(format!("cannot create a terminal: {e}")))?;
        let mut cmd = CommandBuilder::new(&launch.program);
        cmd.args(&launch.args);
        cmd.cwd(cwd);
        for (k, v) in env {
            cmd.env(k, v);
        }
        // Zenv started from inside a Claude Code session must not pass that on.
        for k in ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT"] {
            cmd.env_remove(k);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let mut child = pty
            .slave
            .spawn_command(cmd)
            .map_err(|e| AppError::Platform(format!("cannot start {}: {e}", launch.program.display())))?;
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().map_err(|e| AppError::Platform(format!("terminal: {e}")))?;
        let writer = pty.master.take_writer().map_err(|e| AppError::Platform(format!("terminal: {e}")))?;
        let killer = child.clone_killer();
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), Session { owner: owner.to_string(), master: pty.master, writer, killer });

        // The program's exit closes the pseudo console, which ends the output.
        let (exit_tx, exit_rx) = mpsc::channel();
        let sessions = self.sessions.clone();
        let waited = id.clone();
        std::thread::Builder::new()
            .name("terminal-wait".into())
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code());
                let _ = exit_tx.send(code);
                // Let the last output through before the console goes.
                std::thread::sleep(Duration::from_millis(150));
                sessions.lock().unwrap().remove(&waited);
            })
            .map_err(|e| AppError::Platform(e.to_string()))?;
        std::thread::Builder::new()
            .name("terminal-read".into())
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => sink(TerminalEvent::Output(buf[..n].to_vec())),
                    }
                }
                sink(TerminalEvent::Exit(exit_rx.recv_timeout(Duration::from_secs(5)).ok().flatten()));
            })
            .map_err(|e| AppError::Platform(e.to_string()))?;
        Ok(id)
    }

    fn with<T>(&self, owner: &str, id: &str, f: impl FnOnce(&mut Session) -> T) -> AppResult<T> {
        let mut sessions = self.sessions.lock().unwrap();
        match sessions.get_mut(id) {
            Some(s) if s.owner == owner => Ok(f(s)),
            _ => Err(AppError::NotFound { entity: "terminal", id: id.to_string() }),
        }
    }

    pub fn write(&self, owner: &str, id: &str, data: &[u8]) -> AppResult<()> {
        self.with(owner, id, |s| s.writer.write_all(data).and_then(|_| s.writer.flush()))?
            .map_err(|e| AppError::Platform(format!("terminal input: {e}")))
    }

    pub fn resize(&self, owner: &str, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let size = PtySize { rows: rows.clamp(5, 500), cols: cols.clamp(20, 1000), pixel_width: 0, pixel_height: 0 };
        self.with(owner, id, |s| s.master.resize(size))?.map_err(|e| AppError::Platform(format!("terminal resize: {e}")))
    }

    /// Ends the terminal's program (the session goes once it has exited).
    pub fn close(&self, owner: &str, id: &str) -> AppResult<()> {
        self.with(owner, id, |s| {
            let _ = s.killer.kill();
        })
    }

    /// A Studio window closed: end its terminals.
    pub fn close_owned_by(&self, owner: &str) {
        for s in self.sessions.lock().unwrap().values_mut().filter(|s| s.owner == owner) {
            let _ = s.killer.kill();
        }
    }

    pub fn close_all(&self) {
        for s in self.sessions.lock().unwrap().values_mut() {
            let _ = s.killer.kill();
        }
    }
}

/// `name.exe`, `.cmd`, ... in the first folder on `path` that has one.
pub(crate) fn find(name: &str, path: &OsString) -> Option<PathBuf> {
    std::env::split_paths(path).find_map(|dir| {
        ["exe", "cmd", "bat"].iter().map(|ext| dir.join(format!("{name}.{ext}"))).find(|p| p.is_file())
    })
}

fn is_script(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

/// The agent's program on `path`. npm's `claude.cmd` only starts the native
/// `claude.exe` it installed, so that is run directly; other scripts run
/// through cmd.exe.
pub(crate) fn resolve(agent: Agent, path: &OsString) -> Option<Launch> {
    let script = |p: PathBuf| Launch {
        program: std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cmd.exe")),
        args: vec!["/d".into(), "/c".into(), p.into_os_string()],
        direct: false,
    };
    let exe = |p: PathBuf| Launch { program: p, args: vec![], direct: true };
    match agent {
        Agent::ClaudeCode | Agent::Codex => {
            let found = find(if agent == Agent::ClaudeCode { "claude" } else { "codex" }, path)?;
            if !is_script(&found) {
                return Some(exe(found));
            }
            if agent == Agent::ClaudeCode {
                let native = found.parent()?.join(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe");
                if native.is_file() {
                    return Some(exe(native));
                }
            }
            Some(script(found))
        }
        Agent::Shell => {
            let shell = find("pwsh", path).filter(|p| !is_script(p)).or_else(|| {
                let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
                Some(root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe")).filter(|p| p.is_file())
            });
            match shell {
                Some(p) => Some(Launch { program: p, args: vec!["-NoLogo".into()], direct: true }),
                None => Some(exe(std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cmd.exe")))),
            }
        }
    }
}

pub(crate) fn path_of(env: &[(OsString, OsString)]) -> OsString {
    env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).or_else(|| std::env::var_os("PATH")).unwrap_or_default()
}

impl Core {
    /// Each language's default version, with the app's own PHP version
    /// instead of the default one.
    pub(crate) fn studio_env(&self, site_id: &str) -> AppResult<Vec<(OsString, OsString)>> {
        let site = self.sites.get(site_id)?;
        let mut chosen = BTreeMap::new();
        if let Some(php) = site.runtime_installation_id.as_deref().and_then(|id| runtime_manager::get(&self.db, id).ok()) {
            if php.executable_exists {
                chosen.insert(php.component.as_str(), php);
            }
        }
        for inst in runtime_manager::list(&self.db, None)? {
            if runtime_catalog::info(inst.component).category == Category::Language && inst.executable_exists {
                chosen.entry(inst.component.as_str()).or_insert(inst);
            }
        }
        let installs: Vec<_> = chosen.into_values().collect();
        Ok(crate::runtime_env::build(&installs, std::env::var_os("PATH")))
    }

    /// The agents Studio offers, and which of them are installed.
    pub fn studio_agents(&self, site_id: &str) -> AppResult<Vec<AgentInfo>> {
        let path = path_of(&self.studio_env(site_id)?);
        Ok(Agent::ALL
            .iter()
            .map(|&a| AgentInfo { id: a, name: a.name(), program: resolve(a, &path).map(|l| l.program), install_hint: a.install_hint() })
            .collect())
    }

    /// Starts `agent` in a new terminal in the app's project folder.
    /// `screen` opens one of Claude Code's own screens (see CLAUDE_SCREENS).
    pub fn studio_open_terminal(&self, owner: &str, site_id: &str, agent: Agent, screen: Option<&str>, size: (u16, u16), sink: EventSink) -> AppResult<String> {
        if let Some(screen) = screen {
            if agent != Agent::ClaudeCode || !CLAUDE_SCREENS.contains(&screen) {
                return Err(AppError::Validation(format!("{screen} can't be opened here")));
            }
        }
        let site = self.sites.get(site_id)?;
        if !site.project_root.is_dir() {
            return Err(AppError::invalid_path(&site.project_root, "is not an existing folder"));
        }
        let env = self.studio_env(site_id)?;
        let mut launch = resolve(agent, &path_of(&env))
            .ok_or_else(|| AppError::Validation(format!("{} is not installed. Install it with: {}", agent.name(), agent.install_hint())))?;
        if agent == Agent::ClaudeCode && launch.direct {
            if let Ok(context) = self.studio_context(site_id) {
                launch.args.extend(["--append-system-prompt".into(), context.into()]);
            }
        }
        if let Some(screen) = screen {
            launch.args.push(screen.into());
        }
        tracing::info!(domain = %site.domain, agent = agent.name(), program = %launch.program.display(), "studio terminal");
        self.studio.open(owner, &launch, &site.project_root, &env, size, sink)
    }

    /// How Zenv serves the app, for the agent.
    pub(crate) fn studio_context(&self, site_id: &str) -> AppResult<String> {
        let view = self.sites.view(site_id)?;
        let site = &view.site;
        let mut lines = vec![
            "This project runs locally under Zenv, a Windows development environment manager. Facts about it:".to_string(),
            format!("- The app is served at {} by {}.", view.url, view.service_name),
        ];
        if let Some(v) = &view.php_version {
            lines.push(format!("- PHP {v} (the `php` on PATH in this terminal is that version)."));
        }
        if let (Some(port), true) = (site.upstream_port, site.strategy == crate::local_sites::Strategy::ReverseProxy) {
            lines.push(format!("- The web server forwards requests to the app's own program on 127.0.0.1:{port}; Zenv starts and stops that program."));
        }
        let running: Vec<String> = self
            .services
            .statuses()?
            .into_iter()
            .filter(|s| matches!(s.process, ProcessState::Running { .. }))
            .filter(|s| matches!(s.kind, Category::Database | Category::Cache))
            .map(|s| format!("{} on {}:{}", s.service.name, s.service.listen_address, s.service.http_port))
            .collect();
        if !running.is_empty() {
            lines.push(format!("- Running databases and caches: {}.", running.join(", ")));
        }
        let logs: Vec<String> = self
            .sites
            .log_sources(site_id)?
            .into_iter()
            .filter(|l| l.exists)
            .map(|l| format!("{}: {}", l.label, l.path.display()))
            .collect();
        if !logs.is_empty() {
            lines.push(format!("- Logs: {}.", logs.join("; ")));
        }
        lines.push("- Web server configuration, the hosts file and service ports are managed by Zenv: do not edit files under the Zenv home; ask the user to change those settings in Zenv.".into());
        lines.push("- The user watches the app in a live preview next to this terminal; after changing code, they reload it there.".into());
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_claude_shim_runs_the_native_program() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude.cmd"), "@echo off").unwrap();
        let native = dir.path().join(r"node_modules\@anthropic-ai\claude-code\bin");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::write(native.join("claude.exe"), "").unwrap();
        let launch = resolve(Agent::ClaudeCode, &dir.path().as_os_str().to_owned()).unwrap();
        assert_eq!(launch.program, native.join("claude.exe"));
        assert!(launch.direct);
    }

    #[test]
    fn other_scripts_run_through_cmd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("codex.cmd"), "@echo off").unwrap();
        let launch = resolve(Agent::Codex, &dir.path().as_os_str().to_owned()).unwrap();
        assert!(!launch.direct);
        assert_eq!(launch.args.last().unwrap(), dir.path().join("codex.cmd").as_os_str());
    }

    #[test]
    fn missing_agent_is_none_but_a_shell_always_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().as_os_str().to_owned();
        assert!(resolve(Agent::ClaudeCode, &path).is_none());
        assert!(resolve(Agent::Shell, &path).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn a_terminal_runs_its_program_and_reports_the_exit() {
        let (tx, rx) = mpsc::channel();
        let out = Arc::new(Mutex::new(Vec::new()));
        let seen = out.clone();
        let sink: EventSink = Arc::new(move |e| match e {
            TerminalEvent::Output(b) => seen.lock().unwrap().extend(b),
            TerminalEvent::Exit(code) => {
                let _ = tx.send(code);
            }
        });
        let terminals = Terminals::default();
        let launch = Launch {
            program: std::env::var_os("ComSpec").map(PathBuf::from).unwrap(),
            args: vec!["/d".into(), "/c".into(), "echo zenv-studio-ok && exit 3".into()],
            direct: false,
        };
        let id = terminals.open("studio-test", &launch, &std::env::temp_dir(), &[], (80, 24), sink).unwrap();
        assert!(terminals.write("someone-else", &id, b"x").is_err(), "only the owner may type");
        // The pseudo console asks where the cursor is before it runs the
        // program; xterm.js answers that in the app.
        let mut answered = false;
        let code = loop {
            if !answered && out.lock().unwrap().windows(4).any(|w| w == b"[6n") {
                terminals.write("studio-test", &id, b"[1;1R").unwrap();
                answered = true;
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(code) => break code,
                Err(_) => assert!(out.lock().unwrap().len() < 1 << 20),
            }
        };
        assert_eq!(code, Some(3));
        assert!(String::from_utf8_lossy(&out.lock().unwrap()).contains("zenv-studio-ok"));
        std::thread::sleep(Duration::from_millis(300));
        assert!(terminals.sessions.lock().unwrap().is_empty());
    }
}
