//! HTTPS with certificates from Zenv's own CA, on Apache and on Nginx:
//! the TLS listener appears only once a site uses HTTPS, HTTP redirects to
//! HTTPS, and curl verifies the chain against the Zenv CA (the CA is never
//! added to the Windows trust store by this test).
//!
//!   ZENV_TEST_APACHE_ROOT, ZENV_TEST_NGINX_ROOT
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager;
use zenv_core::service_manager::ServiceInput;

fn http_get(port: u16, host: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

/// curl (Windows' schannel build) verifying against the Zenv CA only.
fn curl_https(ca: &Path, host: &str, port: u16) -> (bool, String) {
    let out = Command::new("curl.exe")
        .args(["-sS", "--ssl-no-revoke", "--cacert"])
        .arg(ca)
        .args(["--resolve", &format!("{host}:{port}:127.0.0.1")])
        .arg(format!("https://{host}:{port}/"))
        .output()
        .expect("curl.exe");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok()
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT and ZENV_TEST_NGINX_ROOT"]
fn https_sites_on_apache_and_nginx() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().join("secure");
    std::fs::create_dir_all(root.join("public")).unwrap();
    std::fs::write(root.join("public").join("index.html"), "<h1>secure static</h1>").unwrap();

    let ca = core.tls.create_ca().unwrap();
    assert!(ca.exists && !ca.trusted_user, "creating the CA never trusts it");

    for (component, var) in [(ComponentId::Apache, "ZENV_TEST_APACHE_ROOT"), (ComponentId::Nginx, "ZENV_TEST_NGINX_ROOT")] {
        let install = runtime_manager::register(&core.db, component, Path::new(&env(var))).unwrap();
        let http = port_manager::suggest_free_port().unwrap();
        let https = port_manager::suggest_free_port().unwrap();
        let svc = core
            .services
            .create(&ServiceInput {
                name: format!("{component} TLS"),
                installation_id: install.id.clone(),
                listen_address: "127.0.0.1".into(),
                http_port: http,
                autostart: false,
                data_dir: None,
                https_port: Some(https),
            })
            .unwrap();
        assert_eq!(svc.https_port, Some(https));
        let domain = format!("secure-{component}.localhost");
        let mut input = SiteInput {
            name: domain.clone(),
            domain: domain.clone(),
            project_root: root.display().to_string(),
            document_root: "public".into(),
            project_type: ProjectType::Static,
            service_id: Some(svc.id.clone()),
            strategy: Strategy::Static,
            upstream_port: None,
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        };
        let site = core.sites.create(&input).unwrap();
        core.start_service(&svc.id).unwrap();
        assert!(!listening(https), "{component}: no TLS listener without an HTTPS site");

        // Switching HTTPS on restarts the server with its TLS listener.
        input.https_enabled = true;
        input.https_redirect = true;
        let view = core.sites.update(&site.site.id, &input).unwrap();
        assert_eq!(view.url, format!("https://{domain}:{https}/"));
        assert!(listening(https), "{component}: TLS listener after enabling HTTPS");
        let (ok, body) = curl_https(&ca.cert_path, &domain, https);
        assert!(ok && body.contains("secure static"), "{component}: {body}");
        let redirect = http_get(http, &domain, "/page");
        assert!(redirect.starts_with("HTTP/1.1 301"), "{component}: {redirect}");
        assert!(redirect.contains(&format!("https://{domain}:{https}/page")), "{component}: {redirect}");
        // Unknown names get the default certificate (localhost), which curl rejects for them.
        let (ok, _) = curl_https(&ca.cert_path, &format!("other-{component}.localhost"), https);
        assert!(!ok, "{component}: the site certificate is not served for other names");
        println!("{component}: https://{domain}:{https}/ verified against the Zenv CA");

        // Switching it off again frees the TLS port.
        input.https_enabled = false;
        core.sites.update(&site.site.id, &input).unwrap();
        assert!(!listening(https), "{component}: TLS listener removed");
        assert!(http_get(http, &domain, "/").contains("secure static"));

        core.services.stop(&svc.id).unwrap();
        core.sites.delete(&site.site.id).unwrap();
    }
    core.shutdown();
}
