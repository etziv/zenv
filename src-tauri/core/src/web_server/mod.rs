//! One adapter per web server. Adapters share an interface but never a config
//! format: each renders its own templates and knows its own control protocol.

pub mod apache;
pub mod caddy;
pub mod nginx;

use std::ffi::OsString;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppResult;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::Installation;

/// Everything an adapter needs to render a service's main configuration.
#[derive(Debug, Clone)]
pub struct ServerContext {
    pub service_id: String,
    pub service_name: String,
    pub listen_address: IpAddr,
    pub port: u16,
    pub document_root: PathBuf,
    pub logs_dir: PathBuf,
    pub sites_dir: PathBuf,
    /// Scratch space some servers need (Nginx request-body/proxy buffers).
    pub temp_dir: PathBuf,
    /// Databases and caches: where their data lives.
    pub data_dir: Option<PathBuf>,
    /// Web servers with at least one HTTPS site.
    pub https: Option<HttpsContext>,
}

#[derive(Debug, Clone)]
pub struct HttpsContext {
    pub port: u16,
    /// Certificate of the default HTTPS server (unknown names).
    pub default_cert: PathBuf,
    pub default_key: PathBuf,
}

/// TLS for one site.
#[derive(Debug, Clone)]
pub struct SiteTls {
    pub cert: PathBuf,
    pub key: PathBuf,
    /// Plain HTTP requests are redirected to HTTPS.
    pub redirect: bool,
}

/// `https://domain` or `https://domain:port`.
pub fn https_origin(domain: &str, port: u16) -> String {
    if port == 443 { format!("https://{domain}") } else { format!("https://{domain}:{port}") }
}

/// Address the HTTPS virtual hosts bind to.
pub fn https_vhost_address(ctx: &ServerContext, port: u16) -> String {
    match ctx.listen_address {
        a if a.is_unspecified() => format!("*:{port}"),
        IpAddr::V6(a) => format!("[{a}]:{port}"),
        IpAddr::V4(a) => format!("{a}:{port}"),
    }
}

/// What an adapter needs to signal a running server.
pub struct Control<'a> {
    pub install: &'a Installation,
    pub ctx: &'a ServerContext,
    /// The active generated main configuration.
    pub config: &'a Path,
    pub pid: u32,
}

/// Address a name-based virtual host binds to, matching the service's Listen.
pub fn vhost_address(ctx: &ServerContext) -> String {
    match ctx.listen_address {
        a if a.is_unspecified() => format!("*:{}", ctx.port),
        IpAddr::V6(a) => format!("[{a}]:{}", ctx.port),
        IpAddr::V4(a) => format!("{a}:{}", ctx.port),
    }
}

/// Server-independent description of one site; each adapter serializes it
/// into its own configuration format.
#[derive(Debug, Clone)]
pub struct SiteModel {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub logs_dir: PathBuf,
    pub kind: SiteKind,
    pub tls: Option<SiteTls>,
    /// Loopback port `tailscale serve` passes this app's tailnet visitors to.
    pub tailnet_port: Option<u16>,
    /// The host names accepted on that port (this computer's tailnet names).
    pub tailnet_hosts: Vec<String>,
    /// Render only the tailnet block (an app this server forwards to another
    /// one, served directly for tailnet visitors).
    pub tailnet_only: bool,
}

