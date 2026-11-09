//! Apache HTTP Server (Apache Lounge builds, mpm_winnt).
//!
//! Verified behaviour (Apache Lounge 2.4.67 VS18): in console mode,
//! `httpd -k shutdown|restart` fails with AH00436 because it addresses the
//! "Apache2.4" Windows service. The parent process instead listens on the named
//! events `ap<parent-pid>_shutdown` and `ap<parent-pid>_restart`.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::{
    https_origin, https_vhost_address, vhost_address, Control, LogFile, ServerContext, ServiceAdapter, SiteKind, SiteModel,
    ValidationReport,
};
use crate::config_generator::{self, safe_value};
use crate::error::{AppError, AppResult};
use crate::paths::to_config_path;
use crate::platform;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{probe, Installation};

pub struct ApacheAdapter;

/// Modules required by the generated main config, sorted for deterministic output.
const MODULES: &[(&str, &str)] = &[
    ("alias_module", "mod_alias.so"),
    ("authz_core_module", "mod_authz_core.so"),
    ("authz_host_module", "mod_authz_host.so"),
    ("autoindex_module", "mod_autoindex.so"),
    ("dir_module", "mod_dir.so"),
    ("env_module", "mod_env.so"),
    ("headers_module", "mod_headers.so"),
    ("log_config_module", "mod_log_config.so"),
    ("mime_module", "mod_mime.so"),
    ("lbmethod_byrequests_module", "mod_lbmethod_byrequests.so"),
    ("proxy_module", "mod_proxy.so"),
    ("proxy_balancer_module", "mod_proxy_balancer.so"),
    ("proxy_fcgi_module", "mod_proxy_fcgi.so"),
    ("proxy_http_module", "mod_proxy_http.so"),
    ("rewrite_module", "mod_rewrite.so"),
    ("setenvif_module", "mod_setenvif.so"),
    ("slotmem_shm_module", "mod_slotmem_shm.so"),
];

/// Added when the service serves HTTPS.
const TLS_MODULES: &[(&str, &str)] = &[("socache_shmcb_module", "mod_socache_shmcb.so"), ("ssl_module", "mod_ssl.so")];

#[derive(Serialize)]
struct Module {
    name: &'static str,
    file: &'static str,
}

#[derive(Serialize)]
struct MainConfig<'a> {
    service_id: &'a str,
    service_name: &'a str,
    server_root: String,
    listen: String,
    vhost_address: String,
    port: u16,
    modules: Vec<Module>,
    logs_dir: String,
    document_root: String,
    sites_dir: String,
    https: Option<MainHttps>,
}

#[derive(Serialize)]
struct MainHttps {
    listen: String,
    vhost_address: String,
    cert: String,
    key: String,
}

#[derive(Serialize)]
struct SiteHttps {
    vhost_address: String,
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
    vhost_address: String,
    kind: &'static str,
    document_root: String,
    upstream: String,
    php_ports: Vec<u16>,
    websocket_upgrade: bool,
    logs_dir: String,
    tls: Option<SiteHttps>,
    tailnet_port: Option<u16>,
    /// Accepted host names on the tailnet port: a list (Caddy) and a pattern.
    tailnet_hosts: Vec<String>,
    tailnet_host_re: String,
    tailnet_peer: &'static str,
    tailnet_only: bool,
}

/// `ProxyPass ... upgrade=websocket` exists since Apache 2.4.47.
fn supports_websocket_upgrade(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    parts.len() >= 3 && (parts[0], parts[1], parts[2]) >= (2, 4, 47)
}

fn config_path_value(field: &str, path: &Path) -> AppResult<String> {
    let s = to_config_path(path);
    safe_value(field, &s)?;
    Ok(s)
}

impl ServiceAdapter for ApacheAdapter {
    fn component(&self) -> ComponentId {
        ComponentId::Apache
    }

