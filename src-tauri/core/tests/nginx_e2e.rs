//! Apache or Nginx, whichever is switched on: "any server" sites (static, PHP
//! per version, FrankenPHP, reverse proxy) served by both, sharing one port.
//!
//!   ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT, ZENV_TEST_PHP (two folders,
//!   ';'-separated), ZENV_TEST_FRANKENPHP_ROOT
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, SiteView, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
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

fn server_header(resp: &str) -> String {
    resp.lines().find_map(|l| l.strip_prefix("Server: ")).unwrap_or("").to_string()
}

fn spawn_upstream() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = "upstream-ok";
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    port
}

fn project(root: &Path, name: &str, php: bool) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    if php {
        std::fs::write(p.join("artisan"), "").unwrap();
        std::fs::write(p.join("composer.json"), "{}").unwrap();
        std::fs::write(
            p.join("public").join("index.php"),
            format!(r#"<?php echo "{name} php=" . PHP_VERSION . " sapi=" . PHP_SAPI . " uri={{$_SERVER['REQUEST_URI']}}";"#),
        )
        .unwrap();
        std::fs::write(
            p.join("public").join(".htaccess"),
            "RewriteEngine On\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^ index.php [L]\n",
        )
        .unwrap();
    } else {
        std::fs::write(p.join("public").join("index.html"), format!("<h1>{name} static</h1>")).unwrap();
    }
    std::fs::write(p.join("public").join(".env"), "SECRET=1").unwrap();
    // Hidden files below the top level, and a folder without an index page.
    std::fs::create_dir_all(p.join("public").join(".git")).unwrap();
    std::fs::write(p.join("public").join(".git").join("config"), "[core] secret").unwrap();
    std::fs::create_dir_all(p.join("public").join("sub")).unwrap();
    std::fs::write(p.join("public").join("sub").join(".env"), "SECRET=2").unwrap();
    std::fs::create_dir_all(p.join("public").join("files")).unwrap();
    std::fs::write(p.join("public").join("files").join("report.txt"), "listed?").unwrap();
    p
}

/// Hidden files are refused at any depth, also through a Windows short name
/// (GIT~1 for .git); folders are not listed.
fn assert_hidden_refused(port: u16, host: &str) {
    for path in ["/.env", "/.git/config", "/sub/.env", "/GIT~1/config"] {
        let (code, r) = get(port, host, path);
        assert!(!r.contains("SECRET") && !r.contains("[core]"), "{host}{path} leaked: {r}");
        // A PHP app may answer an unknown path itself (front controller).
        assert!(code == 403 || code == 404 || (path.contains('~') && r.contains(" php=")), "{host}{path} answered {code}");
    }
    let (code, r) = get(port, host, "/files/");
    assert!(!r.contains("report.txt"), "{host}/files/ is listed ({code})");
}

fn site(root: &Path, domain: &str, strategy: Strategy, php: Option<&Installation>, upstream: Option<u16>) -> SiteInput {
    SiteInput {
        name: domain.into(),
        domain: domain.into(),
        project_root: root.display().to_string(),
        document_root: "public".into(),
        project_type: if php.is_some() { ProjectType::Laravel } else { ProjectType::Static },
        service_id: None, // whichever web server is running
        strategy,
        upstream_port: upstream,
        runtime_installation_id: php.map(|p| p.id.clone()),
        php_engine: PhpEngine::Auto,
        https_enabled: false,
        https_redirect: false,
        enabled: true,
    }
}

