//! PHP execution for local sites.
//!
//! Two backends, chosen per site:
//! - **PHP-CGI pools**: for every PHP installation in use, Zenv runs a few
//!   `php-cgi.exe -b 127.0.0.1:<port>` workers (Windows has no PHP-FPM) and
//!   restarts any that exit. The front web server reaches them over FastCGI, so
//!   every site can use its own PHP version.
//! - **FrankenPHP**: each FrankenPHP build embeds exactly one PHP version. A site
//!   whose PHP version matches an enabled FrankenPHP installation is served by
//!   it (engine "auto"); the front web server proxies to FrankenPHP's internal
//!   port, so one HTTP entry point serves every site.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

use crate::config_generator::managed_files::{self, Owner};
use crate::config_generator::{self, safe_value};
use crate::error::{AppError, AppResult};
use crate::local_sites::PhpEngine;
use crate::log_manager;
use crate::paths::{to_config_path, AppPaths};
use crate::port_manager;
use crate::process_manager::{ProcessManager, ProcessSpec};
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, probe, Installation};
use crate::storage::Db;

/// Workers per PHP version. php-cgi on Windows serves one request at a time.
pub const POOL_SIZE: usize = 4;
const MAX_REQUESTS_PER_WORKER: &str = "10000";
const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// Where a PHP site's requests are executed.
#[derive(Debug, Clone)]
pub enum Backend {
    Fastcgi { php: Installation },
    Frankenphp { franken: Installation },
}

/// `8.4.21` → `(8, 4)`.
pub fn minor(version: &str) -> Option<(u32, u32)> {
    let mut it = version.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??))
}

/// PHP version embedded in a FrankenPHP build, from `frankenphp version`
/// ("FrankenPHP 1.12.2 PHP 8.5.6 Caddy v2.11.2 …").
pub fn embedded_php(franken: &Installation) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\bPHP (\d+\.\d+\.\d+)").expect("valid regex"));
    re.captures(&franken.version_detail).map(|c| c[1].to_string())
}

/// FrankenPHP installations that can run `php`'s PHP version, best first.
pub fn matching_frankens<'a>(php: &Installation, frankens: &'a [Installation]) -> Vec<&'a Installation> {
    let want = minor(&php.version);
    let mut out: Vec<&Installation> = frankens
        .iter()
        .filter(|f| f.enabled && f.executable_exists && want.is_some() && embedded_php(f).and_then(|v| minor(&v)) == want)
        .collect();
    out.sort_by_key(|f| !f.is_default);
    out
}

pub fn select(engine: PhpEngine, php: &Installation, frankens: &[Installation]) -> AppResult<Backend> {
    let franken = matching_frankens(php, frankens).first().map(|f| (*f).clone());
    match (engine, franken) {
        (PhpEngine::Fastcgi, _) | (PhpEngine::Auto, None) => Ok(Backend::Fastcgi { php: php.clone() }),
        (PhpEngine::Auto | PhpEngine::Frankenphp, Some(franken)) => Ok(Backend::Frankenphp { franken }),
        (PhpEngine::Frankenphp, None) => Err(AppError::Validation(format!(
            "no enabled FrankenPHP installation embeds PHP {}; register one or choose PHP-CGI",
            php.version
        ))),
    }
}

/// A site served by a FrankenPHP instance.
#[derive(Debug, Clone, Serialize)]
pub struct FrankenSite {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub document_root: String,
    /// Opened on the tailnet (routed by the X-Zenv-Site header).
    pub tailnet: bool,
}

#[derive(Serialize)]
struct CaddyConfig<'a> {
    version: &'a str,
    install_id: &'a str,
    admin_port: u16,
    http_port: u16,
    log_file: String,
    sites: &'a [FrankenSite],
    tailnet_sites: Vec<&'a FrankenSite>,
    tailnet_peer: &'static str,
}

