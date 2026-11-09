//! MySQL Community Server (dev.mysql.com Windows ZIP builds).
//!
//! Verified behaviour (MySQL 9.7.0): `mysqld --initialize-insecure` creates a
//! data folder with a passwordless root account; `--validate-config` checks a
//! defaults file; the started mysqld is a monitor process that spawns the real
//! server and stops it cleanly when the named event `mysqld<pid>_shutdown` is
//! set (no credentials needed). MariaDB is a different product and is refused
//! at registration.

use std::ffi::OsString;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::{missing_or_empty, version_at_least};
use crate::config_generator::{self, safe_value};
use crate::error::{AppError, AppResult};
use crate::paths::to_config_path;
use crate::platform;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{probe, Installation};
use crate::web_server::{
    ConnectionInfo, Control, DataDirInfo, DataDirState, LogFile, ServerContext, ServiceAdapter, SiteModel,
    ValidationReport,
};

pub struct MysqlAdapter;

#[derive(Serialize)]
struct MyIni<'a> {
    service_id: &'a str,
    service_name: &'a str,
    basedir: String,
    datadir: String,
    port: u16,
    bind: String,
    logs_dir: String,
    modern: bool,
    custom: Option<String>,
}

/// Your own MySQL settings, next to the generated my.ini. Zenv creates it
/// empty (or with settings carried over by an import) and never overwrites it.
pub const CUSTOM_CNF: &str = "my.custom.cnf";

fn path_value(field: &str, p: &Path) -> AppResult<String> {
    let s = to_config_path(p);
    safe_value(field, &s)?;
    Ok(s)
}

fn data_dir<'a>(ctx: &'a ServerContext) -> AppResult<&'a Path> {
    ctx.data_dir.as_deref().ok_or_else(|| AppError::Validation("this service has no data folder".into()))
}

/// Reads the server greeting: protocol 10 followed by the version string.
pub fn read_handshake(stream: &mut impl Read) -> Result<String, String> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).map_err(|e| format!("no greeting: {e}"))?;
    let len = u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize;
    let mut payload = vec![0u8; len.min(512)];
    stream.read_exact(&mut payload).map_err(|e| format!("short greeting: {e}"))?;
    match payload.first() {
        Some(10) => {
            let end = payload[1..].iter().position(|b| *b == 0).map_or(payload.len(), |i| i + 1);
            Ok(format!("MySQL {}", String::from_utf8_lossy(&payload[1..end])))
        }
        Some(0xff) => Err(format!(
            "server refused the connection: {}",
            String::from_utf8_lossy(payload.get(3..).unwrap_or_default())
        )),
        other => Err(format!("unexpected greeting (protocol {other:?})")),
    }
}

impl ServiceAdapter for MysqlAdapter {
    fn component(&self) -> ComponentId {
        ComponentId::Mysql
    }

