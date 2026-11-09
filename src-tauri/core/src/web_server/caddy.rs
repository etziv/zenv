//! Caddy and FrankenPHP as front web servers. FrankenPHP is Caddy with PHP
//! built in, so both take the same Caddyfile; apps on another PHP version go
//! through Zenv's php-cgi workers with `php_fastcgi`, apps on FrankenPHP's own
//! PHP through a FrankenPHP instance, and everything else as static files or a
//! reverse proxy.
//!
//! Control goes through Caddy's admin API on a loopback port derived from the
//! service id: `reload` applies a new Caddyfile without dropping the listener,
//! `stop` shuts it down gracefully (Windows has no signals for this).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::{https_origin, Control, LogFile, ServerContext, ServiceAdapter, SiteKind, SiteModel, ValidationReport};
use crate::config_generator::{self, safe_value};
use crate::error::{AppError, AppResult};
use crate::paths::to_config_path;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{probe, Installation};

pub struct CaddyAdapter {
    component: ComponentId,
}

pub static CADDY: CaddyAdapter = CaddyAdapter { component: ComponentId::Caddy };
pub static FRANKENPHP: CaddyAdapter = CaddyAdapter { component: ComponentId::Frankenphp };

/// Admin API port for a service: stable across renders, reloads and stops.
pub fn admin_port(service_id: &str) -> u16 {
    let mut h: u32 = 2_166_136_261;
    for b in service_id.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
    }
    21_000 + (h % 4_000) as u16
}

fn admin(ctx: &ServerContext) -> String {
    format!("127.0.0.1:{}", admin_port(&ctx.service_id))
}

fn path_value(field: &str, path: &Path) -> AppResult<String> {
    let s = to_config_path(path);
    safe_value(field, &s)?;
    Ok(s)
}

#[derive(Serialize)]
struct MainHttps {
    port: u16,
    cert: String,
    key: String,
}

#[derive(Serialize)]
struct MainConfig<'a> {
    service_id: &'a str,
    service_name: &'a str,
    admin: String,
    bind: String,
    port: u16,
    https: Option<MainHttps>,
    frankenphp: bool,
    logs_dir: String,
    document_root: String,
    sites_dir: String,
}

#[derive(Serialize)]
struct SiteHttps {
    port: u16,
    cert: String,
    key: String,
    redirect: bool,
    origin: String,
}

#[derive(Serialize)]
struct SiteConfig<'a> {
    id: &'a str,
    name: &'a str,
    domain: &'a str,
    port: u16,
    kind: &'static str,
    document_root: String,
    upstream: String,
    php_ports: Vec<u16>,
    logs_dir: String,
    tls: Option<SiteHttps>,
    tailnet_port: Option<u16>,
    /// Accepted host names on the tailnet port: a list (Caddy) and a pattern.
    tailnet_hosts: Vec<String>,
    tailnet_host_re: String,
    tailnet_peer: &'static str,
    tailnet_only: bool,
}

impl CaddyAdapter {
    fn run(&self, install: &Installation, args: &[OsString]) -> AppResult<probe::CapturedOutput> {
        probe::run_capture(&install.executable_path, args, Duration::from_secs(30))
    }

    fn name(&self) -> &'static str {
        if self.component == ComponentId::Frankenphp { "FrankenPHP" } else { "Caddy" }
    }
}

impl ServiceAdapter for CaddyAdapter {
    fn component(&self) -> ComponentId {
        self.component
    }

