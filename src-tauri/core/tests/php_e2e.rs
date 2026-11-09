//! PHP sites end to end: per-site PHP versions through Apache + php-cgi, and
//! automatic FrankenPHP routing for the PHP version it embeds.
//!
//!   ZENV_TEST_APACHE_ROOT=C:\...\Apache24
//!   ZENV_TEST_PHP=C:\php-8.4;C:\php-8.1          (folders with php-cgi.exe)
//!   ZENV_TEST_FRANKENPHP_ROOT=C:\...\frankenphp\1.12.2
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use zenv_core::app_core::Core;
use zenv_core::error::AppError;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, SiteView, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager::{self, PortStatus};
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager::{self, Installation};
use zenv_core::service_manager::ServiceInput;

fn get(port: u16, host: &str, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    (out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0), out)
}

/// A minimal Laravel-shaped project: artisan, composer.json, public/ with a
/// front controller and the stock rewrite rules.
fn laravel_project(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    std::fs::write(p.join("artisan"), "#!/usr/bin/env php").unwrap();
    std::fs::write(p.join("composer.json"), "{}").unwrap();
    std::fs::write(
        p.join("public").join("index.php"),
        r#"<?php echo "site={$_SERVER['HTTP_HOST']} php=" . PHP_VERSION . " sapi=" . PHP_SAPI . " uri={$_SERVER['REQUEST_URI']}";"#,
    )
    .unwrap();
    std::fs::write(
        p.join("public").join(".htaccess"),
        "RewriteEngine On\nRewriteCond %{REQUEST_FILENAME} !-d\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^ index.php [L]\n",
    )
    .unwrap();
    std::fs::write(p.join("public").join(".env"), "APP_KEY=secret").unwrap();
    p
}

