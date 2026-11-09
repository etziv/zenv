//! Apps that run in Docker containers.
//!
//! Such an app is an ordinary reverse-proxy app (`<name>.test` → the port the
//! container publishes on this computer), so the web servers, HTTPS, preview,
//! thumbnails and the tailnet work as for any other app. What differs is its
//! program: instead of a process Zenv spawns, the app is tied to a container
//! (`docker_apps`, migration v12) that Zenv starts with the app (`docker
//! compose up -d <service>` for compose services, so their dependencies start
//! too; `docker start` otherwise) and stops on request. Docker Desktop is
//! started when it is not running. Containers keep running when Zenv quits:
//! they are Docker's, not Zenv's.
//!
//! Everything goes through the `docker` command (no shell, arguments passed
//! as they are).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::local_sites::{SiteInput, SiteView, Strategy};
use crate::runtime_manager::probe::run_capture;
use crate::util::now_rfc3339;

/// A port a container publishes on this computer.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct PublishedPort {
    pub host_port: u16,
    pub container_port: u16,
}

/// The compose service a container belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComposeService {
    pub project: String,
    pub service: String,
    /// The project's folder.
    pub dir: Option<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub name: String,
    pub image: String,
    /// Docker's state: running, exited, created, paused, restarting, dead.
    pub state: String,
    /// e.g. "Up 2 days", "Exited (255) 2 weeks ago".
    pub status: String,
    /// TCP ports published on this computer; empty: not reachable from Windows.
    pub ports: Vec<PublishedPort>,
    pub compose: Option<ComposeService>,
}

/// What an app is tied to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DockerBinding {
    pub container: String,
    pub compose: Option<ComposeService>,
    #[serde(default = "yes")]
    pub autostart: bool,
}

fn yes() -> bool {
    true
}

/// The container of a Docker app, as the app panel shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerAppStatus {
    pub container: String,
    pub compose: Option<ComposeService>,
    /// Docker's state, "missing" when the container is gone, "unknown" when
    /// Docker does not answer.
    pub state: String,
    pub exit_code: Option<i32>,
    pub image: Option<String>,
    /// Docker Desktop (the engine) answers.
    pub engine_running: bool,
    pub problem: Option<String>,
}

// ----- the docker command ---------------------------------------------------------

/// docker.exe: on PATH, else where Docker Desktop puts it.
pub fn docker_exe() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("docker.exe")).find(|p| p.is_file()));
    on_path.or_else(|| {
        ["ProgramFiles", "ProgramW6432"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|pf| PathBuf::from(pf).join(r"Docker\Docker\resources\bin\docker.exe"))
            .find(|p| p.is_file())
    })
}

fn desktop_exe() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|pf| PathBuf::from(pf).join(r"Docker\Docker\Docker Desktop.exe"))
        .find(|p| p.is_file())
}

/// The engine is down (Docker Desktop not started).
fn engine_down(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("error during connect")
        || t.contains("cannot connect to the docker daemon")
        || t.contains("dockerdesktoplinuxengine")
        || t.contains("docker desktop is not running")
        || t.contains("the system cannot find the file specified")
}

/// Runs `docker <args>`; its output, or an error with what docker said.
fn docker(args: &[&str], timeout: Duration) -> AppResult<String> {
    let exe = docker_exe().ok_or_else(|| AppError::Conflict("Docker is not installed (docker.exe was not found)".into()))?;
    let out = run_capture(&exe, args, timeout).map_err(|e| AppError::Platform(format!("docker {}: {e}", args.first().unwrap_or(&""))))?;
    if out.code == Some(0) {
        return Ok(out.text);
    }
    if engine_down(&out.text) {
        return Err(AppError::Conflict("Docker Desktop is not running".into()));
    }
    let msg = out.text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("failed").to_string();
    Err(AppError::Platform(format!("docker {}: {msg}", args.first().unwrap_or(&""))))
}

pub fn engine_running() -> bool {
    docker(&["version", "--format", "{{.Server.Version}}"], Duration::from_secs(15)).is_ok()
}