    fn main_config_name(&self) -> &'static str {
        "httpd.conf"
    }

    fn render_main_config(&self, install: &Installation, ctx: &ServerContext) -> AppResult<String> {
        let modules_dir = install.root_path.join("modules");
        let mut modules: Vec<(&'static str, &'static str)> = MODULES.to_vec();
        if ctx.https.is_some() {
            modules.extend_from_slice(TLS_MODULES);
            modules.sort();
        }
        let missing: Vec<&str> =
            modules.iter().filter(|(_, f)| !modules_dir.join(f).is_file()).map(|(_, f)| *f).collect();
        if !missing.is_empty() {
            return Err(AppError::Validation(format!(
                "Apache installation at {} is missing required modules: {}",
                install.root_path.display(),
                missing.join(", ")
            )));
        }
        if !install.root_path.join("conf").join("mime.types").is_file() {
            return Err(AppError::Validation(format!(
                "Apache installation at {} has no conf/mime.types",
                install.root_path.display()
            )));
        }

        let listen = if ctx.listen_address.is_ipv6() {
            format!("[{}]:{}", ctx.listen_address, ctx.port)
        } else {
            format!("{}:{}", ctx.listen_address, ctx.port)
        };
        let config = MainConfig {
            service_id: safe_value("service id", &ctx.service_id)?,
            service_name: safe_value("service name", &ctx.service_name)?,
            server_root: config_path_value("server root", &install.root_path)?,
            listen,
            vhost_address: vhost_address(ctx),
            port: ctx.port,
            modules: modules.iter().map(|(name, file)| Module { name, file }).collect(),
            logs_dir: config_path_value("logs directory", &ctx.logs_dir)?,
            document_root: config_path_value("document root", &ctx.document_root)?,
            sites_dir: config_path_value("sites directory", &ctx.sites_dir)?,
            https: match &ctx.https {
                Some(h) => Some(MainHttps {
                    listen: if ctx.listen_address.is_ipv6() {
                        format!("[{}]:{}", ctx.listen_address, h.port)
                    } else {
                        format!("{}:{}", ctx.listen_address, h.port)
                    },
                    vhost_address: https_vhost_address(ctx, h.port),
                    cert: config_path_value("certificate", &h.default_cert)?,
                    key: config_path_value("certificate key", &h.default_key)?,
                }),
                None => None,
            },
        };
        config_generator::render(config_generator::APACHE_MAIN, &config)
    }

    fn render_site(&self, install: &Installation, ctx: &ServerContext, site: &SiteModel) -> AppResult<String> {
        let mut php_ports = Vec::new();
        let (kind, document_root, upstream) = match &site.kind {
            SiteKind::Static { document_root } => ("static", config_path_value("document root", document_root)?, String::new()),
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
                ("php_fastcgi", config_path_value("document root", document_root)?, String::new())
            }
        };
        let config = SiteConfig {
            id: safe_value("site id", &site.id)?,
            name: safe_value("site name", &site.name)?,
            domain: safe_value("domain", &site.domain)?,
            vhost_address: vhost_address(ctx),
            kind,
            document_root,
            upstream,
            php_ports,
            websocket_upgrade: supports_websocket_upgrade(&install.version),
            logs_dir: config_path_value("logs directory", &site.logs_dir)?,
            tls: match (&site.tls, &ctx.https) {
                (Some(t), Some(h)) => Some(SiteHttps {
                    vhost_address: https_vhost_address(ctx, h.port),
                    cert: config_path_value("certificate", &t.cert)?,
                    key: config_path_value("certificate key", &t.key)?,
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
        config_generator::render(config_generator::APACHE_SITE, &config)
    }

    fn validate(&self, install: &Installation, config: &Path) -> AppResult<ValidationReport> {
        let args: Vec<OsString> =
            vec!["-t".into(), "-d".into(), install.root_path.clone().into(), "-f".into(), config.into()];
        let out = probe::run_capture(&install.executable_path, &args, Duration::from_secs(20))?;
        Ok(ValidationReport {
            ok: out.code == Some(0) && out.text.contains("Syntax OK"),
            output: out.text.trim().to_string(),
            checked_file: config.to_path_buf(),
        })
    }

    fn launch_args(&self, install: &Installation, config: &Path) -> Vec<OsString> {
        vec!["-d".into(), install.root_path.clone().into(), "-f".into(), config.into()]
    }

    fn request_graceful_stop(&self, ctl: &Control<'_>) -> AppResult<()> {
        platform::signal_named_event(&format!("ap{}_shutdown", ctl.pid))
    }

    /// Shutdown waits for a browser's keep-alive connections; logs are
    /// unbuffered, so nothing is lost by terminating after a few seconds.
    fn graceful_timeout(&self) -> Duration {
        Duration::from_secs(4)
    }

    fn request_reload(&self, ctl: &Control<'_>) -> AppResult<()> {
        platform::signal_named_event(&format!("ap{}_restart", ctl.pid))
    }

    fn log_files(&self, ctx: &ServerContext) -> Vec<LogFile> {
        vec![
            LogFile { id: "error", label: "Apache error log", path: ctx.logs_dir.join("error.log") },
            LogFile { id: "access", label: "Apache access log", path: ctx.logs_dir.join("access.log") },
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::runtime_manager::InstallSource;

    fn fake_install(root: &Path) -> Installation {
        for (_, f) in MODULES {
            let p = root.join("modules").join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        std::fs::create_dir_all(root.join("conf")).unwrap();
        std::fs::write(root.join("conf").join("mime.types"), b"").unwrap();
        Installation {
            id: "i1".into(),
            component: ComponentId::Apache,
            version: "2.4.67".into(),
            version_detail: String::new(),
            root_path: root.to_path_buf(),
            executable_path: root.join("bin").join("httpd.exe"),
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
            service_id: "svc-1".into(),
            service_name: "Main Apache".into(),
            listen_address: "127.0.0.1".parse().unwrap(),
            port: 8080,
            document_root: base.join("www").join("svc-1"),
            logs_dir: base.join("logs"),
            sites_dir: base.join("sites"),
            temp_dir: base.join("temp"),
            data_dir: None,
            https: None,
        }
    }

    #[test]
    fn renders_deterministic_forward_slash_config() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let c = ctx(dir.path());
        let a = ApacheAdapter.render_main_config(&install, &c).unwrap();
        let b = ApacheAdapter.render_main_config(&install, &c).unwrap();
        assert_eq!(a, b, "same input, same bytes");
        assert!(a.contains("Listen 127.0.0.1:8080"));
        assert!(a.contains("ServerName localhost:8080"));
        assert!(a.contains("LoadModule rewrite_module modules/mod_rewrite.so"));
        assert!(a.contains("IncludeOptional \""), "sites include present");
        let root = to_config_path(dir.path());
        for line in a.lines().filter(|l| l.contains(&root)) {
            assert!(!line.contains('\\'), "backslash in Apache path: {line}");
        }
        assert!(!a.contains("{{"), "template fully rendered");
    }

    fn site(kind: SiteKind, base: &Path) -> SiteModel {
        SiteModel { id: "s1".into(), name: "Shop".into(), domain: "shop.test".into(), logs_dir: base.join("logs"), kind, tls: None, tailnet_port: None, tailnet_hosts: vec![], tailnet_only: false }
    }

    #[test]
    fn static_site_blocks_php_sources_and_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let out = ApacheAdapter
            .render_site(&install, &ctx(dir.path()), &site(SiteKind::Static { document_root: dir.path().join("shop").join("public") }, dir.path()))
            .unwrap();
        assert!(out.contains("<VirtualHost 127.0.0.1:8080>"));
        assert!(out.contains("ServerName shop.test"));
        assert!(out.contains("/shop/public\""), "{out}");
        assert!(out.contains("php[0-9]?|phtml|phar"));
        assert!(out.contains(r#"<FilesMatch "^\.">"#));
        assert!(!out.contains("ProxyPass"));
    }

    #[test]
    fn reverse_proxy_site_targets_the_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let mut install = fake_install(&dir.path().join("Apache24"));
        let s = site(SiteKind::ReverseProxy { upstream: "127.0.0.1:3000".parse().unwrap() }, dir.path());
        let out = ApacheAdapter.render_site(&install, &ctx(dir.path()), &s).unwrap();
        assert!(out.contains(r#"ProxyPass "/" "http://127.0.0.1:3000/" upgrade=websocket"#), "{out}");
        assert!(out.contains("ProxyPreserveHost On"));
        assert!(!out.contains("DocumentRoot"));
        install.version = "2.4.38".into();
        let old = ApacheAdapter.render_site(&install, &ctx(dir.path()), &s).unwrap();
        assert!(old.contains(r#"ProxyPass "/" "http://127.0.0.1:3000/"
"#), "no upgrade flag before 2.4.47: {old}");
        let zero = site(SiteKind::ReverseProxy { upstream: "127.0.0.1:0".parse().unwrap() }, dir.path());
        assert!(ApacheAdapter.render_site(&install, &ctx(dir.path()), &zero).is_err());
    }

    #[test]
    fn php_site_balances_over_its_workers() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let s = site(SiteKind::PhpFastcgi { document_root: dir.path().join("app").join("public"), ports: vec![19101, 19102] }, dir.path());
        let out = ApacheAdapter.render_site(&install, &ctx(dir.path()), &s).unwrap();
        assert!(out.contains(r#"<Proxy "balancer://php-s1">"#), "{out}");
        assert!(out.contains(r#"BalancerMember "fcgi://127.0.0.1:19101""#));
        assert!(out.contains(r#"BalancerMember "fcgi://127.0.0.1:19102""#));
        assert!(out.contains(r#"SetHandler "proxy:balancer://php-s1/""#));
        assert!(out.contains("DirectoryIndex index.php index.html"));
        assert!(out.contains(r#"<FilesMatch "\.php$">"#));
        let none = site(SiteKind::PhpFastcgi { document_root: dir.path().to_path_buf(), ports: vec![] }, dir.path());
        assert!(ApacheAdapter.render_site(&install, &ctx(dir.path()), &none).is_err());
    }

    #[test]
    fn main_config_has_a_default_vhost_before_site_includes() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let out = ApacheAdapter.render_main_config(&install, &ctx(dir.path())).unwrap();
        let default_vhost = out.find("ServerName localhost\n").expect("default vhost");
        assert!(default_vhost < out.find("IncludeOptional").unwrap());
    }

    #[test]
    fn ipv6_listen_is_bracketed() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let mut c = ctx(dir.path());
        c.listen_address = "::1".parse().unwrap();
        let out = ApacheAdapter.render_main_config(&install, &c).unwrap();
        assert!(out.contains("Listen [::1]:8080"));
    }

    #[test]
    fn missing_modules_are_reported_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Apache24");
        let install = fake_install(&root);
        std::fs::remove_file(root.join("modules").join("mod_rewrite.so")).unwrap();
        let err = ApacheAdapter.render_main_config(&install, &ctx(dir.path())).unwrap_err();
        assert!(err.to_string().contains("mod_rewrite.so"), "{err}");
    }

    #[test]
    fn quote_in_path_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let install = fake_install(&dir.path().join("Apache24"));
        let mut c = ctx(dir.path());
        c.document_root = PathBuf::from("C:/evil\"\nLoadModule x y");
        assert!(matches!(
            ApacheAdapter.render_main_config(&install, &c),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn https_adds_ssl_listener_and_tls_vhost() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Apache24");
        let install = fake_install(&root);
        let mut c = ctx(dir.path());
        c.https = Some(crate::web_server::HttpsContext {
            port: 8443,
            default_cert: dir.path().join("localhost.pem"),
            default_key: dir.path().join("localhost-key.pem"),
        });
        assert!(ApacheAdapter.render_main_config(&install, &c).is_err(), "mod_ssl missing is reported");
        for f in ["mod_ssl.so", "mod_socache_shmcb.so"] {
            std::fs::write(root.join("modules").join(f), b"").unwrap();
        }
        let main = ApacheAdapter.render_main_config(&install, &c).unwrap();
        assert!(main.contains("Listen 127.0.0.1:8443") && main.contains("LoadModule ssl_module modules/mod_ssl.so"));
        assert!(main.contains("SSLSessionCache"));

        let mut model = site(SiteKind::Static { document_root: dir.path().join("shop") }, dir.path());
        model.tls = Some(crate::web_server::SiteTls { cert: dir.path().join("shop.pem"), key: dir.path().join("shop-key.pem"), redirect: true });
        let out = ApacheAdapter.render_site(&install, &c, &model).unwrap();
        assert!(out.contains("<VirtualHost 127.0.0.1:8443>") && out.contains("SSLEngine on"));
        assert!(out.contains("Redirect permanent / \"https://shop.test:8443/\""), "{out}");
        assert_eq!(out.matches("DocumentRoot").count(), 1, "the HTTP vhost only redirects");

        // Without HTTPS the main config stays exactly as before.
        c.https = None;
        assert!(!ApacheAdapter.render_main_config(&install, &c).unwrap().contains("ssl"));
    }
}
