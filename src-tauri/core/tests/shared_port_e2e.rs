//! Apache and FrankenPHP on the same port at once: each serves its own apps.
//! The one started first holds the port and passes the other's apps on; when
//! it stops, the other moves onto the port.
//!
//!   ZENV_TEST_APACHE_ROOT=C:\...\Apache24
//!   ZENV_TEST_PHP=C:\php-8.4                     (folder with php-cgi.exe)
//!   ZENV_TEST_FRANKENPHP_ROOT=C:\...\frankenphp\1.12.2
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, SiteView, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager::{self, Installation};
use zenv_core::service_manager::ServiceInput;

fn get(port: u16, host: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

/// A PHP app; `apache_rule` adds an .htaccess rewrite only Apache applies.
fn project(root: &Path, name: &str, apache_rule: bool) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    std::fs::write(p.join("artisan"), "#!/usr/bin/env php").unwrap();
    std::fs::write(p.join("composer.json"), "{}").unwrap();
    std::fs::write(
        p.join("public").join("index.php"),
        r#"<?php echo "app={$_SERVER['HTTP_HOST']} sapi=" . PHP_SAPI . " from=" . ($_GET['from'] ?? '-');"#,
    )
    .unwrap();
    let mut rules = String::from("RewriteEngine On\n");
    if apache_rule {
        rules.push_str("RewriteRule ^old-page$ index.php?from=htaccess [L]\n");
    }
    rules.push_str("RewriteCond %{REQUEST_FILENAME} !-d\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^ index.php [L]\n");
    std::fs::write(p.join("public").join(".htaccess"), rules).unwrap();
    p
}

fn site(core: &Core, service: Option<&str>, root: &Path, domain: &str, php: &Installation) -> SiteView {
    let detected = core.sites.detect(&root.display().to_string()).unwrap();
    core.sites
        .create(&SiteInput {
            name: domain.into(),
            domain: domain.into(),
            project_root: root.display().to_string(),
            document_root: detected.document_root.display().to_string(),
            project_type: ProjectType::Laravel,
            service_id: service.map(Into::into),
            strategy: Strategy::Php,
            upstream_port: None,
            runtime_installation_id: Some(php.id.clone()),
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        })
        .unwrap_or_else(|e| panic!("create {domain}: {e:?}"))
}