/// Starts Docker Desktop when the engine is down and waits for it (up to two
/// minutes). Returns whether it had to be started.
pub fn ensure_engine() -> AppResult<bool> {
    if engine_running() {
        return Ok(false);
    }
    let desktop = desktop_exe().ok_or_else(|| AppError::Conflict("Docker Desktop is not running and was not found; start Docker yourself".into()))?;
    tracing::info!("starting Docker Desktop");
    std::process::Command::new(&desktop)
        .spawn()
        .map_err(|e| AppError::Platform(format!("cannot start Docker Desktop: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(3));
        if engine_running() {
            return Ok(true);
        }
    }
    Err(AppError::Platform("Docker Desktop did not become ready within 2 minutes".into()))
}

// ----- parsing docker's output ------------------------------------------------------

/// `docker ps --format "{{json .}}"`, one object per line.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsLine {
    names: String,
    image: String,
    state: String,
    status: String,
    ports: String,
    labels: String,
}

/// `0.0.0.0:38080->8080/tcp, [::]:38080->8080/tcp, 80/tcp` → [38080→8080].
pub fn parse_ports(text: &str) -> Vec<PublishedPort> {
    let mut out = BTreeSet::new();
    for entry in text.split(',').map(str::trim) {
        let Some((host, container)) = entry.split_once("->") else { continue };
        let Some(container) = container.strip_suffix("/tcp") else { continue };
        let host = host.rsplit_once(':').map_or(host, |(_, p)| p);
        let range = |s: &str| -> Option<(u16, u16)> {
            match s.split_once('-') {
                Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
                None => s.parse().ok().map(|p| (p, p)),
            }
        };
        let (Some((h0, h1)), Some((c0, _))) = (range(host), range(container)) else { continue };
        for (i, h) in (h0..=h1).take(32).enumerate() {
            out.insert(PublishedPort { host_port: h, container_port: c0.saturating_add(i as u16) });
        }
    }
    out.into_iter().collect()
}

/// `a=1,b=2`. Docker joins labels with commas without escaping, and compose's
/// `config_files` itself is a comma list: a piece without `=` continues the
/// label before it.
fn parse_labels(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut last: Option<String> = None;
    for piece in text.split(',') {
        match piece.split_once('=') {
            Some((k, v)) => {
                let k = k.trim().to_string();
                out.insert(k.clone(), v.to_string());
                last = Some(k);
            }
            None => {
                if let Some(v) = last.as_ref().and_then(|k| out.get_mut(k)) {
                    v.push(',');
                    v.push_str(piece);
                }
            }
        }
    }
    out
}

fn compose_of(labels: &BTreeMap<String, String>) -> Option<ComposeService> {
    let project = labels.get("com.docker.compose.project")?.clone();
    let service = labels.get("com.docker.compose.service")?.clone();
    let files = labels
        .get("com.docker.compose.project.config_files")
        .map(|f| f.split([',', ';']).map(str::trim).filter(|f| !f.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    Some(ComposeService { project, service, dir: labels.get("com.docker.compose.project.working_dir").cloned(), files })
}

pub fn parse_ps(text: &str) -> Vec<DockerContainer> {
    let mut list: Vec<DockerContainer> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<PsLine>(l.trim()).ok())
        .map(|p| {
            let labels = parse_labels(&p.labels);
            DockerContainer {
                // A container may have several names; the first is the one to use.
                name: p.names.split(',').next().unwrap_or_default().trim().to_string(),
                image: p.image,
                state: p.state,
                status: p.status,
                ports: parse_ports(&p.ports),
                compose: compose_of(&labels),
            }
        })
        .collect();
    // Running first, then by name.
    list.sort_by(|a, b| (b.state == "running").cmp(&(a.state == "running")).then_with(|| a.name.cmp(&b.name)));
    list
}

/// Container, compose project and service names: what Docker allows.
fn valid_name(what: &str, name: &str) -> AppResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 128
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(AppError::Validation(format!("'{name}' is not a valid Docker {what} name")))
    }
}

fn validate(binding: &DockerBinding) -> AppResult<()> {
    valid_name("container", &binding.container)?;
    if let Some(c) = &binding.compose {
        valid_name("compose project", &c.project)?;
        valid_name("compose service", &c.service)?;
    }
    Ok(())
}