struct Pool {
    install: Installation,
    ports: Vec<u16>,
}

struct Franken {
    http_port: u16,
    admin_port: u16,
    config: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProcess {
    pub kind: &'static str,
    pub installation_id: String,
    pub version: String,
    pub ports: Vec<u16>,
    pub running: usize,
}

pub struct PhpRuntimes {
    db: Arc<Db>,
    paths: AppPaths,
    processes: ProcessManager,
    pools: Mutex<HashMap<String, Pool>>,
    frankens: Mutex<HashMap<String, Franken>>,
}

/// Folder holding the logs of one installation's PHP processes.
pub fn runtime_logs_dir(paths: &AppPaths, install: &Installation) -> PathBuf {
    paths.logs_dir.join("runtimes").join(format!("{}-{}", install.component, install.id))
}

/// Runtime logs of one installation, grouped for the Logs page.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLogGroup {
    pub installation_id: String,
    pub label: String,
    pub sources: Vec<crate::service_manager::LogSource>,
}

/// `*.log` files directly inside `dir`, sorted, with readable labels.
fn runtime_log_files(dir: &Path) -> Vec<crate::service_manager::LogSource> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("log")))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let label = match id.strip_prefix("php-cgi-").and_then(|n| n.parse::<usize>().ok()) {
                Some(n) => format!("Worker {}", n + 1),
                None if id == "frankenphp" => "Server log".to_string(),
                None if id == "frankenphp-process" => "Process output".to_string(),
                None => id.clone(),
            };
            let size_bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
            crate::service_manager::LogSource { id, label, exists: true, size_bytes, path }
        })
        .collect()
}

fn worker_key(install_id: &str, i: usize) -> String {
    format!("php:{install_id}:{i}")
}

fn franken_key(install_id: &str) -> String {
    format!("frankenphp:{install_id}")
}

fn free_ports(n: usize) -> AppResult<Vec<u16>> {
    // Hold every listener until all ports are chosen so none repeats.
    let listeners: Vec<std::net::TcpListener> = (0..n)
        .map(|_| std::net::TcpListener::bind("127.0.0.1:0"))
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Platform(format!("cannot allocate local ports: {e}")))?;
    listeners
        .iter()
        .map(|l| l.local_addr().map(|a| a.port()).map_err(|e| AppError::Platform(e.to_string())))
        .collect()
}

fn wait_listening(port: u16, alive: impl Fn() -> bool) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        if !alive() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

impl PhpRuntimes {
    pub fn new(db: Arc<Db>, paths: AppPaths) -> Self {
        PhpRuntimes {
            db,
            paths,
            processes: ProcessManager::new(),
            pools: Mutex::default(),
            frankens: Mutex::default(),
        }
    }

    fn runtime_logs(&self, install: &Installation) -> PathBuf {
        runtime_logs_dir(&self.paths, install)
    }

    /// Log files of every PHP and FrankenPHP installation that has written any.
    pub fn log_groups(&self) -> AppResult<Vec<RuntimeLogGroup>> {
        let mut groups = Vec::new();
        for install in runtime_manager::list(&self.db, None)? {
            if !matches!(install.component, ComponentId::Php | ComponentId::Frankenphp) {
                continue;
            }
            let sources = runtime_log_files(&runtime_logs_dir(&self.paths, &install));
            if sources.is_empty() {
                continue;
            }
            let label = match install.component {
                ComponentId::Frankenphp => format!("FrankenPHP {}", install.version),
                _ => format!("PHP {} workers", install.version),
            };
            groups.push(RuntimeLogGroup { installation_id: install.id.clone(), label, sources });
        }
        Ok(groups)
    }