    fn main_config_name(&self) -> &'static str {
        "my.ini"
    }

    fn supports_sites(&self) -> bool {
        false
    }

    fn needs_data_dir(&self) -> bool {
        true
    }

    fn needs_initialization(&self) -> bool {
        true
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        config_generator::render(
            config_generator::MYSQL_INI,
            &MyIni {
                service_id: safe_value("service id", &ctx.service_id)?,
                service_name: safe_value("service name", &ctx.service_name)?,
                basedir: path_value("installation folder", &install.root_path)?,
                datadir: path_value("data folder", data_dir(ctx)?)?,
                port: ctx.port,
                bind: ctx.listen_address.to_string(),
                logs_dir: path_value("logs folder", &ctx.logs_dir)?,
                modern: version_at_least(&install.version, (8, 0, 16)),
                custom: {
                    let file = ctx.sites_dir.parent().map(|d| d.join(CUSTOM_CNF));
                    match file.filter(|f| f.is_file()) {
                        Some(f) => Some(path_value("custom settings file", &f)?),
                        None => None,
                    }
                },
            },
        )
    }

    fn render_site(&self, _: &Installation, _: &ServerContext, _: &SiteModel) -> AppResult<String> {
        Err(AppError::Unsupported("MySQL does not serve websites".into()))
    }

    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        if !version_at_least(&install.version, (8, 0, 16)) {
            return Ok(ValidationReport {
                ok: true,
                output: format!("MySQL {} has no configuration checker; settings are applied at start", install.version),
                checked_file: config.to_path_buf(),
            });
        }
        let mut arg = OsString::from("--defaults-file=");
        arg.push(config);
        let out = probe::run_capture(&install.executable_path, &[arg, "--validate-config".into()], Duration::from_secs(30))?;
        Ok(ValidationReport {
            ok: out.code == Some(0),
            output: if out.text.trim().is_empty() { "Configuration is valid".into() } else { out.text.trim().to_string() },
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_args(&self, _install: &Installation, config: &Path) -> Vec<OsString> {
        // --defaults-file must be the first argument.
        let mut arg = OsString::from("--defaults-file=");
        arg.push(config);
        vec![arg]
    }

    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()> {
        // MySQL 8+: monitor process event; older servers: MySQLShutdown<pid>.
        platform::signal_named_event(&format!("mysqld{}_shutdown", ctl.pid))
            .or_else(|_| platform::signal_named_event(&format!("MySQLShutdown{}", ctl.pid)))
    }

    fn graceful_timeout(&self) -> Duration {
        // InnoDB flushes on shutdown; give it time before terminating.
        Duration::from_secs(30)
    }

    fn request_reload(&self, _ctl: &Control<'_>) -> AppResult<()> {
        Err(AppError::Unsupported("MySQL applies configuration changes on restart".into()))
    }

    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile> {
        vec![LogFile { id: "error", label: "MySQL error log", path: ctx.logs_dir.join("mysql-error.log") }]
    }

    fn inspect_data_dir(&self, _install: &Installation, dir: &Path) -> DataDirInfo {
        match missing_or_empty(dir) {
            Some(true) => DataDirInfo::new(if dir.exists() { DataDirState::Empty } else { DataDirState::Missing }, "not initialized yet"),
            None => DataDirInfo::new(DataDirState::Foreign, "cannot read the folder"),
            Some(false) => {
                let markers = ["mysql", "ibdata1", "mysql.ibd", "auto.cnf", "#innodb_redo"];
                if markers.iter().any(|m| dir.join(m).exists()) {
                    DataDirInfo::new(DataDirState::Initialized, "MySQL data folder")
                } else {
                    DataDirInfo::new(DataDirState::Foreign, "contains files that are not a MySQL data folder")
                }
            }
        }
    }

    fn initialize_data_dir(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        let dir = data_dir(ctx)?;
        if missing_or_empty(dir) != Some(true) {
            return Err(AppError::Conflict(format!("{} is not empty; Zenv only initializes empty folders", dir.display())));
        }
        let mut basedir = OsString::from("--basedir=");
        basedir.push(&install.root_path);
        let mut datadir = OsString::from("--datadir=");
        datadir.push(dir);
        let args: Vec<OsString> =
            vec!["--no-defaults".into(), basedir, datadir, "--initialize-insecure".into(), "--console".into()];
        let out = probe::run_capture(&install.executable_path, &args, Duration::from_secs(180))?;
        if out.code != Some(0) {
            return Err(AppError::StartFailed {
                message: "MySQL could not initialize the data folder".into(),
                log_tail: out.text.lines().rev().take(15).map(str::to_string).collect::<Vec<_>>().into_iter().rev().collect(),
            });
        }
        Ok(out.text)
    }

    fn connection_info(&self, ctx: &ServerContext) -> Option<ConnectionInfo> {
        Some(ConnectionInfo {
            host: "127.0.0.1".into(),
            port: ctx.port,
            user: Some("root".into()),
            password: Some(String::new()),
            database: None,
            url: format!("mysql://root@127.0.0.1:{}", ctx.port),
            note: Some("A folder initialized by Zenv has a root account without a password, reachable from this computer only.".into()),
        })
    }

    fn probe(&self, addr: SocketAddr) -> Result<String, String> {
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(5))).ok();
        read_handshake(&mut s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_greeting() {
        // Protocol 10, "9.7.0\0", then auth data we ignore.
        let mut payload = vec![10u8];
        payload.extend_from_slice(b"9.7.0\0abcdefgh");
        let mut packet = vec![payload.len() as u8, 0, 0, 0];
        packet.extend(payload);
        assert_eq!(read_handshake(&mut packet.as_slice()).unwrap(), "MySQL 9.7.0");
    }

    #[test]
    fn reports_refusals() {
        let mut payload = vec![0xffu8, 0x6a, 0x04];
        payload.extend_from_slice(b"Host is not allowed");
        let mut packet = vec![payload.len() as u8, 0, 0, 0];
        packet.extend(payload);
        assert!(read_handshake(&mut packet.as_slice()).unwrap_err().contains("Host is not allowed"));
    }

    #[test]
    fn classifies_data_folders() {
        let dir = tempfile::tempdir().unwrap();
        let install = crate::web_server::tests_support::fake_install(ComponentId::Mysql, "9.7.0", dir.path());
        let d = dir.path().join("data");
        assert_eq!(MysqlAdapter.inspect_data_dir(&install, &d).state, DataDirState::Missing);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(MysqlAdapter.inspect_data_dir(&install, &d).state, DataDirState::Empty);
        std::fs::write(d.join("notes.txt"), "").unwrap();
        assert_eq!(MysqlAdapter.inspect_data_dir(&install, &d).state, DataDirState::Foreign);
        std::fs::create_dir(d.join("mysql")).unwrap();
        assert_eq!(MysqlAdapter.inspect_data_dir(&install, &d).state, DataDirState::Initialized);
    }

    #[test]
    fn my_ini_only_uses_modern_options_on_modern_servers() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = crate::web_server::tests_support::ctx(dir.path(), Some(dir.path().join("data")));
        let modern = crate::web_server::tests_support::fake_install(ComponentId::Mysql, "9.7.0", dir.path());
        let text = MysqlAdapter.render_main_config(&modern, &ctx).unwrap();
        assert!(text.contains("upgrade=NONE") && text.contains("mysqlx=OFF") && text.contains("port=8080"));
        let old = crate::web_server::tests_support::fake_install(ComponentId::Mysql, "5.7.44", dir.path());
        let text = MysqlAdapter.render_main_config(&old, &ctx).unwrap();
        assert!(!text.contains("upgrade=") && !text.contains("mysqlx"));
        assert!(!text.contains('\\'), "forward slashes only: {text}");
    }
}