#[derive(Debug, Clone)]
pub enum SiteKind {
    Static { document_root: PathBuf },
    ReverseProxy { upstream: SocketAddr },
    /// PHP through FastCGI workers (one php-cgi per port).
    PhpFastcgi { document_root: PathBuf, ports: Vec<u16> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataDirState {
    Missing,
    Empty,
    Initialized,
    /// Holds something that is not this server's data; never touched.
    Foreign,
    /// This server's data, but from a version that cannot open it.
    Incompatible,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirInfo {
    pub state: DataDirState,
    pub detail: String,
}

impl DataDirInfo {
    pub fn new(state: DataDirState, detail: impl Into<String>) -> Self {
        DataDirInfo { state, detail: detail.into() }
    }
}

/// How applications connect to a database or cache service.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    /// `Some("")` means "no password"; never a stored secret.
    pub password: Option<String>,
    pub database: Option<String>,
    pub url: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub ok: bool,
    pub output: String,
    pub checked_file: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub id: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
}

/// One adapter per managed service (web servers, databases, caches). Servers
/// share this interface but never a configuration format.
pub trait ServiceAdapter: Send + Sync {
    fn component(&self) -> ComponentId;

    /// Web servers host local sites; databases and caches do not.
    fn supports_sites(&self) -> bool {
        true
    }

    /// Databases and caches keep data in a folder of their own.
    fn needs_data_dir(&self) -> bool {
        false
    }

    /// The data folder must be created by an explicit initialize step.
    fn needs_initialization(&self) -> bool {
        false
    }

    fn default_port(&self) -> u16 {
        80
    }

    fn inspect_data_dir(&self, _install: &Installation, _dir: &Path) -> DataDirInfo {
        DataDirInfo::new(DataDirState::Initialized, "")
    }

    /// Creates a fresh data folder. Only called for missing or empty folders,
    /// and only on an explicit user request.
    fn initialize_data_dir(&self, _install: &Installation, _ctx: &ServerContext) -> AppResult<String> {
        Err(crate::error::AppError::Unsupported("this service has no data folder to initialize".into()))
    }

    fn connection_info(&self, _ctx: &ServerContext) -> Option<ConnectionInfo> {
        None
    }

    /// Protocol-level check that the service really answers.
    fn probe(&self, _addr: SocketAddr) -> Result<String, String> {
        Ok(String::new())
    }

    /// File name of the generated main configuration.
    fn main_config_name(&self) -> &'static str;

    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String>;

    /// One site's configuration, included from `ctx.sites_dir`.
    fn render_site(&self, install: &Installation, ctx: &ServerContext, site: &SiteModel) -> AppResult<String>;

    /// Runs the server's own configuration checker against `config`.
    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport>;

    fn launch_args(&self, install: &Installation, config: &Path) -> Vec<OsString>;

    /// The program to start (the installation's executable unless it is a
    /// script run by another program, e.g. Node).
    fn launch_program(&self, install: &Installation) -> AppResult<PathBuf> {
        Ok(install.executable_path.clone())
    }

    /// Extra environment for the server process.
    fn launch_env(&self, _install: &Installation, _ctx: &ServerContext) -> AppResult<Vec<(OsString, OsString)>> {
        Ok(vec![])
    }

    /// How long the server may take until it accepts connections.
    fn ready_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(15)
    }

    /// Folder name under `<home>\data` when none is chosen.
    fn default_data_dir_name(&self, service_id: &str) -> String {
        format!("{}-{}", self.component(), &service_id[..8.min(service_id.len())])
    }

    /// Asks the running server to shut down cleanly.
    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()>;

    /// How long to wait for a graceful stop before terminating the process tree.
    fn graceful_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }

    /// Asks the running server to reload configuration without dropping the listener.
    fn request_reload(&self, ctl: &Control<'_>) -> AppResult<()>;

    /// Log files the server writes itself (beyond captured stdout/stderr).
    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile>;
}

pub fn adapter_for(component: ComponentId) -> Option<&'static dyn ServiceAdapter> {
    match component {
        ComponentId::Apache => Some(&apache::ApacheAdapter),
        ComponentId::Nginx => Some(&nginx::NginxAdapter),
        ComponentId::Caddy => Some(&caddy::CADDY),
        ComponentId::Frankenphp => Some(&caddy::FRANKENPHP),
        ComponentId::Mysql => Some(&crate::database_manager::mysql::MysqlAdapter),
        ComponentId::Postgresql => Some(&crate::database_manager::postgres::PostgresAdapter),
        ComponentId::Redis => Some(&crate::cache_manager::redis::RedisAdapter),
        ComponentId::Omniroute => Some(&crate::ai_router::OMNIROUTE_ADAPTER),
        ComponentId::Ninerouter => Some(&crate::ai_router::NINEROUTER_ADAPTER),
        _ => None,
    }
}

#[cfg(test)]
pub mod tests_support {
    use std::path::{Path, PathBuf};

    use super::ServerContext;
    use crate::runtime_catalog::ComponentId;
    use crate::runtime_manager::{InstallSource, Installation};

    pub fn fake_install(component: ComponentId, version: &str, root: &Path) -> Installation {
        Installation {
            id: format!("{component}-{version}"),
            component,
            version: version.into(),
            version_detail: String::new(),
            root_path: root.to_path_buf(),
            executable_path: root.join("bin").join("server.exe"),
            source: InstallSource::External,
            is_default: true,
            enabled: true,
            executable_exists: false,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    pub fn ctx(base: &Path, data_dir: Option<PathBuf>) -> ServerContext {
        ServerContext {
            service_id: "svc".into(),
            service_name: "Service".into(),
            listen_address: "127.0.0.1".parse().unwrap(),
            port: 8080,
            document_root: base.join("www"),
            logs_dir: base.join("logs"),
            sites_dir: base.join("sites"),
            temp_dir: base.join("temp"),
            data_dir,
            https: None,
        }
    }
}
