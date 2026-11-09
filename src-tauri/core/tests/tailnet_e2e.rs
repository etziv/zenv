//! Apps opened on the tailnet: every web server answers an app's loopback
//! tailnet port for this computer's tailnet names (as `tailscale serve` sends
//! them) and refuses other host names (DNS rebinding), for a php-cgi app, an
//! app on FrankenPHP behind the server and a proxy app. Tailnet visitors are
//! not taken for this computer. Tailscale itself is not touched here.
//!
//!   ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT, ZENV_TEST_PHP (php-cgi folder),
//!   ZENV_TEST_FRANKENPHP_ROOT
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

fn get(port: u16, host: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    write!(s, "GET /x HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn php_project(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    std::fs::write(p.join("artisan"), "").unwrap();
    std::fs::write(p.join("composer.json"), "{}").unwrap();
    std::fs::write(p.join("public").join("index.php"), r#"<?php echo "app={$_SERVER['HTTP_HOST']} sapi=" . PHP_SAPI . " ip={$_SERVER['REMOTE_ADDR']}";"#).unwrap();
    std::fs::write(
        p.join("public").join(".htaccess"),
        "RewriteEngine On\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^ index.php [L]\n",
    )
    .unwrap();
    p
}

/// This computer's tailnet names, as `tailnet_open` saves them.
fn save_names(core: &Core) {
    core.db
        .write(|tx| {
            tx.execute("INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('tailnet.hosts', 'desktop-test desktop-test.tail1234.ts.net 100.64.0.1', 't')", [])?;
            Ok(())
        })
        .unwrap();
}

/// A web page elsewhere that pointed its own name at the port gets nothing.
fn assert_refused_elsewhere(port: u16, marker: &str, what: &str) {
    for host in [format!("evil.example:{port}"), "localhost".to_string()] {
        let r = get(port, &host);
        let code = r.split_whitespace().nth(1).unwrap_or("");
        assert!(!r.contains(marker) && (code == "421" || code == "403"), "{what}: {host} answered {code}: {r}");
    }
}

/// A program on its own port that reports the host name it was asked for and
/// who asked (X-Forwarded-For).
fn spawn_program() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let header = |name: &str| {
                req.lines()
                    .find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
                    .unwrap_or_default()
            };
            let body = format!("program host={} ip={}", header("Host"), header("X-Forwarded-For"));
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    port
}

fn site(core: &Core, service: &str, domain: &str, root: &Path, strategy: Strategy, php: Option<&Installation>, upstream: Option<u16>) -> SiteView {
    core.sites
        .create(&SiteInput {
            name: domain.into(),
            domain: domain.into(),
            project_root: root.display().to_string(),
            document_root: if strategy == Strategy::Php { root.join("public").display().to_string() } else { root.display().to_string() },
            project_type: if strategy == Strategy::Php { ProjectType::Laravel } else { ProjectType::Custom },
            service_id: Some(service.into()),
            strategy,
            upstream_port: upstream,
            runtime_installation_id: php.map(|p| p.id.clone()),
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        })
        .unwrap_or_else(|e| panic!("create {domain}: {e:?}"))
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT, ZENV_TEST_PHP and ZENV_TEST_FRANKENPHP_ROOT"]
fn every_web_server_answers_the_tailnet_port() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();
    let php = runtime_manager::register(&core.db, ComponentId::Php, Path::new(&env("ZENV_TEST_PHP"))).unwrap();
    let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
    let franken = runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
    let php_franken = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();
    let servers = [
        runtime_manager::register(&core.db, ComponentId::Apache, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap(),
        runtime_manager::register(&core.db, ComponentId::Nginx, Path::new(&env("ZENV_TEST_NGINX_ROOT"))).unwrap(),
        franken,
    ];
    let program = spawn_program();

    for (i, install) in servers.into_iter().enumerate() {
        let port = port_manager::suggest_free_port().unwrap();
        let svc = core
            .services
            .create(&ServiceInput { name: format!("web {i}"), installation_id: install.id.clone(), listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
            .unwrap();
        let apps = [
            site(&core, &svc.id, &format!("cgi{i}.localhost"), &php_project(projects.path(), &format!("cgi{i}")), Strategy::Php, Some(&php), None),
            site(&core, &svc.id, &format!("fr{i}.localhost"), &php_project(projects.path(), &format!("fr{i}")), Strategy::Php, Some(&php_franken), None),
            site(&core, &svc.id, &format!("api{i}.localhost"), projects.path(), Strategy::ReverseProxy, None, Some(program)),
        ];
        // Open each app on the "tailnet": a row per app and this computer's
        // tailnet names, as tailnet_open saves them.
        save_names(&core);
        let mut tailnet = Vec::new();
        for a in &apps {
            let p = port_manager::suggest_free_port().unwrap();
            core.db
                .write(|tx| {
                    tx.execute("INSERT INTO tailnet_apps (site_id, port, created_at) VALUES (?1, ?2, 't')", rusqlite::params![a.site.id, p])?;
                    Ok(())
                })
                .unwrap();
            tailnet.push(p);
        }
        // The proxy app is also a Tailscale Service: its own name reaches it (and only it).
        let service = format!("svc{i}");
        core.db
            .write(|tx| {
                tx.execute("UPDATE tailnet_apps SET service = ?1 WHERE site_id = ?2", rusqlite::params![service, apps[2].site.id])?;
                Ok(())
            })
            .unwrap();
        core.start_service(&svc.id).unwrap_or_else(|e| panic!("{}: {e}", install.component));

        let expect = [("sapi=cgi-fcgi", "cgi"), ("sapi=frankenphp", "fr"), ("program host=", "api")];
        for ((p, (marker, name)), a) in tailnet.iter().zip(expect).zip(&apps) {
            let host = format!("desktop-test:{p}");
            let r = get(*p, &host);
            assert!(r.contains(marker) && r.contains(&host), "{} {name} on tailnet port {p}: {r}", install.component);
            assert!(r.contains(&format!("ip={}", zenv_core::tailnet::PEER)), "{} {name}: a tailnet visitor is not this computer: {r}", install.component);
            assert_refused_elsewhere(*p, marker, &format!("{} {name}", install.component));
            // The app's own domain still works on the normal port, as this computer.
            let local = get(port, &a.site.domain);
            assert!(local.contains(marker) && !local.contains(zenv_core::tailnet::PEER), "{} {name} local: {local}", install.component);
        }
        let service_host = format!("{service}.tail1234.ts.net");
        let r = get(tailnet[2], &service_host);
        assert!(r.contains("program host=") && r.contains(&service_host), "{} Service name: {r}", install.component);
        assert!(!get(tailnet[0], &service_host).contains("sapi=cgi-fcgi"), "{}: another app's Service name must be refused", install.component);
        println!("{} {}: tailnet ports answer for php-cgi, FrankenPHP and proxy apps", install.component, install.version);
        core.services.stop(&svc.id).unwrap();
    }
    core.shutdown();
}

/// Apps bound to a web server that runs behind another one (sharing port 80):
/// the front server answers their tailnet port itself.
#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT, ZENV_TEST_PHP and ZENV_TEST_FRANKENPHP_ROOT"]
fn apps_behind_another_server_answer_on_the_tailnet() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let projects = tempfile::tempdir().unwrap();
    let program = spawn_program();
    let pairs = [(ComponentId::Apache, ComponentId::Frankenphp), (ComponentId::Frankenphp, ComponentId::Apache), (ComponentId::Apache, ComponentId::Nginx)];
    for (n, (front_c, guest_c)) in pairs.into_iter().enumerate() {
        let data = tempfile::tempdir().unwrap();
        let core = Core::initialize(AppPaths::new(data.path())).unwrap();
        let php = runtime_manager::register(&core.db, ComponentId::Php, Path::new(&env("ZENV_TEST_PHP"))).unwrap();
        let franken_root = PathBuf::from(env("ZENV_TEST_FRANKENPHP_ROOT"));
        let franken = runtime_manager::register(&core.db, ComponentId::Frankenphp, &franken_root).unwrap();
        let php_franken = runtime_manager::register(&core.db, ComponentId::Php, &franken_root).unwrap();
        let install = |c: ComponentId| match c {
            ComponentId::Apache => runtime_manager::register(&core.db, c, Path::new(&env("ZENV_TEST_APACHE_ROOT"))).unwrap(),
            ComponentId::Nginx => runtime_manager::register(&core.db, c, Path::new(&env("ZENV_TEST_NGINX_ROOT"))).unwrap(),
            _ => franken.clone(),
        };
        let port = port_manager::suggest_free_port().unwrap();
        let mk = |name: &str, inst: Installation| {
            core.services
                .create(&ServiceInput { name: name.into(), installation_id: inst.id, listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
                .unwrap()
        };
        let front = mk("front", install(front_c));
        let guest = mk("guest", install(guest_c));
        let apps = [
            site(&core, &guest.id, &format!("cgi{n}.localhost"), &php_project(projects.path(), &format!("bcgi{n}")), Strategy::Php, Some(&php), None),
            site(&core, &guest.id, &format!("fr{n}.localhost"), &php_project(projects.path(), &format!("bfr{n}")), Strategy::Php, Some(&php_franken), None),
            site(&core, &guest.id, &format!("api{n}.localhost"), projects.path(), Strategy::ReverseProxy, None, Some(program)),
        ];
        save_names(&core);
        let mut tailnet = Vec::new();
        for a in &apps {
            let p = port_manager::suggest_free_port().unwrap();
            core.db
                .write(|tx| {
                    tx.execute("INSERT INTO tailnet_apps (site_id, port, created_at) VALUES (?1, ?2, 't')", rusqlite::params![a.site.id, p])?;
                    Ok(())
                })
                .unwrap();
            tailnet.push(p);
        }
        core.start_service(&front.id).unwrap();
        core.start_service(&guest.id).unwrap();
        assert!(core.services.hosted(&guest.id).is_some(), "{guest_c} runs behind {front_c}");

        for ((p, marker), a) in tailnet.iter().zip(["sapi=cgi-fcgi", "sapi=frankenphp", "program host="]).zip(&apps) {
            let host = format!("100.64.0.1:{p}");
            let r = get(*p, &host);
            assert!(r.contains(marker) && r.contains(&host), "{guest_c} behind {front_c}, {}: {r}", a.site.domain);
            assert!(r.contains(&format!("ip={}", zenv_core::tailnet::PEER)), "{guest_c} behind {front_c}, {}: not this computer: {r}", a.site.domain);
            assert_refused_elsewhere(*p, marker, &format!("{guest_c} behind {front_c}, {}", a.site.domain));
            assert!(get(port, &a.site.domain).contains(marker), "{guest_c} behind {front_c}, {} locally", a.site.domain);
        }
        println!("{guest_c} behind {front_c}: tailnet ports answer");
        core.shutdown();
    }
}
