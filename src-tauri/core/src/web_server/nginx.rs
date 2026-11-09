//! Nginx for Windows (nginx.org "nginx/Windows" builds).
//!
//! Verified behaviour (nginx 1.22.0): `nginx -t -p <prefix> -c <conf>` checks a
//! configuration, and `nginx -s stop|reload -p <prefix> -c <conf>` signals the
//! running master through the pid file named in the config. PHP is served over
//! FastCGI from php-cgi workers (there is no PHP-FPM on Windows).

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

pub struct NginxAdapter;

#[derive(Serialize)]
struct MainConfig<'a> {
    service_id: &'a str,
    service_name: &'a str,
    install_root: String,
    listen: String,
    logs_dir: String,
    temp_dir: String,
    document_root: String,
    sites_dir: String,
    https: Option<MainHttps>,
}

#[derive(Serialize)]
struct MainHttps {
    listen: String,
    cert: String,
    key: String,
}

#[derive(Serialize)]
struct SiteHttps {
    listen: String,
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
    listen: String,
    kind: &'static str,
    install_root: String,
    document_root: String,
    upstream: String,
    upstream_name: String,
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

fn path_value(field: &str, path: &Path) -> AppResult<String> {
    let s = to_config_path(path);
    safe_value(field, &s)?;
    Ok(s)
}

fn listen(ctx: &ServerContext) -> String {
    listen_on(ctx, ctx.port)
}

fn listen_on(ctx: &ServerContext, port: u16) -> String {
    if ctx.listen_address.is_ipv6() {
        format!("[{}]:{}", ctx.listen_address, port)
    } else {
        format!("{}:{}", ctx.listen_address, port)
    }
}

/// `-p` wants the prefix with a trailing slash; Zenv uses the folder holding
/// the generated config.
fn prefix_arg(config: &Path) -> OsString {
    let mut p = config.parent().unwrap_or(config).as_os_str().to_os_string();
    p.push("\\");
    p
}

fn signal(ctl: &Control<'_>, sig: &str) -> AppResult<()> {
    let args: Vec<OsString> = vec!["-s".into(), sig.into(), "-p".into(), prefix_arg(ctl.config), "-c".into(), ctl.config.into()];
    let out = probe::run_capture(&ctl.install.executable_path, &args, Duration::from_secs(15))?;
    if out.code == Some(0) {
        Ok(())
    } else {
        Err(AppError::Platform(format!("nginx -s {sig} failed: {}", out.text.trim())))
    }
}

/// `nginx -t` also binds the listening ports, after reading the whole
/// configuration. While another server holds the port (a stopped Nginx next
/// to a running Apache), the only error is that bind; the files are fine.
fn only_port_busy(output: &str) -> bool {
    let mut emerg = output.lines().filter(|l| l.contains("[emerg]")).peekable();
    emerg.peek().is_some() && emerg.all(|l| l.contains("bind() to") && (l.contains("10013") || l.contains("10048")))
}

impl ServiceAdapter for NginxAdapter {
    fn component(&self) -> ComponentId {
        ComponentId::Nginx
    }