fn assert_all_sites(port: u16, phps: &[Installation], franken_php: &Installation, expect_server: &str) {
    let (code, r) = get(port, "docs.localhost", "/");
    assert_eq!(code, 200, "{r}");
    assert!(r.contains("docs static"));
    assert!(server_header(&r).to_lowercase().contains(expect_server), "{}", server_header(&r));
    let (_, r) = get(port, "shop.localhost", "/orders/5?x=1");
    assert!(r.contains(&format!("php={}", phps[0].version)) && r.contains("uri=/orders/5?x=1"), "{r}");
    let (_, r) = get(port, "legacy.localhost", "/");
    assert!(r.contains(&format!("php={}", phps[1].version)), "{r}");
    let (_, r) = get(port, "modern.localhost", "/a/b");
    assert!(r.contains("sapi=frankenphp") && r.contains(&format!("php={}", franken_php.version)), "{r}");
    assert!(get(port, "api.localhost", "/").1.contains("upstream-ok"));
    for host in ["docs.localhost", "shop.localhost", "legacy.localhost", "modern.localhost"] {
        assert_hidden_refused(port, host);
    }
    assert!(get(port, "nobody.localhost", "/").1.contains("default-index"));
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT, ZENV_TEST_PHP and ZENV_TEST_FRANKENPHP_ROOT"]
fn apache_or_nginx_whichever_is_on() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();

    let apache = runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap();
    let nginx = runtime_manager::register(&core.db, ComponentId::Nginx, Path::new(&env("ZENV_TEST_NGINX_ROOT"))).unwrap();
    let phps: Vec<Installation> = env("ZENV_TEST_PHP")
        .split(';')
        .map(|p| runtime_manager::register(&core.db, ComponentId::Php, Path::new(p.trim())).unwrap())
        .collect();
    let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
    runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
    let franken_php = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();
    println!("Apache {}, Nginx {}", apache.version, nginx.version);

    // Both web servers on the same port.
    let port = port_manager::suggest_free_port().unwrap();
    let mk = |name: &str, inst: &Installation| {
        core.services
            .create(&ServiceInput { name: name.into(), installation_id: inst.id.clone(), listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
            .unwrap()
    };
    let ap = mk("Apache", &apache);
    let ng = mk("Nginx", &nginx);

    let up = spawn_upstream();
    let sites: Vec<SiteView> = vec![
        site(&project(projects.path(), "docs", false), "docs.localhost", Strategy::Static, None, None),
        site(&project(projects.path(), "shop", true), "shop.localhost", Strategy::Php, Some(&phps[0]), None),
        site(&project(projects.path(), "legacy", true), "legacy.localhost", Strategy::Php, Some(&phps[1]), None),
        site(&project(projects.path(), "modern", true), "modern.localhost", Strategy::Php, Some(&franken_php), None),
        site(&project(projects.path(), "api", false), "api.localhost", Strategy::ReverseProxy, None, Some(up)),
    ]
    .iter()
    .map(|s| core.sites.create(s).unwrap_or_else(|e| panic!("{}: {e}", s.domain)))
    .collect();
    assert_eq!(sites[3].php_backend, Some("frankenphp"));

    // Nginx on.
    core.start_service(&ng.id).expect("start nginx");
    assert_all_sites(port, &phps, &franken_php, "nginx");

    // PHP errors reach the project's PHP log through Nginx's FastCGI stderr.
    std::fs::write(
        projects.path().join("shop").join("public").join("warn.php"),
        r#"<?php trigger_error("zenv-nginx-warning", E_USER_WARNING); undefined_call();"#,
    )
    .unwrap();
    get(port, "shop.localhost", "/warn.php");
    let mut php_log = String::new();
    for _ in 0..40 {
        php_log = core.sites.read_log(&sites[1].site.id, "php", 100).unwrap().lines.join("\n");
        if php_log.contains("undefined_call") {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    println!("nginx php log:\n{php_log}");
    let lines: Vec<&str> = php_log.lines().collect();
    assert!(lines.iter().any(|l| l.contains(" PHP Warning:  zenv-nginx-warning in ")), "{php_log}");
    assert!(lines.iter().any(|l| l.starts_with("PHP Fatal error:  Uncaught Error")), "one message per line: {php_log}");
    assert!(lines.iter().all(|l| !l.contains("FastCGI sent in stderr")), "nginx wrapping removed: {php_log}");

    // Apache on as well: it runs behind Nginx, which keeps the port.
    core.start_service(&ap.id).expect("start apache");
    assert!(core.services.is_running(&ng.id) && core.services.is_running(&ap.id));
    assert_eq!(core.services.hosted(&ap.id).map(|(f, _)| f), Some(ng.id.clone()));
    assert_all_sites(port, &phps, &franken_php, "nginx");

    // Nginx off: Apache moves onto the freed port.
    core.stop_service(&ng.id).expect("stop nginx");
    assert!(core.services.is_running(&ap.id) && core.services.hosted(&ap.id).is_none());
    assert_all_sites(port, &phps, &franken_php, "apache");

    // Nginx on again (behind Apache); a site added while both run is served.
    core.start_service(&ng.id).expect("start nginx again");
    assert!(core.services.is_running(&ap.id) && core.services.is_running(&ng.id));
    let late = core
        .sites
        .create(&site(&project(projects.path(), "late", false), "late.localhost", Strategy::Static, None, None))
        .unwrap();
    assert!(get(port, "late.localhost", "/").1.contains("late static"));
    let checks = core.sites.diagnose(&late.site.id).unwrap();
    assert!(checks.iter().all(|c| format!("{:?}", c.status) == "Ok"), "{checks:?}");

    for s in sites.iter().chain(std::iter::once(&late)) {
        core.sites.delete(&s.site.id).unwrap();
    }
    assert!(core.runtime_status().is_empty());
    core.shutdown();
    assert!(!core.services.is_running(&ng.id) && !core.services.is_running(&ap.id));
}