/// `{{json .HostConfig.PortBindings}}`: the ports a container publishes once
/// it runs (`docker ps` shows them only after it ran). Random ports ("")
/// are not known before then.
pub fn parse_port_bindings(json: &str) -> Vec<PublishedPort> {
    #[derive(Deserialize)]
    struct Binding {
        #[serde(rename = "HostPort", default)]
        host_port: String,
    }
    let map: BTreeMap<String, Option<Vec<Binding>>> = serde_json::from_str(json).unwrap_or_default();
    let mut out = BTreeSet::new();
    for (key, bindings) in map {
        let Some(container_port) = key.strip_suffix("/tcp").and_then(|p| p.parse().ok()) else { continue };
        for b in bindings.unwrap_or_default() {
            if let Ok(host_port) = b.host_port.parse() {
                out.insert(PublishedPort { host_port, container_port });
            }
        }
    }
    out.into_iter().collect()
}

pub fn list_containers() -> AppResult<Vec<DockerContainer>> {
    let mut list = parse_ps(&docker(&["ps", "-a", "--no-trunc", "--format", "{{json .}}"], Duration::from_secs(20))?);
    // Containers that never ran: their configured ports, in one call.
    let unknown: Vec<&str> = list.iter().filter(|c| c.ports.is_empty() && c.state != "running").map(|c| c.name.as_str()).collect();
    if !unknown.is_empty() {
        let mut args = vec!["inspect", "--type", "container", "--format", "{{.Name}}|{{json .HostConfig.PortBindings}}"];
        args.extend(unknown);
        if let Ok(text) = docker(&args, Duration::from_secs(20)) {
            let bindings: BTreeMap<String, Vec<PublishedPort>> = text
                .lines()
                .filter_map(|l| l.trim().split_once('|'))
                .map(|(name, json)| (name.trim_start_matches('/').to_string(), parse_port_bindings(json)))
                .collect();
            for c in list.iter_mut().filter(|c| c.ports.is_empty()) {
                if let Some(ports) = bindings.get(&c.name) {
                    c.ports = ports.clone();
                }
            }
        }
    }
    Ok(list)
}

/// (state, exit code, image) of a container; None when it does not exist.
fn inspect(container: &str) -> AppResult<Option<(String, Option<i32>, String)>> {
    match docker(&["inspect", "--type", "container", "--format", "{{.State.Status}}|{{.State.ExitCode}}|{{.Config.Image}}", container], Duration::from_secs(10)) {
        Ok(text) => {
            let line = text.lines().next().unwrap_or_default().trim().to_string();
            let mut parts = line.splitn(3, '|');
            let state = parts.next().unwrap_or_default().to_string();
            let code = parts.next().and_then(|c| c.parse().ok());
            Ok(Some((state, code, parts.next().unwrap_or_default().to_string())))
        }
        Err(AppError::Platform(msg)) if msg.to_ascii_lowercase().contains("no such") => Ok(None),
        Err(e) => Err(e),
    }
}

/// A compose file named by the container's labels that Zenv may hand to
/// `docker compose up`: a YAML file inside the project's own folder (after
/// resolving links). Labels are text anyone creating a container can set, and
/// `up` starts whatever the file says the service depends on.
fn usable_compose_file(dir: &Path, file: &str) -> bool {
    let f = Path::new(file);
    let yaml = f.extension().is_some_and(|x| x.eq_ignore_ascii_case("yml") || x.eq_ignore_ascii_case("yaml"));
    let (Ok(dir), Ok(real)) = (dunce::canonicalize(dir), dunce::canonicalize(f)) else { return false };
    yaml && f.is_absolute() && real.is_file() && real.starts_with(&dir)
}

