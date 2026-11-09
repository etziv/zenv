//! Milestone 2 end-to-end test: local sites served by a real Apache.
//!   ZENV_TEST_APACHE_ROOT=C:\path\to\Apache24 npm run test:e2e

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use zenv_core::app_core::Core;
use zenv_core::error::AppError;
use zenv_core::local_sites::{ConfigStatus, PhpEngine, ProjectType, SiteInput, SiteQuery, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager;
use zenv_core::service_manager::{Health, ServiceInput};

/// GET with an explicit Host header against the server's listener.
fn get(port: u16, host: &str, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, out)
}

/// A tiny upstream "app" answering every request with a fixed body.
fn spawn_upstream(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let host = req.lines().find_map(|l| l.strip_prefix("Host: ")).unwrap_or("").to_string();
            let text = format!("{body} host={host}");
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
        }
    });
    port
}

fn project(root: &Path, name: &str, marker: &str) -> PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(p.join("public")).unwrap();
    std::fs::write(p.join("public").join("index.html"), format!("<h1>{marker}</h1>")).unwrap();
    std::fs::write(p.join("public").join("info.php"), "<?php echo 'secret source';").unwrap();
    std::fs::write(p.join("public").join(".env"), "DB_PASSWORD=hunter2").unwrap();
    p
}

fn static_site(service_id: &str, root: &Path, name: &str, domain: &str) -> SiteInput {
    SiteInput {
        name: name.into(),
        domain: domain.into(),
        project_root: root.display().to_string(),
        document_root: "public".into(),
        project_type: ProjectType::Static,
        service_id: Some(service_id.into()),
        strategy: Strategy::Static,
        upstream_port: None,
        runtime_installation_id: None,
        php_engine: PhpEngine::Auto,        https_enabled: false,
        https_redirect: false,
        enabled: true,
    }
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT pointing at an Apache for Windows installation"]
fn local_sites_end_to_end() {
    let apache_root = PathBuf::from(std::env::var_os("ZENV_TEST_APACHE_ROOT").expect("set ZENV_TEST_APACHE_ROOT"));
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();

    let install = runtime_manager::register(&core.db, ComponentId::Apache, &apache_root).unwrap();
    let port = port_manager::suggest_free_port().unwrap();
    let svc = core
        .services
        .create(&ServiceInput {
            name: "Sites Apache".into(),
            installation_id: install.id,
            listen_address: "127.0.0.1".into(),
            http_port: port,
            autostart: false, data_dir: None, https_port: None
        })
        .unwrap();
    core.services.start(&svc.id).expect("start apache");

    // Static site, created while the server runs: applied and reloaded.
    let shop_root = project(projects.path(), "shop", "shop-home");
    let detected = core.sites.detect(&shop_root.display().to_string()).unwrap();
    assert_eq!(detected.project_type, ProjectType::Static);
    let shop = core.sites.create(&static_site(&svc.id, &shop_root, "Shop", "shop.localhost")).unwrap();
    assert_eq!(shop.site.config_status, ConfigStatus::Applied);
    assert_eq!(shop.url, format!("http://shop.localhost:{port}/"));
    let (code, body) = get(port, "shop.localhost", "/");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("shop-home"));
    assert_eq!(get(port, "shop.localhost", "/info.php").0, 403, "PHP source must not leak");
    assert_eq!(get(port, "shop.localhost", "/.env").0, 403, "dotfiles must not be served");

    // Unknown hosts fall back to the server's default page.
    let (_, fallback) = get(port, "nobody.localhost", "/");
    assert!(fallback.contains(r#"data-zenv="default-index""#), "{fallback}");

    // Reverse proxy to a running app; the Host header is preserved.
    let app_port = spawn_upstream("upstream-ok");
    let mut api = static_site(&svc.id, &projects.path().join("shop"), "API", "api.localhost");
    api.strategy = Strategy::ReverseProxy;
    api.upstream_port = Some(app_port);
    api.project_type = ProjectType::Go;
    let api_view = core.sites.create(&api).unwrap();
    let (code, body) = get(port, "api.localhost", "/anything");
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("upstream-ok host=api.localhost"), "{body}");
    let checks = core.sites.diagnose(&api_view.site.id).unwrap();
    assert!(checks.iter().all(|c| format!("{:?}", c.status) == "Ok"), "{checks:?}");

    // Disable → falls back to the default page; enable → served again.
    core.sites.set_enabled(&shop.site.id, false).unwrap();
    assert!(get(port, "shop.localhost", "/").1.contains("default-index"));
    core.sites.set_enabled(&shop.site.id, true).unwrap();
    assert!(get(port, "shop.localhost", "/").1.contains("shop-home"));

    // Invalid input is rejected before anything is written.
    let dup = core.sites.create(&static_site(&svc.id, &shop_root, "Dup", "SHOP.localhost")).unwrap_err();
    assert!(matches!(dup, AppError::Validation(_)), "{dup}");
    let mut loop_proxy = api.clone();
    loop_proxy.domain = "loop.localhost".into();
    loop_proxy.upstream_port = Some(port);
    assert!(core.sites.create(&loop_proxy).is_err(), "proxying to itself is refused");

    // A broken include makes activation fail: nothing changes, server keeps serving.
    let sites_dir = data.path().join("etc").join(&svc.id).join("sites");
    std::fs::write(sites_dir.join("zz-user.conf"), "NotADirective here\n").unwrap();
    let blog_root = project(projects.path(), "blog", "blog-home");
    let err = core.sites.create(&static_site(&svc.id, &blog_root, "Blog", "blog.localhost")).unwrap_err();
    match &err {
        AppError::ConfigInvalid { output, .. } => assert!(output.contains("zz-user.conf"), "{output}"),
        other => panic!("expected ConfigInvalid, got {other:?}"),
    }
    assert_eq!(core.sites.list(&SiteQuery { search: "blog".into(), ..Default::default() }).unwrap().total, 0);
    assert!(get(port, "shop.localhost", "/").1.contains("shop-home"), "previous config still active");
    assert_eq!(core.services.status(&svc.id).unwrap().health, Health::Running);
    std::fs::remove_file(sites_dir.join("zz-user.conf")).unwrap();

    // Many sites at once.
    let started = Instant::now();
    let bulk_root = project(projects.path(), "bulk", "bulk-home");
    for i in 0..300 {
        let mut s = static_site(&svc.id, &bulk_root, &format!("Bulk {i}"), &format!("b{i}.localhost"));
        s.enabled = i == 299; // only the last create triggers a visible change for the others
        core.sites.create(&s).unwrap();
    }
    let page = core.sites.list(&SiteQuery { search: "bulk".into(), offset: 0, limit: 50 }).unwrap();
    assert_eq!((page.total, page.items.len()), (300, 50));
    for i in [0, 150, 298] {
        let id = core.sites.list(&SiteQuery { search: format!("b{i}.localhost"), ..Default::default() }).unwrap().items[0].site.id.clone();
        core.sites.set_enabled(&id, true).unwrap();
    }
    for i in [0, 150, 298, 299] {
        assert!(get(port, &format!("b{i}.localhost"), "/").1.contains("bulk-home"), "b{i}");
    }
    println!("300 sites created and 4 activated in {:?}", started.elapsed());

    // Deleting a site removes its config but never the project files.
    core.sites.delete(&shop.site.id).unwrap();
    assert!(get(port, "shop.localhost", "/").1.contains("default-index"));
    assert!(shop_root.join("public").join("index.html").is_file());
    assert!(shop_root.join("public").join(".env").is_file());

    core.services.stop(&svc.id).unwrap();
}