    /// Reads one of an installation's runtime log files (listed by `log_groups`).
    pub fn read_log(&self, installation_id: &str, log_id: &str, lines: usize) -> AppResult<log_manager::LogTail> {
        let install = runtime_manager::get(&self.db, installation_id)?;
        let file = runtime_log_files(&runtime_logs_dir(&self.paths, &install))
            .into_iter()
            .find(|s| s.id == log_id)
            .ok_or_else(|| AppError::NotFound { entity: "log", id: log_id.to_string() })?;
        log_manager::tail(&file.path, lines)
    }

    // ----- PHP-CGI pools ----------------------------------------------------

    fn worker_spec(&self, install: &Installation, port: u16, i: usize) -> AppResult<ProcessSpec> {
        let cgi = install.root_path.join("php-cgi.exe");
        if !cgi.is_file() {
            return Err(AppError::Unsupported(format!(
                "PHP {} at {} has no php-cgi.exe, so the web server cannot run it",
                install.version,
                install.root_path.display()
            )));
        }
        let mut args: Vec<OsString> = vec!["-b".into(), format!("127.0.0.1:{port}").into()];
        let ini = install.root_path.join("php.ini");
        if ini.is_file() {
            args.push("-c".into());
            args.push(ini.into_os_string());
        }
        Ok(ProcessSpec {
            program: cgi,
            args,
            cwd: install.root_path.clone(),
            env: vec![("PHP_FCGI_MAX_REQUESTS".into(), MAX_REQUESTS_PER_WORKER.into())],
            log_file: self.runtime_logs(install).join(format!("php-cgi-{i}.log")),
        })
    }

    fn spawn_worker(&self, install: &Installation, port: u16, i: usize) -> AppResult<()> {
        let key = worker_key(&install.id, i);
        let spec = self.worker_spec(install, port, i)?;
        self.processes.spawn(&key, &spec)?;
        if !wait_listening(port, || self.processes.is_running(&key)) {
            let _ = self.processes.stop(&key, None, Duration::ZERO);
            return Err(AppError::StartFailed {
                message: format!("php-cgi {} did not start listening on 127.0.0.1:{port}", install.version),
                log_tail: log_manager::diagnostic_tail(&[spec.log_file], 15),
            });
        }
        Ok(())
    }