/// Lines in a web server's access log for one app (proves who served it).
fn served_by(core: &Core, service_id: &str, domain: &str) -> usize {
    let log = core.paths.service_logs_dir(service_id).join("sites").join(format!("{domain}-access.log"));
    std::thread::sleep(Duration::from_millis(400));
    std::fs::read_to_string(log).map(|t| t.lines().count()).unwrap_or(0)
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_PHP and ZENV_TEST_FRANKENPHP_ROOT"]
fn apache_and_frankenphp_share_port_80() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();
    let apache = runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap();
    let php = runtime_manager::register(&core.db, ComponentId::Php, Path::new(&env("ZENV_TEST_PHP"))).unwrap();
    let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
    let franken = runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
    let php_franken = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();

    let port = port_manager::suggest_free_port().unwrap();
    let mk = |name: &str, inst: &Installation| {
        core.services
            .create(&ServiceInput { name: name.into(), installation_id: inst.id.clone(), listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
            .unwrap()
    };
    let ap = mk("Apache", &apache);
    let fr = mk("FrankenPHP", &franken);

    // legacy: bound to Apache, needs its .htaccess. modern: bound to
    // FrankenPHP. gis: "any server" with Apache-only rules.
    site(&core, Some(&ap.id), &project(projects.path(), "legacy", true), "legacy.localhost", &php);
    site(&core, Some(&fr.id), &project(projects.path(), "modern", false), "modern.localhost", &php_franken);
    let gis = site(&core, None, &project(projects.path(), "gis", true), "gis.localhost", &php);
    assert!(!gis.compat.frankenphp_ready);

    let check = |label: &str| {
        let r = get(port, "legacy.localhost", "/old-page");
        assert!(r.contains("app=legacy.localhost") && r.contains("from=htaccess"), "{label} legacy: {r}");
        let before = served_by(&core, &fr.id, "modern.localhost");
        let r = get(port, "modern.localhost", "/x");
        assert!(r.contains("app=modern.localhost") && r.contains("sapi=frankenphp"), "{label} modern: {r}");
        assert!(served_by(&core, &fr.id, "modern.localhost") > before, "{label}: FrankenPHP serves modern itself");
        let r = get(port, "gis.localhost", "/old-page");
        assert!(r.contains("from=htaccess"), "{label} gis (Apache-only rules): {r}");
    };

    // FrankenPHP first (holds the port), Apache behind it.
    core.start_service(&fr.id).unwrap();
    core.start_service(&ap.id).unwrap();
    assert!(core.services.is_running(&fr.id) && core.services.is_running(&ap.id));
    assert_eq!(core.services.status(&ap.id).unwrap().behind.as_deref(), Some("FrankenPHP"));
    check("frankenphp in front");

    // FrankenPHP off: Apache moves onto the port; FrankenPHP's app is no
    // longer served (it is bound to FrankenPHP).
    core.stop_service(&fr.id).unwrap();
    assert!(core.services.status(&ap.id).unwrap().behind.is_none());
    assert!(get(port, "legacy.localhost", "/old-page").contains("from=htaccess"));

    // Start apps: Apache (needed by gis) is already in front, FrankenPHP joins behind it.
    let outcomes = core.start_services_for_apps().unwrap();
    assert!(outcomes.iter().all(|o| o.result != "failed"), "{outcomes:?}");
    assert_eq!(core.services.status(&fr.id).unwrap().behind.as_deref(), Some("Apache"));
    check("apache in front");

    // Apache off: FrankenPHP takes the port and keeps serving its app.
    core.stop_service(&ap.id).unwrap();
    assert!(core.services.is_running(&fr.id) && core.services.status(&fr.id).unwrap().behind.is_none());
    assert!(get(port, "modern.localhost", "/").contains("sapi=frankenphp"));

    let stopped = core.stop_all_services().unwrap();
    assert!(stopped.iter().all(|o| o.result == "stopped"), "{stopped:?}");
    assert!(!core.services.is_running(&fr.id) && !core.services.is_running(&ap.id));
    core.shutdown();
}

/// Removing an "any server" app re-checks every web server's configuration,
/// including a stopped Nginx whose port a running Apache holds.
#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT and ZENV_TEST_PHP"]
fn apps_change_while_a_stopped_server_shares_the_port() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();
    let apache = runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap();
    let nginx = runtime_manager::register(&core.db, ComponentId::Nginx, Path::new(&env("ZENV_TEST_NGINX_ROOT"))).unwrap();
    let php = runtime_manager::register(&core.db, ComponentId::Php, Path::new(&env("ZENV_TEST_PHP"))).unwrap();
    let port = port_manager::suggest_free_port().unwrap();
    let mk = |name: &str, inst: &Installation| {
        core.services
            .create(&ServiceInput { name: name.into(), installation_id: inst.id.clone(), listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
            .unwrap()
    };
    let ap = mk("Apache", &apache);
    mk("Nginx", &nginx);
    core.start_service(&ap.id).unwrap();

    let a = site(&core, None, &project(projects.path(), "shop", false), "shop.localhost", &php);
    let b = site(&core, None, &project(projects.path(), "blog", false), "blog.localhost", &php);
    assert!(get(port, "blog.localhost", "/").contains("app=blog.localhost"));
    core.sites.delete(&a.site.id).unwrap_or_else(|e| panic!("delete with a stopped Nginx on the same port: {e:?}"));
    assert!(core.sites.get(&a.site.id).is_err(), "removed");
    assert!(get(port, "blog.localhost", "/").contains("app=blog.localhost"));
    core.sites.delete(&b.site.id).unwrap();
    core.shutdown();
}

/// A proxy app's program starts with its web server and stops with Stop all.
#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT and ZENV_TEST_NODE_ROOT"]
fn proxy_apps_programs_start_with_their_web_server() {
    use zenv_core::local_sites::Strategy;
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();
    let apache = runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap();
    runtime_manager::register(&core.db, ComponentId::Node, Path::new(&env("ZENV_TEST_NODE_ROOT"))).unwrap();
    let port = port_manager::suggest_free_port().unwrap();
    let svc = core
        .services
        .create(&ServiceInput { name: "Apache".into(), installation_id: apache.id, listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
        .unwrap();

    let app_port = port_manager::suggest_free_port().unwrap();
    let root = projects.path().join("chat");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"chat","scripts":{"start":"node server.js"}}"#).unwrap();
    // The app reads its port and settings only from the environment (like
    // many Go apps); its .env names another port, which Zenv's port replaces.
    std::fs::write(root.join(".env"), "APP_PORT=1
GREETING=hello-from-env
").unwrap();
    std::fs::write(
        root.join("server.js"),
        "require('http').createServer((q, r) => r.end('node-app host=' + q.headers.host + ' greeting=' + process.env.GREETING)).listen(Number(process.env.APP_PORT), process.env.HOST);",
    )
    .unwrap();
    let view = core
        .sites
        .create(&SiteInput {
            name: "chat".into(),
            domain: "chat.localhost".into(),
            project_root: root.display().to_string(),
            document_root: root.display().to_string(),
            project_type: ProjectType::Custom,
            service_id: None,
            strategy: Strategy::ReverseProxy,
            upstream_port: Some(app_port),
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        })
        .unwrap();
    let id = view.site.id.clone();
    let status = core.app_process_status(&id).unwrap();
    assert_eq!((status.command.as_deref(), status.detected, status.autostart, status.state), (Some("npm start"), true, false, "stopped"));

    // A detected command (`npm start`) does not run by itself, not even with the web server.
    core.start_service(&svc.id).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(core.app_process_status(&id).unwrap().state, "stopped", "not approved yet");
    assert!(core.start_app_process(&id).is_err());
    assert!(core.autostart_apps(None).unwrap().iter().all(|s| s.id != id));
    // Starting it by hand approves and saves it; from then on it starts with the web server.
    assert!(core.start_app_process_by_user(&id).unwrap());
    let s = core.app_process_status(&id).unwrap();
    assert_eq!((s.command.as_deref(), s.detected, s.autostart), (Some("npm start"), false, true));
    let mut body = String::new();
    for _ in 0..60 {
        body = get(port, "chat.localhost", "/");
        if body.contains("node-app") {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(body.contains("node-app host=chat.localhost greeting=hello-from-env"), "{body}");
    let s = core.app_process_status(&id).unwrap();
    assert_eq!(s.port_vars, vec![format!("APP_PORT={app_port}"), format!("PORT={app_port}"), "HOST=127.0.0.1".to_string()]);
    assert!(!s.exposed, "listens on this computer only");
    assert_eq!(std::fs::read_to_string(root.join(".env")).unwrap(), "APP_PORT=1
GREETING=hello-from-env
", ".env untouched");
    assert_eq!(core.app_process_status(&id).unwrap().state, "running");

    // Start apps reports it as already running; Stop all ends it.
    let outcomes = core.start_services_for_apps().unwrap();
    assert!(outcomes.iter().any(|o| o.name == "chat.localhost" && o.result == "already_running"), "{outcomes:?}");
    core.stop_all_services().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let s = core.app_process_status(&id).unwrap();
    assert!(!s.listening && s.state != "running", "{s:?}");

    // A saved command replaces the detected one; unknown programs are refused.
    assert!(core.set_app_command(&id, Some("cmd /c calc"), true).is_err());
    let s = core.set_app_command(&id, Some("node server.js"), true).unwrap();
    assert_eq!((s.command.as_deref(), s.detected), (Some("node server.js"), false));
    assert!(core.start_app_process(&id).unwrap());
    std::thread::sleep(Duration::from_secs(1));
    assert!(core.app_process_status(&id).unwrap().listening);
    let log = std::fs::read_to_string(zenv_core::app_process::log_file(&core.paths, &id)).unwrap();
    assert!(log.contains("Zenv starting"), "{log}");
    core.shutdown();
    assert!(!core.app_process_status(&id).unwrap().listening);
}
