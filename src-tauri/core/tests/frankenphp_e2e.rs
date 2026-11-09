//! FrankenPHP as the front web server: static files, PHP on another version
//! through php-cgi (`php_fastcgi`), PHP on FrankenPHP's own version, and a
//! reverse proxy, all on one port; reload picks up a new app; stop is graceful.
//!
//!   ZENV_TEST_FRANKENPHP_ROOT=C:\zenv\bin\frankenphp\1.12.2
//!   ZENV_TEST_PHP=C:\zenv\bin\php\8.4.21          (a folder with php-cgi.exe)
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager;
use zenv_core::service_manager::ServiceInput;

fn get(port: u16, host: &str, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    (out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0), out)
}

fn project(root: &Path, name: &str, php: bool) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    if php {
        std::fs::write(p.join("public").join("index.php"), format!(r#"<?php echo "{name} php=" . PHP_VERSION . " sapi=" . PHP_SAPI;"#)).unwrap();
    } else {
        std::fs::write(p.join("public").join("index.html"), format!("<h1>{name} static</h1>")).unwrap();
    }
    std::fs::write(p.join("public").join(".env"), "SECRET=1").unwrap();
    std::fs::create_dir_all(p.join("public").join(".git")).unwrap();
    std::fs::write(p.join("public").join(".git").join("config"), "[core] secret").unwrap();
    std::fs::create_dir_all(p.join("public").join("sub")).unwrap();
    std::fs::write(p.join("public").join("sub").join(".env"), "SECRET=2").unwrap();
    std::fs::create_dir_all(p.join("public").join("files")).unwrap();
    std::fs::write(p.join("public").join("files").join("report.txt"), "listed?").unwrap();
    p
}

fn site(root: &Path, domain: &str, strategy: Strategy, php: Option<&str>, upstream: Option<u16>) -> SiteInput {
    SiteInput {
        name: domain.into(),
        domain: domain.into(),
        project_root: root.display().to_string(),
        document_root: "public".into(),
        project_type: if php.is_some() { ProjectType::Php } else { ProjectType::Static },
        service_id: None,
        strategy,
        upstream_port: upstream,
        runtime_installation_id: php.map(str::to_string),
        php_engine: PhpEngine::Auto,
        https_enabled: false,
        https_redirect: false,
        enabled: true,
    }
}

#[test]
#[ignore = "requires ZENV_TEST_FRANKENPHP_ROOT and ZENV_TEST_PHP"]
fn frankenphp_serves_every_kind_of_app() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let home = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
    let franken = runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
    let franken_php = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();
    let php = runtime_manager::register(&core.db, ComponentId::Php, Path::new(&env("ZENV_TEST_PHP"))).unwrap();

    let port = port_manager::suggest_free_port().unwrap();
    let svc = core
        .services
        .create(&ServiceInput {
            name: "FrankenPHP".into(),
            installation_id: franken.id.clone(),
            listen_address: "127.0.0.1".into(),
            http_port: port,
            autostart: false,
            data_dir: None,
            https_port: None,
        })
        .unwrap();
    assert!(core.services.validate(&svc.id).unwrap().ok);

    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let up_port = upstream.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in upstream.incoming().flatten() {
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nupstream-ok");
        }
    });

    for input in [
        site(&project(projects.path(), "docs", false), "docs.localhost", Strategy::Static, None, None),
        site(&project(projects.path(), "shop", true), "shop.localhost", Strategy::Php, Some(&php.id), None),
        site(&project(projects.path(), "modern", true), "modern.localhost", Strategy::Php, Some(&franken_php.id), None),
        site(&project(projects.path(), "api", false), "api.localhost", Strategy::ReverseProxy, None, Some(up_port)),
    ] {
        core.sites.create(&input).unwrap_or_else(|e| panic!("{}: {e}", input.domain));
    }
    core.start_service(&svc.id).expect("start FrankenPHP");

    let (code, body) = get(port, "docs.localhost", "/");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("docs static"));
    let (code, body) = get(port, "shop.localhost", "/");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains(&format!("php={}", php.version)) && body.contains("sapi=cgi-fcgi"), "{body}");
    let (code, body) = get(port, "modern.localhost", "/");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("sapi=frankenphp"), "{body}");
    assert!(get(port, "api.localhost", "/").1.contains("upstream-ok"));
    assert_eq!(get(port, "shop.localhost", "/.env").0, 403, ".env must never be served");
    // Hidden files at any depth (also as the short name GIT~1), and no folder listing.
    for host in ["docs.localhost", "shop.localhost", "modern.localhost"] {
        for path in ["/.git/config", "/sub/.env", "/GIT~1/config"] {
            let (code, r) = get(port, host, path);
            assert!(!r.contains("SECRET") && !r.contains("[core]"), "{host}{path} leaked: {r}");
            assert!(code == 403 || code == 404, "{host}{path} answered {code}");
        }
        assert!(!get(port, host, "/files/").1.contains("report.txt"), "{host}/files/ is listed");
    }
    println!("FrankenPHP {} on :{port}: static, PHP {} (php-cgi), PHP {} (FrankenPHP), proxy", franken.version, php.version, franken_php.version);

    // A new app is picked up by a reload, without a restart.
    core.sites.create(&site(&project(projects.path(), "late", false), "late.localhost", Strategy::Static, None, None)).unwrap();
    assert!(get(port, "late.localhost", "/").1.contains("late static"));

    let started = std::time::Instant::now();
    core.services.stop(&svc.id).unwrap();
    println!("stopped in {:.1?}", started.elapsed());
    assert!(!core.services.is_running(&svc.id));
    core.shutdown();
}