    /// Starts (or repairs) the worker pool for a PHP installation; returns its ports.
    pub fn ensure_pool(&self, install: &Installation) -> AppResult<Vec<u16>> {
        let mut pools = self.pools.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pool) = pools.get(&install.id) {
            for (i, port) in pool.ports.iter().enumerate() {
                if !self.processes.is_running(&worker_key(&install.id, i)) {
                    self.spawn_worker(&pool.install, *port, i)?;
                }
            }
            return Ok(pool.ports.clone());
        }
        let ports = free_ports(POOL_SIZE)?;
        for (i, port) in ports.iter().enumerate() {
            if let Err(e) = self.spawn_worker(install, *port, i) {
                for j in 0..i {
                    let _ = self.processes.stop(&worker_key(&install.id, j), None, Duration::ZERO);
                }
                return Err(e);
            }
        }
        tracing::info!(php = %install.version, ?ports, "php-cgi pool started");
        pools.insert(install.id.clone(), Pool { install: install.clone(), ports: ports.clone() });
        Ok(ports)
    }

    /// Restarts pool workers that exited (php-cgi quits after a request quota
    /// or on fatal errors). Called periodically.
    pub fn supervise(&self) {
        let pools = self.pools.lock().unwrap_or_else(|p| p.into_inner());
        for pool in pools.values() {
            for (i, port) in pool.ports.iter().enumerate() {
                if !self.processes.is_running(&worker_key(&pool.install.id, i)) {
                    match self.spawn_worker(&pool.install, *port, i) {
                        Ok(()) => tracing::info!(php = %pool.install.version, port, "php-cgi worker restarted"),
                        Err(e) => tracing::warn!(php = %pool.install.version, port, error = %e, "php-cgi worker restart failed"),
                    }
                }
            }
        }
    }

    fn stop_pool(&self, install_id: &str, pool: &Pool) {
        for i in 0..pool.ports.len() {
            let _ = self.processes.stop(&worker_key(install_id, i), None, Duration::ZERO);
        }
        tracing::info!(php = %pool.install.version, "php-cgi pool stopped");
    }

    // ----- FrankenPHP -------------------------------------------------------

    fn render_caddyfile(&self, install: &Installation, f: &Franken, sites: &[FrankenSite]) -> AppResult<String> {
        for s in sites {
            safe_value("domain", &s.domain)?;
            safe_value("document root", &s.document_root)?;
            safe_value("site name", &s.name)?;
        }
        let log_file = to_config_path(&self.runtime_logs(install).join("frankenphp.log"));
        safe_value("log file", &log_file)?;
        config_generator::render(
            config_generator::FRANKENPHP_CADDYFILE,
            &CaddyConfig {
                version: &install.version,
                install_id: &install.id,
                admin_port: f.admin_port,
                http_port: f.http_port,
                log_file,
                sites,
                tailnet_sites: sites.iter().filter(|s| s.tailnet).collect(),
                tailnet_peer: crate::tailnet::PEER,
            },
        )
    }

    fn franken_cmd(&self, install: &Installation, args: &[&str]) -> AppResult<probe::CapturedOutput> {
        probe::run_capture(&install.executable_path, args, Duration::from_secs(30))
    }

    /// Serves `sites` with a FrankenPHP installation: validates the generated
    /// Caddyfile, then reloads the running instance or starts it. Returns the
    /// internal HTTP port the front web server proxies to.
    pub fn ensure_franken(&self, install: &Installation, sites: &[FrankenSite]) -> AppResult<u16> {
        let mut frankens = self.frankens.lock().unwrap_or_else(|p| p.into_inner());
        if !frankens.contains_key(&install.id) {
            let ports = free_ports(2)?;
            let config = self.paths.generated_dir.join("runtimes").join(format!("frankenphp-{}", install.id)).join("Caddyfile");
            frankens.insert(install.id.clone(), Franken { http_port: ports[0], admin_port: ports[1], config });
        }
        let f = frankens.get(&install.id).expect("inserted above");
        std::fs::create_dir_all(self.runtime_logs(install)).ok();

        let text = self.render_caddyfile(install, f, sites)?;
        let staged = f.config.with_extension("staged");
        let owner = Owner { kind: "runtime", id: &install.id };
        managed_files::write_managed(&self.db, &self.paths.backups_dir, &staged, &text, owner)?;
        let staged_s = staged.display().to_string();
        let check = self.franken_cmd(install, &["validate", "--config", &staged_s, "--adapter", "caddyfile"])?;
        if check.code != Some(0) {
            return Err(AppError::ConfigInvalid {
                message: "FrankenPHP rejected the generated configuration".into(),
                output: check.text.trim().to_string(),
            });
        }
        let changed = managed_files::write_managed(&self.db, &self.paths.backups_dir, &f.config, &text, owner)?.changed;
        let key = franken_key(&install.id);
        let config_s = f.config.display().to_string();

        if self.processes.is_running(&key) {
            if changed {
                let admin = format!("127.0.0.1:{}", f.admin_port);
                let out = self.franken_cmd(install, &["reload", "--config", &config_s, "--adapter", "caddyfile", "--address", &admin])?;
                if out.code != Some(0) {
                    return Err(AppError::StartFailed {
                        message: "FrankenPHP did not accept the new configuration".into(),
                        log_tail: out.text.lines().map(str::to_string).collect(),
                    });
                }
            }
            return Ok(f.http_port);
        }

        let log_file = self.runtime_logs(install).join("frankenphp-process.log");
        let spec = ProcessSpec {
            program: install.executable_path.clone(),
            args: vec!["run".into(), "--config".into(), f.config.clone().into(), "--adapter".into(), "caddyfile".into()],
            cwd: install.root_path.clone(),
            env: vec![],
            log_file: log_file.clone(),
        };
        self.processes.spawn(&key, &spec)?;
        if !wait_listening(f.http_port, || self.processes.is_running(&key)) {
            let _ = self.processes.stop(&key, None, Duration::ZERO);
            return Err(AppError::StartFailed {
                message: format!("FrankenPHP {} did not start", install.version),
                log_tail: log_manager::diagnostic_tail(&[log_file, self.runtime_logs(install).join("frankenphp.log")], 15),
            });
        }
        tracing::info!(version = %install.version, port = f.http_port, "FrankenPHP started");
        Ok(f.http_port)
    }

    fn stop_franken(&self, install_id: &str, f: &Franken) {
        let key = franken_key(install_id);
        let admin = format!("127.0.0.1:{}", f.admin_port);
        let exe = runtime_manager::get(&self.db, install_id).map(|i| i.executable_path).ok();
        let graceful = |_pid: u32| -> AppResult<()> {
            let exe = exe.clone().ok_or_else(|| AppError::Unsupported("installation gone".into()))?;
            let out = probe::run_capture(&exe, &["stop", "--address", admin.as_str()], Duration::from_secs(10))?;
            if out.code == Some(0) { Ok(()) } else { Err(AppError::Platform(out.text)) }
        };
        let _ = self.processes.stop(&key, Some(&graceful), Duration::from_secs(5));
    }

    // ----- lifecycle --------------------------------------------------------

    /// Stops pools and FrankenPHP instances no enabled site needs any more.
    pub fn retain(&self, needed_php: &HashSet<String>, needed_franken: &HashSet<String>) {
        let mut pools = self.pools.lock().unwrap_or_else(|p| p.into_inner());
        let stale: Vec<String> = pools.keys().filter(|k| !needed_php.contains(*k)).cloned().collect();
        for id in stale {
            if let Some(pool) = pools.remove(&id) {
                self.stop_pool(&id, &pool);
            }
        }
        drop(pools);
        let mut frankens = self.frankens.lock().unwrap_or_else(|p| p.into_inner());
        let stale: Vec<String> = frankens.keys().filter(|k| !needed_franken.contains(*k)).cloned().collect();
        for id in stale {
            if let Some(f) = frankens.remove(&id) {
                self.stop_franken(&id, &f);
            }
        }
    }

    pub fn shutdown_all(&self) {
        self.retain(&HashSet::new(), &HashSet::new());
    }

    /// Running pool/instance summary for the UI and diagnostics.
    pub fn status(&self) -> Vec<RuntimeProcess> {
        let mut out = Vec::new();
        for (id, pool) in self.pools.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            let running = (0..pool.ports.len()).filter(|i| self.processes.is_running(&worker_key(id, *i))).count();
            out.push(RuntimeProcess { kind: "php-cgi", installation_id: id.clone(), version: pool.install.version.clone(), ports: pool.ports.clone(), running });
        }
        for (id, f) in self.frankens.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            let running = usize::from(self.processes.is_running(&franken_key(id)));
            let version = runtime_manager::get(&self.db, id).map(|i| i.version).unwrap_or_default();
            out.push(RuntimeProcess { kind: "frankenphp", installation_id: id.clone(), version, ports: vec![f.http_port], running });
        }
        out
    }

    pub fn php_ports(&self, install_id: &str) -> Option<Vec<u16>> {
        self.pools.lock().unwrap_or_else(|p| p.into_inner()).get(install_id).map(|p| p.ports.clone())
    }

    pub fn franken_port(&self, install_id: &str) -> Option<u16> {
        self.frankens.lock().unwrap_or_else(|p| p.into_inner()).get(install_id).map(|f| f.http_port)
    }
}

