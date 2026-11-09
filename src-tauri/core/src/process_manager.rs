//! Owns child processes started by Zenv. Callers pass fully resolved
//! programs and arguments built by adapters; nothing here is reachable from the
//! frontend with arbitrary input.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{AppError, AppResult, IoContext};
use crate::platform::{self, Job};
use crate::util::now_rfc3339;

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    /// stdout and stderr are both appended here.
    pub log_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ProcessState {
    #[serde(rename_all = "camelCase")]
    Running { pid: u32, pids: Vec<u32>, started_at: String },
    #[serde(rename_all = "camelCase")]
    Exited { exit: ExitInfo },
    NotStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StopMethod {
    Graceful,
    Terminated,
    AlreadyExited,
}

struct Managed {
    child: Child,
    job: Job,
    started_at: String,
}

#[derive(Default)]
struct Table {
    running: HashMap<String, Managed>,
    last_exit: HashMap<String, ExitInfo>,
}

#[derive(Default)]
pub struct ProcessManager {
    table: Mutex<Table>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Table> {
        self.table.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn spawn(&self, key: &str, spec: &ProcessSpec) -> AppResult<u32> {
        let mut table = self.lock();
        reap(&mut table);
        if table.running.contains_key(key) {
            return Err(AppError::Conflict(format!("process '{key}' is already running")));
        }
        if !spec.program.is_file() {
            return Err(AppError::invalid_path(&spec.program, "executable does not exist"));
        }

        if let Some(dir) = spec.log_file.parent() {
            std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
        }
        rotate_if_large(&spec.log_file)?;
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.log_file)
            .ctx(|| format!("opening {}", spec.log_file.display()))?;
        let args_display: Vec<_> = spec.args.iter().map(|a| a.to_string_lossy()).collect();
        writeln!(
            log,
            "=== {} Zenv starting {} {}",
            now_rfc3339(),
            spec.program.display(),
            args_display.join(" ")
        )
        .ctx(|| "writing log header".into())?;
        let stderr = log.try_clone().ctx(|| "cloning log handle".into())?;

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        platform::hide_console_window(&mut cmd);

        let job = Job::new()?;
        let mut child = cmd.spawn().ctx(|| format!("launching {}", spec.program.display()))?;
        if let Err(e) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let pid = child.id();
        table.last_exit.remove(key);
        table.running.insert(
            key.to_string(),
            Managed { child, job, started_at: now_rfc3339() },
        );
        tracing::info!(key, pid, program = %spec.program.display(), "process started");
        Ok(pid)
    }

    pub fn state(&self, key: &str) -> ProcessState {
        let mut table = self.lock();
        reap(&mut table);
        if let Some(m) = table.running.get(key) {
            let pid = m.child.id();
            let pids = m.job.process_ids().unwrap_or_else(|_| vec![pid]);
            return ProcessState::Running { pid, pids, started_at: m.started_at.clone() };
        }
        match table.last_exit.get(key) {
            Some(exit) => ProcessState::Exited { exit: exit.clone() },
            None => ProcessState::NotStarted,
        }
    }

    pub fn is_running(&self, key: &str) -> bool {
        matches!(self.state(key), ProcessState::Running { .. })
    }

    /// Stops a process: optional graceful request, wait up to `grace`, then
    /// terminate the whole job (process tree).
    pub fn stop(
        &self,
        key: &str,
        graceful: Option<&dyn Fn(u32) -> AppResult<()>>,
        grace: Duration,
    ) -> AppResult<StopMethod> {
        let pid = match self.state(key) {
            ProcessState::Running { pid, .. } => pid,
            _ => return Ok(StopMethod::AlreadyExited),
        };

        if let Some(request) = graceful {
            match request(pid) {
                Ok(()) => {
                    if self.wait_for_exit(key, grace) {
                        tracing::info!(key, pid, "process stopped gracefully");
                        return Ok(StopMethod::Graceful);
                    }
                    tracing::warn!(key, pid, "graceful stop timed out; terminating job");
                }
                Err(e) => tracing::warn!(key, pid, error = %e, "graceful stop request failed; terminating job"),
            }
        }

        let mut table = self.lock();
        if let Some(mut m) = table.running.remove(key) {
            m.job.terminate(1)?;
            let status = m.child.wait().ctx(|| format!("waiting for '{key}'"))?;
            // Termination is asynchronous: a child that holds the port (a
            // server started by a CLI, like OmniRoute's) may outlive the
            // main process briefly, and a restart would find the port taken.
            let deadline = Instant::now() + Duration::from_secs(10);
            while m.job.process_ids().is_ok_and(|p| !p.is_empty()) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            table.last_exit.insert(key.to_string(), ExitInfo { code: status.code(), at: now_rfc3339() });
        }
        Ok(StopMethod::Terminated)
    }

    /// Returns true once the process has exited within `timeout`.
    pub fn wait_for_exit(&self, key: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.is_running(key) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn running_keys(&self) -> Vec<String> {
        let mut table = self.lock();
        reap(&mut table);
        table.running.keys().cloned().collect()
    }
}

/// Moves exited children from `running` to `last_exit`.
fn reap(table: &mut Table) {
    let mut exited = Vec::new();
    for (key, m) in table.running.iter_mut() {
        if let Ok(Some(status)) = m.child.try_wait() {
            exited.push((key.clone(), status.code()));
        }
    }
    for (key, code) in exited {
        table.running.remove(&key);
        tracing::info!(key, ?code, "process exited");
        table.last_exit.insert(key, ExitInfo { code, at: now_rfc3339() });
    }
}

fn rotate_if_large(path: &std::path::Path) -> AppResult<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_LOG_BYTES {
            let rotated = path.with_extension("log.1");
            std::fs::rename(path, &rotated).ctx(|| format!("rotating {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn system32(exe: &str) -> PathBuf {
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("System32")
            .join(exe)
    }

    fn spec(dir: &tempfile::TempDir, program: &str, args: &[&str]) -> ProcessSpec {
        ProcessSpec {
            program: system32(program),
            args: args.iter().map(OsString::from).collect(),
            cwd: dir.path().to_path_buf(),
            env: vec![],
            log_file: dir.path().join("logs").join("process.log"),
        }
    }

    #[test]
    fn lifecycle_running_then_terminated() {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProcessManager::new();
        let pid = pm.spawn("svc", &spec(&dir, "PING.EXE", &["-n", "60", "127.0.0.1"])).unwrap();
        match pm.state("svc") {
            ProcessState::Running { pid: p, pids, .. } => {
                assert_eq!(p, pid);
                assert!(pids.contains(&pid));
            }
            other => panic!("expected running, got {other:?}"),
        }
        assert!(matches!(
            pm.spawn("svc", &spec(&dir, "PING.EXE", &["-n", "1", "127.0.0.1"])),
            Err(AppError::Conflict(_))
        ));

        let method = pm.stop("svc", None, Duration::from_secs(1)).unwrap();
        assert_eq!(method, StopMethod::Terminated);
        assert!(matches!(pm.state("svc"), ProcessState::Exited { .. }));
        assert_eq!(pm.stop("svc", None, Duration::ZERO).unwrap(), StopMethod::AlreadyExited);

        let log = std::fs::read_to_string(dir.path().join("logs").join("process.log")).unwrap();
        assert!(log.contains("Zenv starting"));
        assert!(log.contains("127.0.0.1"), "ping output captured: {log}");
    }

    #[test]
    fn exit_code_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProcessManager::new();
        pm.spawn("quick", &spec(&dir, "cmd.exe", &["/C", "exit 3"])).unwrap();
        assert!(pm.wait_for_exit("quick", Duration::from_secs(10)));
        match pm.state("quick") {
            ProcessState::Exited { exit } => assert_eq!(exit.code, Some(3)),
            other => panic!("expected exited, got {other:?}"),
        }
    }

    #[test]
    fn terminating_the_job_kills_grandchildren() {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProcessManager::new();
        pm.spawn("tree", &spec(&dir, "cmd.exe", &["/C", "PING.EXE -n 60 127.0.0.1"])).unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        let grandchild = loop {
            if let ProcessState::Running { pid, pids, .. } = pm.state("tree") {
                if let Some(&g) = pids.iter().find(|&&p| p != pid) {
                    break g;
                }
            }
            assert!(Instant::now() < deadline, "grandchild never appeared in job");
            std::thread::sleep(Duration::from_millis(50));
        };

        pm.stop("tree", None, Duration::ZERO).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            platform::process_image_name(grandchild).is_none(),
            "grandchild {grandchild} survived job termination"
        );
    }

    #[test]
    fn graceful_callback_is_used_when_process_exits() {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProcessManager::new();
        pm.spawn("g", &spec(&dir, "cmd.exe", &["/C", "exit 0"])).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        // Already exited before stop: graceful callback must not even run.
        let called = std::cell::Cell::new(false);
        let cb = |_pid: u32| {
            called.set(true);
            Ok(())
        };
        assert_eq!(pm.stop("g", Some(&cb), Duration::from_secs(1)).unwrap(), StopMethod::AlreadyExited);
        assert!(!called.get());
    }

    #[test]
    fn missing_executable_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProcessManager::new();
        let mut s = spec(&dir, "PING.EXE", &[]);
        s.program = dir.path().join("nope.exe");
        assert!(matches!(pm.spawn("x", &s), Err(AppError::InvalidPath { .. })));
    }
}