fn php_site(core: &Core, service: &str, root: &Path, domain: &str, php: &Installation, engine: PhpEngine) -> SiteView {
    let detected = core.sites.detect(&root.display().to_string()).unwrap();
    assert_eq!((detected.project_type, detected.strategy), (ProjectType::Laravel, Strategy::Php));
    core.sites
        .create(&SiteInput {
            name: domain.into(),
            domain: domain.into(),
            project_root: root.display().to_string(),
            document_root: detected.document_root.display().to_string(),
            project_type: ProjectType::Laravel,
            service_id: Some(service.into()),
            strategy: Strategy::Php,
            upstream_port: None,
            runtime_installation_id: Some(php.id.clone()),
            php_engine: engine,            https_enabled: false,
            https_redirect: false,
            enabled: true,
        })
        .unwrap_or_else(|e| panic!("create {domain}: {e}"))
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_PHP and ZENV_TEST_FRANKENPHP_ROOT"]
fn php_sites_end_to_end() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();

    let apache = runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap();
    let phps: Vec<Installation> = env("ZENV_TEST_PHP")
        .split(';')
        .map(|p| runtime_manager::register(&core.db, ComponentId::Php, Path::new(p.trim())).unwrap())
        .collect();
    let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
    let franken = runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
    // FrankenPHP's folder also ships the PHP it embeds; that is the version a
    // site picks to be routed to FrankenPHP.
    let php_franken = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();
    println!(
        "PHP {}; FrankenPHP {} ({})",
        phps.iter().map(|p| p.version.as_str()).collect::<Vec<_>>().join(", "),
        franken.version,
        php_franken.version
    );

    let port = port_manager::suggest_free_port().unwrap();
    let svc = core
        .services
        .create(&ServiceInput { name: "PHP Apache".into(), installation_id: apache.id, listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
        .unwrap();
    core.start_service(&svc.id).unwrap();

    // Two projects, two PHP versions, one Apache.
    let a = php_site(&core, &svc.id, &laravel_project(projects.path(), "shop"), "shop.localhost", &phps[0], PhpEngine::Auto);
    let b = php_site(&core, &svc.id, &laravel_project(projects.path(), "legacy"), "legacy.localhost", &phps[1], PhpEngine::Auto);
    assert_eq!(a.php_backend, Some("fastcgi"));
    let (code, body) = get(port, "shop.localhost", "/");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains(&format!("php={}", phps[0].version)) && body.contains("sapi=cgi-fcgi"), "{body}");
    let (_, body) = get(port, "shop.localhost", "/users/9?tab=orders");
    assert!(body.contains("uri=/users/9?tab=orders"), "Laravel-style rewrite: {body}");
    assert_eq!(get(port, "shop.localhost", "/.env").0, 403, ".env must never be served");
    // Laravel's rewrite rules send unknown paths, even *.php, to the front controller.
    assert!(get(port, "shop.localhost", "/missing.php").1.contains("uri=/missing.php"));
    let (_, body) = get(port, "legacy.localhost", "/");
    assert!(body.contains(&format!("php={}", phps[1].version)), "{body}");
    assert_eq!(b.php_version.as_deref(), Some(phps[1].version.as_str()));

    // Parallel requests are spread over the worker pool.
    let handles: Vec<_> = (0..12).map(|_| std::thread::spawn(move || get(port, "shop.localhost", "/").0)).collect();
    assert!(handles.into_iter().all(|h| h.join().unwrap() == 200));

    // The PHP version FrankenPHP embeds is routed to FrankenPHP automatically.
    let c = php_site(&core, &svc.id, &laravel_project(projects.path(), "modern"), "modern.localhost", &php_franken, PhpEngine::Auto);
    assert_eq!(c.php_backend, Some("frankenphp"));
    let (code, body) = get(port, "modern.localhost", "/orders/1");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("sapi=frankenphp") && body.contains("uri=/orders/1") && body.contains("site=modern.localhost"), "{body}");
    let checks = core.sites.diagnose(&c.site.id).unwrap();
    assert!(checks.iter().all(|c| format!("{:?}", c.status) == "Ok"), "{checks:?}");

    // Project logs: each project sees its own PHP errors, access lines and app log.
    let warn = |project: &str, tag: &str| {
        std::fs::write(
            projects.path().join(project).join("public").join("warn.php"),
            format!(r#"<?php error_log("{tag}"); trigger_error("{tag}-warning", E_USER_WARNING); echo "ok";"#),
        )
        .unwrap();
    };
    warn("shop", "zenv-shop-error");
    warn("modern", "zenv-modern-error");
    let laravel_logs = projects.path().join("shop").join("storage").join("logs");
    std::fs::create_dir_all(&laravel_logs).unwrap();
    std::fs::write(laravel_logs.join("laravel.log"), "[2026-10-19 10:00:00] local.ERROR: zenv-app-error\n").unwrap();
    assert_eq!(get(port, "shop.localhost", "/warn.php").0, 200);
    assert_eq!(get(port, "modern.localhost", "/warn.php").0, 200);
    let ids: Vec<String> = core.sites.log_sources(&a.site.id).unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, ["access", "error", "php", "app"]);
    let read = |site: &SiteView, kind: &str| core.sites.read_log(&site.site.id, kind, 200).unwrap().lines.join("\n");
    wait_for("shop PHP error in its project log", || read(&a, "php").contains("zenv-shop-error"));
    let shop_php = read(&a, "php");
    println!("shop php log:\n{shop_php}");
    assert!(shop_php.lines().any(|l| l.starts_with("PHP Warning:  zenv-shop-error-warning")), "one message per line: {shop_php}");
    assert!(!read(&b, "php").contains("zenv-shop-error"), "other projects do not see it");
    assert!(read(&a, "access").contains("/warn.php"));
    assert!(read(&a, "app").contains("zenv-app-error"));
    wait_for("FrankenPHP error in its project log", || read(&c, "php").contains("zenv-modern-error"));
    println!("modern php log:\n{}", read(&c, "php"));
    assert!(!read(&c, "php").contains("zenv-shop-error"));
    let groups = core.runtimes.log_groups().unwrap();
    assert!(groups.iter().any(|g| g.installation_id == phps[0].id && g.sources.iter().any(|s| s.id == "php-cgi-0")), "{groups:?}");
    assert!(groups.iter().any(|g| g.installation_id == franken.id), "{groups:?}");

    // Switching FrankenPHP off re-routes that site to Apache + php-cgi, and back.
    core.set_installation_enabled(&franken.id, false).unwrap();
    let (_, body) = get(port, "modern.localhost", "/");
    assert!(body.contains("sapi=cgi-fcgi") && body.contains(&format!("php={}", php_franken.version)), "{body}");
    core.set_installation_enabled(&franken.id, true).unwrap();
    assert!(get(port, "modern.localhost", "/").1.contains("sapi=frankenphp"));

    // Languages are always available: a PHP version cannot be switched off.
    assert!(matches!(core.set_installation_enabled(&phps[1].id, false), Err(AppError::Validation(_))));

    // A killed php-cgi worker is restarted by the supervisor.
    let pool = core.runtime_status().into_iter().find(|r| r.installation_id == phps[0].id).expect("pool");
    let victim_port = pool.ports[0];
    let pid = match port_manager::inspect(victim_port, "127.0.0.1".parse().unwrap()).unwrap().status {
        PortStatus::InUse { owners } => owners[0].pid,
        other => panic!("worker not listening: {other:?}"),
    };
    std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).output().unwrap();
    wait_for("worker restart", || {
        matches!(port_manager::inspect(victim_port, "127.0.0.1".parse().unwrap()).map(|i| i.status),
            Ok(PortStatus::InUse { ref owners }) if owners[0].pid != pid)
    });
    assert_eq!(get(port, "shop.localhost", "/").0, 200);

    // Removing every PHP site stops the pools and FrankenPHP.
    for s in [&a, &b, &c] {
        core.sites.delete(&s.site.id).unwrap();
    }
    assert!(core.runtime_status().is_empty(), "{:?}", core.runtime_status());
    assert!(projects.path().join("shop").join("public").join("index.php").is_file());

    core.shutdown();
}