/// Enabled FrankenPHP installations (candidates for engine "auto").
pub fn frankenphp_installations(db: &Db) -> AppResult<Vec<Installation>> {
    runtime_manager::list(db, Some(ComponentId::Frankenphp))
}

pub fn port_in_use(port: u16) -> bool {
    !matches!(
        port_manager::inspect(port, "127.0.0.1".parse().expect("ip")).map(|i| i.status),
        Ok(port_manager::PortStatus::Free)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_manager::InstallSource;

    fn inst(component: ComponentId, version: &str, detail: &str) -> Installation {
        Installation {
            id: format!("{component}-{version}"),
            component,
            version: version.into(),
            version_detail: detail.into(),
            root_path: PathBuf::from("C:/x"),
            executable_path: PathBuf::from("C:/x/x.exe"),
            source: InstallSource::External,
            is_default: false,
            enabled: true,
            executable_exists: true,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn franken(php: &str) -> Installation {
        inst(ComponentId::Frankenphp, "1.12.2", &format!("FrankenPHP 1.12.2 PHP {php} Caddy v2.11.2 h1:abc"))
    }

    #[test]
    fn parses_versions() {
        assert_eq!(minor("8.4.21"), Some((8, 4)));
        assert_eq!(minor("8"), None);
        assert_eq!(embedded_php(&franken("8.5.6")).as_deref(), Some("8.5.6"));
    }

    #[test]
    fn auto_prefers_frankenphp_only_for_its_own_php_minor() {
        let php85 = inst(ComponentId::Php, "8.5.6", "");
        let php84 = inst(ComponentId::Php, "8.4.21", "");
        let frankens = vec![franken("8.5.6")];
        assert!(matches!(select(PhpEngine::Auto, &php85, &frankens).unwrap(), Backend::Frankenphp { .. }));
        assert!(matches!(select(PhpEngine::Auto, &php84, &frankens).unwrap(), Backend::Fastcgi { .. }));
        assert!(matches!(select(PhpEngine::Fastcgi, &php85, &frankens).unwrap(), Backend::Fastcgi { .. }));
        let err = select(PhpEngine::Frankenphp, &php84, &frankens).unwrap_err();
        assert!(err.to_string().contains("PHP 8.4.21"), "{err}");
    }

    #[test]
    fn disabled_or_missing_frankenphp_is_ignored() {
        let php85 = inst(ComponentId::Php, "8.5.1", "");
        let mut off = franken("8.5.6");
        off.enabled = false;
        let mut gone = franken("8.5.6");
        gone.executable_exists = false;
        assert!(matches!(select(PhpEngine::Auto, &php85, &[off, gone]).unwrap(), Backend::Fastcgi { .. }));
    }

    #[test]
    fn caddyfile_lists_each_site_on_the_internal_port() {
        let dir = tempfile::tempdir().unwrap();
        let rt = PhpRuntimes::new(Arc::new(Db::open_in_memory().unwrap()), AppPaths::new(dir.path()));
        let f = Franken { http_port: 19300, admin_port: 19301, config: dir.path().join("Caddyfile") };
        let sites = vec![FrankenSite { id: "s1".into(), name: "Shop".into(), domain: "shop.test".into(), document_root: "C:/p/shop/public".into(), tailnet: false }];
        let text = rt.render_caddyfile(&franken("8.5.6"), &f, &sites).unwrap();
        assert!(text.contains("admin 127.0.0.1:19301"));
        assert!(text.contains("default_bind 127.0.0.1"));
        assert!(text.contains("http://shop.test:19300 {"));
        assert!(text.contains("root * \"C:/p/shop/public\""));
        assert!(text.contains("php_server"));
        let bad = vec![FrankenSite { document_root: "C:/x\"\n}".into(), ..sites[0].clone() }];
        assert!(rt.render_caddyfile(&franken("8.5.6"), &f, &bad).is_err());
    }

    #[test]
    fn free_ports_are_distinct() {
        let p = free_ports(4).unwrap();
        assert_eq!(p.iter().collect::<HashSet<_>>().len(), 4);
    }
}