    fn main_config_name(&self) -> &'static str {
        "nginx.conf"
    }

    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        for required in ["mime.types", "fastcgi_params"] {
            if !install.root_path.join("conf").join(required).is_file() {
                return Err(AppError::Validation(format!(
                    "Nginx installation at {} has no conf/{required}",
                    install.root_path.display()
                )));
            }
        }
        let config = MainConfig {
            service_id: safe_value("service id", &ctx.service_id)?,
            service_name: safe_value("service name", &ctx.service_name)?,
            install_root: path_value("installation folder", &install.root_path)?,
            listen: listen(ctx),
            logs_dir: path_value("logs directory", &ctx.logs_dir)?,
            temp_dir: path_value("temp directory", &ctx.temp_dir)?,
            document_root: path_value("document root", &ctx.document_root)?,
            sites_dir: path_value("sites directory", &ctx.sites_dir)?,
            https: match &ctx.https {
                Some(h) => Some(MainHttps {
                    listen: listen_on(ctx, h.port),
                    cert: path_value("certificate", &h.default_cert)?,
                    key: path_value("certificate key", &h.default_key)?,
                }),
                None => None,
            },
        };
        config_generator::render(config_generator::NGINX_MAIN, &config)
    }

    fn render_site(&self, install: &Installation, ctx: &ServerContext, site: &SiteModel) -> AppResult<String> {
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
            listen: listen(ctx),
            kind,
            install_root: path_value("installation folder", &install.root_path)?,
            document_root,
            upstream,
            upstream_name: format!("php_{}", site.id.replace('-', "_")),
            php_ports,
            logs_dir: path_value("logs directory", &site.logs_dir)?,
            tls: match (&site.tls, &ctx.https) {
                (Some(t), Some(h)) => Some(SiteHttps {
                    listen: listen_on(ctx, h.port),
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
        config_generator::render(config_generator::NGINX_SITE, &config)
    }

    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        let args: Vec<OsString> = vec!["-t".into(), "-p".into(), prefix_arg(config), "-c".into(), config.into()];
        let out = probe::run_capture(&install.executable_path, &args, Duration::from_secs(20))?;
        Ok(ValidationReport {
            ok: (out.code == Some(0) && out.text.contains("test is successful")) || only_port_busy(&out.text),
            output: out.text.trim().to_string(),
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_args(&self, _install: &Installation, config: &Path) -> Vec<OsString> {
        vec!["-p".into(), prefix_arg(config), "-c".into(), config.into()]
    }

    /// `-s quit` waits for every keep-alive connection a browser holds open
    /// (verified: >10 s with a browser attached), so Zenv uses the fast
    /// `-s stop`, which is what a local development server wants.
    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()> {
        signal(ctl, "stop")
    }

    /// Even `-s stop` lingers several seconds while a browser keeps
    /// connections open; nginx keeps no state worth waiting for.
    fn graceful_timeout(&self) -> Duration {
        Duration::from_secs(2)
    }

    fn request_reload(&self, ctl: &Control<'_>) -> AppResult<()> {
        signal(ctl, "reload")
    }

    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile> {
        vec![
            LogFile { id: "error", label: "Nginx error log", path: ctx.logs_dir.join("error.log") },
            LogFile { id: "access", label: "Nginx access log", path: ctx.logs_dir.join("access.log") },
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_busy_port_alone_does_not_fail_the_check() {
        let busy = "nginx: [alert] could not open error log file\r\n2026/10/19 [emerg] 1#2: bind() to 127.0.0.1:80 failed (10013: An attempt was made to access a socket in a way forbidden by its access permissions)\r\nnginx: configuration file x test failed";
        assert!(only_port_busy(busy));
        let broken = "2026/10/19 [emerg] 1#2: unknown directive \"lisen\" in x.conf:3\r\nnginx: configuration file x test failed";
        assert!(!only_port_busy(broken));
        assert!(!only_port_busy("nginx: configuration file x test is successful"));
    }
    use crate::runtime_manager::InstallSource;

    fn install(root: &Path) -> Installation {
        std::fs::create_dir_all(root.join("conf")).unwrap();
        std::fs::write(root.join("conf").join("mime.types"), "").unwrap();
        std::fs::write(root.join("conf").join("fastcgi_params"), "").unwrap();
        Installation {
            id: "n1".into(),
            component: ComponentId::Nginx,
            version: "1.22.0".into(),
            version_detail: String::new(),
            root_path: root.to_path_buf(),
            executable_path: root.join("nginx.exe"),
            source: InstallSource::External,
            is_default: true,
            enabled: true,
            executable_exists: false,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn ctx(base: &Path) -> ServerContext {
        ServerContext {
            service_id: "svc".into(),
            service_name: "Nginx".into(),
            listen_address: "127.0.0.1".parse().unwrap(),
            port: 8080,
            document_root: base.join("www"),
            logs_dir: base.join("logs"),
            sites_dir: base.join("sites"),
            temp_dir: base.join("temp"),
            data_dir: None,
            https: None,
        }
    }

    fn site(kind: SiteKind, base: &Path) -> SiteModel {
        SiteModel { id: "ab-cd".into(), name: "Shop".into(), domain: "shop.test".into(), logs_dir: base.join("logs"), kind, tls: None, tailnet_port: None, tailnet_hosts: vec![], tailnet_only: false }
    }

    #[test]
    fn main_config_is_self_contained() {
        let dir = tempfile::tempdir().unwrap();
        let out = NginxAdapter.render_main_config(&install(&dir.path().join("nginx")), &ctx(dir.path())).unwrap();
        assert!(out.contains("listen 127.0.0.1:8080 default_server;"));
        assert!(out.contains("/nginx/conf/mime.types\";"));
        assert!(out.contains("fastcgi_temp_path"));
        assert!(out.contains("include \"") && out.contains("/sites/*.conf\";"));
        assert!(!out.contains("{{"));
        assert_eq!(out, NginxAdapter.render_main_config(&install(&dir.path().join("nginx")), &ctx(dir.path())).unwrap());
    }

    #[test]
    fn php_site_uses_an_upstream_of_workers_and_laravel_try_files() {
        let dir = tempfile::tempdir().unwrap();
        let s = site(SiteKind::PhpFastcgi { document_root: PathBuf::from("C:/p/shop/public"), ports: vec![9001, 9002] }, dir.path());
        let out = NginxAdapter.render_site(&install(&dir.path().join("nginx")), &ctx(dir.path()), &s).unwrap();
        assert!(out.contains("upstream php_ab_cd {"), "{out}");
        assert!(out.contains("server 127.0.0.1:9001;") && out.contains("server 127.0.0.1:9002;"));
        assert!(out.contains("fastcgi_pass php_ab_cd;"));
        assert!(out.contains("try_files $uri $uri/ /index.php?$query_string;"));
        assert!(out.contains("fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;"));
        assert!(out.contains("location ~ /\\.(?!well-known)"));
    }

    #[test]
    fn static_and_proxy_sites() {
        let dir = tempfile::tempdir().unwrap();
        let i = install(&dir.path().join("nginx"));
        let st = NginxAdapter.render_site(&i, &ctx(dir.path()), &site(SiteKind::Static { document_root: PathBuf::from("C:/p/docs") }, dir.path())).unwrap();
        assert!(st.contains("root \"C:/p/docs\";") && st.contains("phtml|phar") && !st.contains("fastcgi_pass"));
        let px = NginxAdapter.render_site(&i, &ctx(dir.path()), &site(SiteKind::ReverseProxy { upstream: "127.0.0.1:3000".parse().unwrap() }, dir.path())).unwrap();
        assert!(px.contains("proxy_pass http://127.0.0.1:3000;") && px.contains("proxy_set_header Host $http_host;"));
        assert!(!px.contains("root "));
    }

    #[test]
    fn missing_fastcgi_params_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let i = install(&dir.path().join("nginx"));
        std::fs::remove_file(i.root_path.join("conf").join("fastcgi_params")).unwrap();
        let err = NginxAdapter.render_main_config(&i, &ctx(dir.path())).unwrap_err();
        assert!(err.to_string().contains("fastcgi_params"));
    }

    #[test]
    fn https_site_listens_with_ssl_and_redirects() {
        let dir = tempfile::tempdir().unwrap();
        let inst = install(&dir.path().join("nginx"));
        let mut c = ctx(dir.path());
        c.https = Some(crate::web_server::HttpsContext {
            port: 443,
            default_cert: dir.path().join("localhost.pem"),
            default_key: dir.path().join("localhost-key.pem"),
        });
        let main = NginxAdapter.render_main_config(&inst, &c).unwrap();
        assert!(main.contains("listen 127.0.0.1:443 ssl default_server;"));
        let mut model = site(SiteKind::Static { document_root: dir.path().join("shop") }, dir.path());
        model.tls = Some(crate::web_server::SiteTls { cert: dir.path().join("shop.pem"), key: dir.path().join("shop-key.pem"), redirect: true });
        let out = NginxAdapter.render_site(&inst, &c, &model).unwrap();
        assert!(out.contains("listen 127.0.0.1:443 ssl;") && out.contains("ssl_certificate "));
        assert!(out.contains("return 301 https://shop.test$request_uri;"), "{out}");
        c.https = None;
        assert!(NginxAdapter.render_site(&inst, &c, &model).is_err(), "TLS site on a server without HTTPS is refused");
    }
}
