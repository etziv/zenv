//! Redis. There is no official Windows build: users register one they chose
//! (Memurai is the commercial partner; community MSYS2/Cygwin builds exist).
//!
//! Verified behaviour: Redis 5.0.14 (native port) and Redis 8.8.0 (MSYS2 build)
//! both start from a config file, answer PING and save on SHUTDOWN. MSYS2 and
//! Cygwin builds resolve `C:/...` as a relative path, so Zenv hands them
//! `/cygdrive/c/...` paths.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::config_generator::{self, safe_value};
use crate::error::{AppError, AppResult};
use crate::paths::to_config_path;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::Installation;
use crate::service_manager::connect_address;
use crate::web_server::{
    ConnectionInfo, Control, DataDirInfo, DataDirState, LogFile, ServerContext, ServiceAdapter, SiteModel,
    ValidationReport,
};

pub struct RedisAdapter;

#[derive(Serialize)]
struct RedisConf<'a> {
    service_id: &'a str,
    service_name: &'a str,
    port: u16,
    bind: String,
    data_dir: String,
    log_file: String,
}

/// MSYS2/Cygwin builds ship their POSIX runtime next to the executable.
pub fn is_posix_build(install: &Installation) -> bool {
    let dir = install.executable_path.parent().unwrap_or(&install.root_path);
    ["msys-2.0.dll", "cygwin1.dll"].iter().any(|dll| dir.join(dll).is_file())
}

/// `C:\Users\x` → `/cygdrive/c/Users/x`.
pub fn posix_path(path: &Path) -> String {
    let s = to_config_path(path);
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        format!("/cygdrive/{}{}", (b[0] as char).to_ascii_lowercase(), &s[2..])
    } else {
        s
    }
}

fn path_for(install: &Installation, field: &str, path: &Path) -> AppResult<String> {
    let s = if is_posix_build(install) { posix_path(path) } else { to_config_path(path) };
    safe_value(field, &s)?;
    Ok(s)
}

fn command(addr: SocketAddr, parts: &[&str]) -> Result<String, String> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut req = format!("*{}\r\n", parts.len());
    for p in parts {
        req.push_str(&format!("${}\r\n{p}\r\n", p.len()));
    }
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 256];
    let n = s.read(&mut buf).unwrap_or(0);
    Ok(String::from_utf8_lossy(&buf[..n]).trim().to_string())
}

impl ServiceAdapter for RedisAdapter {
    fn component(&self) -> ComponentId {
        ComponentId::Redis
    }

    fn main_config_name(&self) -> &'static str {
        "redis.conf"
    }

    fn supports_sites(&self) -> bool {
        false
    }

    fn needs_data_dir(&self) -> bool {
        true
    }

    fn default_port(&self) -> u16 {
        6379
    }

    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        let data = ctx.data_dir.as_deref().ok_or_else(|| AppError::Validation("this service has no data folder".into()))?;
        // Zenv sets no password, and an explicit `bind` turns protected mode
        // off: on another address every device on the network could use it.
        if !ctx.listen_address.is_loopback() {
            return Err(AppError::Validation(format!(
                "Redis in Zenv has no password, so it listens on this computer only: set the address to 127.0.0.1 instead of {}",
                ctx.listen_address
            )));
        }
        config_generator::render(
            config_generator::REDIS_CONF,
            &RedisConf {
                service_id: safe_value("service id", &ctx.service_id)?,
                service_name: safe_value("service name", &ctx.service_name)?,
                port: ctx.port,
                bind: ctx.listen_address.to_string(),
                data_dir: path_for(install, "data folder", data)?,
                log_file: path_for(install, "log file", &ctx.logs_dir.join("redis.log"))?,
            },
        )
    }

    fn render_site(&self, _: &Installation, _: &ServerContext, _: &SiteModel) -> AppResult<String> {
        Err(AppError::Unsupported("Redis does not serve websites".into()))
    }

    fn validate(&self, _install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        Ok(ValidationReport {
            ok: true,
            output: "Redis has no configuration checker; problems are reported when it starts".into(),
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_args(&self, install: &Installation, config: &Path) -> Vec<OsString> {
        let p = if is_posix_build(install) { posix_path(config) } else { config.display().to_string() };
        vec![p.into()]
    }

    /// SHUTDOWN saves the dataset first (save points are configured).
    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()> {
        let addr = SocketAddr::new(connect_address(ctl.ctx.listen_address), ctl.ctx.port);
        match command(addr, &["SHUTDOWN"]) {
            Ok(reply) if reply.starts_with('-') => Err(AppError::Platform(format!("Redis refused SHUTDOWN: {reply}"))),
            Ok(_) => Ok(()),
            Err(e) => Err(AppError::Platform(e)),
        }
    }

    fn request_reload(&self, _ctl: &Control<'_>) -> AppResult<()> {
        Err(AppError::Unsupported("Redis applies configuration changes on restart".into()))
    }

    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile> {
        vec![LogFile { id: "redis", label: "Redis log", path: ctx.logs_dir.join("redis.log") }]
    }

    fn inspect_data_dir(&self, _install: &Installation, dir: &Path) -> DataDirInfo {
        if dir.is_dir() {
            DataDirInfo::new(DataDirState::Initialized, "Redis data folder")
        } else {
            // Redis needs no initialization; Zenv creates the folder on start.
            DataDirInfo::new(DataDirState::Missing, "created on first start")
        }
    }

    fn connection_info(&self, ctx: &ServerContext) -> Option<ConnectionInfo> {
        Some(ConnectionInfo {
            host: "127.0.0.1".into(),
            port: ctx.port,
            user: None,
            password: None,
            database: Some("0".into()),
            url: format!("redis://127.0.0.1:{}", ctx.port),
            note: None,
        })
    }

    fn probe(&self, addr: SocketAddr) -> Result<String, String> {
        match command(addr, &["PING"])? {
            r if r == "+PONG" => Ok("Redis answered PONG".into()),
            r if r.starts_with("-NOAUTH") => Ok("Redis is running (password protected)".into()),
            r => Err(format!("unexpected reply: {r}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_server::tests_support::{ctx, fake_install};

    #[test]
    fn posix_paths_for_msys_builds() {
        assert_eq!(posix_path(Path::new(r"C:\Users\x\redis.conf")), "/cygdrive/c/Users/x/redis.conf");
        assert_eq!(posix_path(Path::new("D:/data")), "/cygdrive/d/data");
    }

    #[test]
    fn config_uses_posix_paths_only_for_posix_builds() {
        let dir = tempfile::tempdir().unwrap();
        let mut install = fake_install(ComponentId::Redis, "8.8.0", dir.path());
        install.executable_path = dir.path().join("redis-server.exe");
        let c = ctx(dir.path(), Some(dir.path().join("data")));
        let native = RedisAdapter.render_main_config(&install, &c).unwrap();
        assert!(native.contains("dir \"") && !native.contains("/cygdrive/"), "{native}");
        std::fs::write(dir.path().join("msys-2.0.dll"), "").unwrap();
        let msys = RedisAdapter.render_main_config(&install, &c).unwrap();
        assert!(msys.contains("dir \"/cygdrive/"), "{msys}");
        assert!(msys.contains("port 8080") && msys.contains("save 900 1"));
    }
}