/// Compose arguments that address the project from any folder; None when its
/// files cannot be used (then the container is started by itself).
fn compose_args<'a>(c: &'a ComposeService) -> Option<Vec<&'a str>> {
    let dir = c.dir.as_deref().filter(|d| Path::new(d).is_dir())?;
    let files: Vec<&str> = c.files.iter().map(String::as_str).filter(|f| usable_compose_file(Path::new(dir), f)).collect();
    if files.is_empty() || files.len() != c.files.len() {
        return None;
    }
    let mut args = vec!["compose", "-p", c.project.as_str()];
    for f in files {
        args.extend(["-f", f]);
    }
    args.extend(["--project-directory", dir]);
    Some(args)
}

/// Containers Zenv started in this session (stopped again by "Stop all").
static STARTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn started() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    STARTED.lock().unwrap_or_else(|p| p.into_inner())
}

// ----- Zenv's side ---------------------------------------------------------------------

impl Core {
    pub fn docker_binding(&self, site_id: &str) -> AppResult<Option<DockerBinding>> {
        let row: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, bool)> = self.db.read(|c| {
            Ok(c.query_row(
                "SELECT container, compose_project, compose_service, compose_dir, compose_files, autostart FROM docker_apps WHERE site_id = ?1",
                [site_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?)
        })?;
        Ok(row.map(|(container, project, service, dir, files, autostart)| DockerBinding {
            container,
            compose: project.zip(service).map(|(project, service)| ComposeService {
                project,
                service,
                dir,
                files: files.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default(),
            }),
            autostart,
        }))
    }

    fn save_docker_binding(&self, site_id: &str, b: &DockerBinding) -> AppResult<()> {
        validate(b)?;
        let files = b.compose.as_ref().map(|c| serde_json::to_string(&c.files).unwrap_or_default());
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO docker_apps (site_id, container, compose_project, compose_service, compose_dir, compose_files, autostart, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(site_id) DO UPDATE SET container = ?2, compose_project = ?3, compose_service = ?4, compose_dir = ?5,
                     compose_files = ?6, autostart = ?7, updated_at = ?8",
                params![
                    site_id,
                    b.container,
                    b.compose.as_ref().map(|c| &c.project),
                    b.compose.as_ref().map(|c| &c.service),
                    b.compose.as_ref().and_then(|c| c.dir.as_ref()),
                    files,
                    b.autostart,
                    now_rfc3339()
                ],
            )?;
            Ok(())
        })
    }

    /// Adds an app served from a container: a reverse proxy to the
    /// container's published port, tied to it. Its folder is the compose
    /// project's, else Zenv's `dev` folder.
    pub fn create_docker_app(&self, input: &SiteInput, binding: &DockerBinding) -> AppResult<SiteView> {
        validate(binding)?;
        let mut input = input.clone();
        input.strategy = Strategy::ReverseProxy;
        if input.project_root.trim().is_empty() {
            let dir = binding.compose.as_ref().and_then(|c| c.dir.clone()).filter(|d| Path::new(d).is_dir());
            input.project_root = dir.unwrap_or_else(|| self.paths.dev_dir.display().to_string());
        }
        if input.document_root.trim().is_empty() {
            input.document_root = input.project_root.clone();
        }
        let view = self.sites.create(&input)?;
        if let Err(e) = self.save_docker_binding(&view.site.id, binding) {
            let _ = self.sites.delete(&view.site.id);
            return Err(e);
        }
        Ok(view)
    }

    pub fn set_docker_autostart(&self, site_id: &str, on: bool) -> AppResult<()> {
        let mut b = self.docker_binding(site_id)?.ok_or_else(|| AppError::Validation("this app has no container".into()))?;
        b.autostart = on;
        self.save_docker_binding(site_id, &b)
    }

    /// Containers on this computer, for choosing one.
    pub fn docker_containers(&self) -> AppResult<Vec<DockerContainer>> {
        list_containers()
    }

    pub fn docker_app_status(&self, binding: &DockerBinding) -> DockerAppStatus {
        let (state, exit_code, image, engine, problem) = match inspect(&binding.container) {
            Ok(Some((state, code, image))) => (state, code, Some(image), true, None),
            Ok(None) => ("missing".into(), None, None, true, Some(format!("container {} no longer exists", binding.container))),
            Err(e) => ("unknown".into(), None, None, false, Some(e.to_string())),
        };
        DockerAppStatus { container: binding.container.clone(), compose: binding.compose.clone(), state, exit_code, image, engine_running: engine, problem }
    }

    /// Starts an app's container (and, for compose, what it depends on),
    /// starting Docker Desktop first when needed. Returns false when it
    /// already ran.
    pub fn start_docker_app(&self, binding: &DockerBinding) -> AppResult<bool> {
        ensure_engine()?;
        if inspect(&binding.container)?.is_some_and(|(state, _, _)| state == "running") {
            return Ok(false);
        }
        let composed = binding.compose.as_ref().and_then(|c| {
            let mut args = compose_args(c)?;
            args.extend(["up", "-d", "--no-recreate", "--no-build", c.service.as_str()]);
            Some(docker(&args, Duration::from_secs(180)))
        });
        match composed {
            Some(Ok(_)) => {}
            // The compose files moved or changed: the container itself still starts.
            Some(Err(e)) => {
                tracing::warn!(container = %binding.container, error = %e, "compose up failed; starting the container");
                docker(&["start", &binding.container], Duration::from_secs(120))?;
            }
            None => {
                docker(&["start", &binding.container], Duration::from_secs(120))?;
            }
        }
        started().insert(binding.container.clone());
        tracing::info!(container = %binding.container, "container started");
        Ok(true)
    }

    pub fn stop_docker_app(&self, binding: &DockerBinding) -> AppResult<()> {
        if inspect(&binding.container)?.is_some_and(|(state, _, _)| state == "running") {
            docker(&["stop", &binding.container], Duration::from_secs(90))?;
        }
        started().remove(&binding.container);
        Ok(())
    }

    /// Stops the containers Zenv started ("Stop all"); returns how many.
    pub fn stop_started_containers(&self) -> usize {
        let list: Vec<String> = std::mem::take(&mut *started()).into_iter().collect();
        if list.is_empty() || !engine_running() {
            return 0;
        }
        let mut args = vec!["stop"];
        args.extend(list.iter().map(String::as_str));
        match docker(&args, Duration::from_secs(120)) {
            Ok(_) => list.len(),
            Err(e) => {
                tracing::warn!(error = %e, "containers not stopped");
                0
            }
        }
    }

    /// Copies the container's recent output into the app's output log (shown
    /// as "Output").
    pub fn refresh_docker_log(&self, site_id: &str) -> AppResult<()> {
        let Some(b) = self.docker_binding(site_id)? else { return Ok(()) };
        let exe = docker_exe().ok_or_else(|| AppError::Conflict("Docker is not installed".into()))?;
        let out = run_capture(&exe, &["logs", "--tail", "1000", "--timestamps", &b.container], Duration::from_secs(20))?;
        let file = crate::app_process::log_file(&self.paths, site_id);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| AppError::Platform(format!("creating {}: {e}", dir.display())))?;
        }
        std::fs::write(&file, out.text).map_err(|e| AppError::Platform(format!("writing {}: {e}", file.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_tcp_ports_only() {
        assert_eq!(
            parse_ports("0.0.0.0:38080->8080/tcp, [::]:38080->8080/tcp, 3306/tcp, 33060/tcp"),
            vec![PublishedPort { host_port: 38080, container_port: 8080 }]
        );
        assert_eq!(parse_ports("80/tcp"), vec![]);
        assert_eq!(parse_ports("127.0.0.1:5000->5000/tcp, 0.0.0.0:53->53/udp"), vec![PublishedPort { host_port: 5000, container_port: 5000 }]);
        assert_eq!(
            parse_ports("0.0.0.0:8000-8001->9000-9001/tcp"),
            vec![PublishedPort { host_port: 8000, container_port: 9000 }, PublishedPort { host_port: 8001, container_port: 9001 }]
        );
        assert_eq!(parse_ports(""), vec![]);
        assert_eq!(
            parse_port_bindings(r#"{"6379/tcp":[{"HostIp":"127.0.0.1","HostPort":"56999"}],"7000/tcp":[{"HostIp":"","HostPort":""}],"53/udp":[{"HostPort":"53"}],"9/tcp":null}"#),
            vec![PublishedPort { host_port: 56999, container_port: 6379 }]
        );
        assert_eq!(parse_port_bindings("{}"), vec![]);
    }

    #[test]
    fn containers_with_their_compose_service() {
        let text = r#"{"Command":"x","Image":"example/reportserver:1.0","Labels":"com.docker.compose.project=reports,com.docker.compose.project.config_files=C:\\projects\\reports\\docker-compose.yml,com.docker.compose.project.working_dir=C:\\projects\\reports,com.docker.compose.service=reportserver","Names":"reportserver","Ports":"0.0.0.0:38080->8080/tcp, [::]:38080->8080/tcp","State":"running","Status":"Up 2 days"}
{"Image":"example/ui:1.0","Labels":"","Names":"uiapp","Ports":"80/tcp","State":"running","Status":"Up 6 minutes"}
{"Image":"postgres:13","Labels":"","Names":"shop_db","Ports":"","State":"exited","Status":"Exited (255) 2 weeks ago"}
not json"#;
        let list = parse_ps(text);
        assert_eq!(list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["reportserver", "uiapp", "shop_db"]);
        let compose = list[0].compose.as_ref().unwrap();
        assert_eq!((compose.project.as_str(), compose.service.as_str()), ("reports", "reportserver"));
        assert_eq!(compose.dir.as_deref(), Some(r"C:\projects\reports"));
        assert_eq!(compose.files, vec![r"C:\projects\reports\docker-compose.yml".to_string()]);
        assert_eq!(list[0].ports, vec![PublishedPort { host_port: 38080, container_port: 8080 }]);
        assert!(list[1].ports.is_empty() && list[1].compose.is_none());

        // Two compose files: one label, both files.
        let labels = parse_labels(r"com.docker.compose.project=e,com.docker.compose.project.config_files=C:\a\docker-compose.yml,C:\a\docker-compose.local.yml,com.docker.compose.service=web");
        let c = compose_of(&labels).unwrap();
        assert_eq!(c.files, vec![r"C:\a\docker-compose.yml".to_string(), r"C:\a\docker-compose.local.yml".to_string()]);
        assert_eq!(c.service, "web");
    }

    #[test]
    fn compose_files_must_be_yaml_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("shop");
        std::fs::create_dir_all(&project).unwrap();
        let inside = project.join("docker-compose.yml");
        let outside = dir.path().join("evil.yml");
        let script = project.join("run.ps1");
        for f in [&inside, &outside, &script] {
            std::fs::write(f, "services: {}").unwrap();
        }
        let s = |p: &Path| p.display().to_string();
        assert!(usable_compose_file(&project, &s(&inside)));
        assert!(!usable_compose_file(&project, &s(&outside)), "outside the project folder");
        assert!(!usable_compose_file(&project, &s(&project.join("..").join("evil.yml"))), "climbing out");
        assert!(!usable_compose_file(&project, &s(&script)), "not YAML");
        assert!(!usable_compose_file(&project, "docker-compose.yml"), "relative");
        let c = |files: Vec<String>| ComposeService { project: "shop".into(), service: "web".into(), dir: Some(s(&project)), files };
        assert!(compose_args(&c(vec![s(&inside)])).is_some());
        assert!(compose_args(&c(vec![s(&inside), s(&outside)])).is_none(), "one bad file: not compose at all");
        assert!(compose_args(&ComposeService { dir: None, ..c(vec![s(&inside)]) }).is_none());
    }

    #[test]
    fn names_are_checked() {
        let ok = DockerBinding { container: "web-shop".into(), compose: None, autostart: true };
        assert!(validate(&ok).is_ok());
        for bad in ["", "-x", "a b", "a;calc", "--help"] {
            assert!(validate(&DockerBinding { container: bad.into(), ..ok.clone() }).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_stopped_engine_is_recognised() {
        assert!(engine_down("error during connect: Get \"http://%2F%2F.%2Fpipe%2FdockerDesktopLinuxEngine/v1.51/version\": open //./pipe/dockerDesktopLinuxEngine: The system cannot find the file specified."));
        assert!(!engine_down("Error response from daemon: No such container: x"));
    }
}