    fn main_config_name(&self) -> &'static str {
        "Caddyfile"
    }

    fn render_main_config(&self, _install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        let config = MainConfig {
            service_id: safe_value("service id", &ctx.service_id)?,
            service_name: safe_value("service name", &ctx.service_name)?,
            admin: admin(ctx),
            bind: ctx.listen_address.to_string(),
            port: ctx.port,
            https: match &ctx.https {
                Some(h) => Some(MainHttps {
                    port: h.port,
                    cert: path_value("certificate", &h.default_cert)?,
                    key: path_value("certificate key", &h.default_key)?,
                }),
                None => None,
            },
            frankenphp: self.component == ComponentId::Frankenphp,
            logs_dir: path_value("logs directory", &ctx.logs_dir)?,
            document_root: path_value("document root", &ctx.document_root)?,
            sites_dir: path_value("sites directory", &ctx.sites_dir)?,
        };
        config_generator::render(config_generator::CADDY_MAIN, &config)
    }

    fn render_site(&self, _install: &Installation, ctx: &ServerContext, site: &SiteModel) -> AppResult<String> {
        let mut php_ports = Vec::new();
        let (kind, document_root, upstream) = match &site.kind {
            SiteKind::Static { document_root } => ("static", path_value("document root", document_root)?, String::new()),
            SiteKind::ReverseProxy { upstream } => {
                if upstream.port() == 0 {
                    return Err(AppError::Validation(format!("site {} has no upstream port", site.domain)));
                }
                ("reverse_proxy", String::new(), upstream.to_string())
            }
            SiteKind::PhpFastcgi { document_root, ports } => {
                if ports.is_empty() || ports.contains(&0) {
                    return Err(AppError::Validation(format!("site {} has no PHP workers", site.domain)));
                }
                php_ports = ports.clone();
                ("php_fastcgi", path_value("document root", document_root)?, String::new())
            }
        };
        let config = SiteConfig {
            id: safe_value("site id", &site.id)?,
            name: safe_value("site name", &site.name)?,
            domain: safe_value("domain", &site.domain)?,
            port: ctx.port,
            kind,
            document_root,
            upstream,
            php_ports,
            logs_dir: path_value("logs directory", &site.logs_dir)?,
            tls: match (&site.tls, &ctx.https) {
                (Some(t), Some(h)) => Some(SiteHttps {
                    port: h.port,
                    cert: path_value("certificate", &t.cert)?,
                    key: path_value("certificate key", &t.key)?,
                    redirect: t.redirect,
                    origin: https_origin(&site.domain, h.port),
                }),
                (Some(_), None) => {
                    return Err(AppError::Validation(format!("site {} uses HTTPS but its web server has no HTTPS port", site.domain)))
                }
                _ => None,
            },
            tailnet_port: site.tailnet_port,
            tailnet_hosts: site.tailnet_hosts.clone(),
            tailnet_host_re: crate::tailnet::host_pattern(&site.tailnet_hosts),
            tailnet_peer: crate::tailnet::PEER,
            tailnet_only: site.tailnet_only,
        };
        config_generator::render(config_generator::CADDY_SITE, &config)
    }

    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        let out = self.run(install, &["validate".into(), "--config".into(), config.into(), "--adapter".into(), "caddyfile".into()])?;
        Ok(ValidationReport {
            ok: out.code == Some(0),
            output: out.text.trim().to_string(),
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_args(&self, _install: &Installation, config: &Path) -> Vec<OsString> {
        vec!["run".into(), "--config".into(), config.into(), "--adapter".into(), "caddyfile".into()]
    }

    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()> {
        let out = self.run(ctl.install, &["stop".into(), "--address".into(), admin(ctl.ctx).into()])?;
        if out.code == Some(0) {
            Ok(())
        } else {
            Err(AppError::Platform(format!("{} did not stop: {}", self.name(), out.text.trim())))
        }
    }

    fn graceful_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn request_reload(&self, ctl: &Control<'_>) -> AppResult<()> {
        let args: Vec<OsString> = vec![
            "reload".into(),
            "--config".into(),
            ctl.config.into(),
            "--adapter".into(),
            "caddyfile".into(),
            "--address".into(),
            admin(ctl.ctx).into(),
        ];
        let out = self.run(ctl.install, &args)?;
        if out.code == Some(0) {
            Ok(())
        } else {
            Err(AppError::Platform(format!("{} did not accept the new configuration: {}", self.name(), out.text.trim())))
        }
    }

    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile> {
        vec![LogFile {
            id: "caddy",
            label: if self.component == ComponentId::Frankenphp { "FrankenPHP log" } else { "Caddy log" },
            path: ctx.logs_dir.join("caddy.log"),
        }]
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;
    use crate::web_server::tests_support::{ctx, fake_install};

    #[test]
    fn renders_main_and_site_caddyfiles() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(ComponentId::Frankenphp, "1.13.1", &dir.path().join("fp"));
        let c = ctx(dir.path(), None);
        let main = FRANKENPHP.render_main_config(&install, &c).unwrap();
        assert!(main.contains("http_port 8080") && main.contains("default_bind 127.0.0.1") && main.contains("\tfrankenphp\n"));
        assert!(main.contains(&format!("admin 127.0.0.1:{}", admin_port("svc"))));
        assert!(!CADDY.render_main_config(&install, &c).unwrap().contains("\tfrankenphp\n"), "plain Caddy has no PHP");

        let site = |kind| SiteModel { id: "s1".into(), name: "Shop".into(), domain: "shop.test".into(), logs_dir: dir.path().join("logs"), kind, tls: None, tailnet_port: None, tailnet_hosts: vec![], tailnet_only: false };
        let php = FRANKENPHP
            .render_site(&install, &c, &site(SiteKind::PhpFastcgi { document_root: dir.path().join("shop/public"), ports: vec![9001, 9002] }))
            .unwrap();
        assert!(php.contains("php_fastcgi 127.0.0.1:9001 127.0.0.1:9002"), "{php}");
        assert!(php.contains("http://shop.test:8080 {") && php.contains("import zenv_site_s1"));
        let proxy = FRANKENPHP
            .render_site(&install, &c, &site(SiteKind::ReverseProxy { upstream: SocketAddr::from(([127, 0, 0, 1], 8090)) }))
            .unwrap();
        assert!(proxy.contains("reverse_proxy 127.0.0.1:8090"));
        let fixed = admin_port("abc");
        assert_eq!(fixed, admin_port("abc"));
        assert!((21_000..25_000).contains(&fixed));
    }
}
