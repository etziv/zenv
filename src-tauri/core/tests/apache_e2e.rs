//! First end-to-end acceptance test (see docs/ARCHITECTURE.md).
//!
//! Needs a real Apache for Windows installation:
//!   ZENV_TEST_APACHE_ROOT=C:\path\to\Apache24 npm run test:e2e

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::error::AppError;
use zenv_core::paths::AppPaths;
use zenv_core::platform;
use zenv_core::port_manager::{self, PortStatus};
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager;
use zenv_core::service_manager::{Health, ServiceInput};

fn http_get(port: u16, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
#[ignore = "requires ZENV_TEST_APACHE_ROOT pointing at an Apache for Windows installation"]
fn apache_service_end_to_end() {
    let apache_root = PathBuf::from(
        std::env::var_os("ZENV_TEST_APACHE_ROOT").expect("set ZENV_TEST_APACHE_ROOT"),
    );
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    let data = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).expect("fresh core with migrated database");

    // 1–2. Register a real installation; the version comes from the executable itself.
    let install = runtime_manager::register(&core.db, ComponentId::Apache, &apache_root).expect("register apache");
    println!("registered {} {} ({})", install.component, install.version, install.version_detail);
    assert!(install.version.starts_with("2.4."), "unexpected version {}", install.version);
    assert!(install.is_default);

    // 3. A service on a free port; generated config passes Apache's own checker.
    let port = port_manager::suggest_free_port().unwrap();
    let service = core
        .services
        .create(&ServiceInput {
            name: "E2E Apache".into(),
            installation_id: install.id.clone(),
            listen_address: "127.0.0.1".into(),
            http_port: port,
            autostart: false, data_dir: None, https_port: None
        })
        .unwrap();
    let report = core.services.validate(&service.id).unwrap();
    assert!(report.ok, "httpd -t failed:\n{}", report.output);
    assert!(report.output.contains("Syntax OK"));

    // 4. Start: running, PIDs reported, port owned by a job process, page served.
    let status = core.services.start(&service.id).expect("start");
    assert_eq!(status.health, Health::Running);
    let ProcessState::Running { pid, pids, .. } = status.process.clone() else {
        panic!("not running: {:?}", status.process)
    };
    println!("apache running: pid {pid}, job pids {pids:?}, {}", status.url);
    match port_manager::inspect(port, loopback).unwrap().status {
        PortStatus::InUse { owners } => {
            assert!(owners.iter().any(|o| pids.contains(&o.pid)), "listener {owners:?} not in job {pids:?}")
        }
        other => panic!("port not in use after start: {other:?}"),
    }
    let response = http_get(port, "/");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains(r#"data-zenv="default-index""#));

    // A second start is refused rather than spawning a duplicate.
    assert!(matches!(core.services.start(&service.id), Err(AppError::Conflict(_))));

    // Graceful reload keeps serving.
    core.services.reload(&service.id).expect("reload");
    std::thread::sleep(Duration::from_secs(2));
    assert!(http_get(port, "/").starts_with("HTTP/1.1 200"));

    // 5. Graceful stop: exited, port released, whole tree gone, logs readable.
    let stopped = core.services.stop(&service.id).expect("stop");
    assert_eq!(stopped.health, Health::Stopped);
    let events = core.services.events(&service.id, 20).unwrap();
    assert!(
        events.iter().any(|e| e.message == "stopped gracefully"),
        "expected graceful stop, events: {events:?}"
    );
    assert!(port_manager::ensure_available(port, loopback).is_ok(), "port still held after stop");
    for p in &pids {
        assert!(platform::process_image_name(*p).is_none(), "pid {p} survived stop");
    }
    let error_log = core.services.read_log(&service.id, "error", 50).unwrap();
    assert!(error_log.lines.iter().any(|l| l.contains("resuming normal operations")), "{:?}", error_log.lines);
    assert!(error_log.lines.iter().any(|l| l.contains("Received shutdown signal")), "{:?}", error_log.lines);
    let access_log = core.services.read_log(&service.id, "access", 50).unwrap();
    assert!(access_log.lines.iter().any(|l| l.contains("GET / HTTP/1.1")));

    // 6. Invalid configuration: useful diagnostics, start refused, active config untouched.
    let active_config = std::fs::read_to_string(&status.config_path).unwrap();
    let sites = data.path().join("etc").join(&service.id).join("sites");
    std::fs::write(sites.join("broken.conf"), "ThisIsNotAnApacheDirective on\n").unwrap();
    let bad = core.services.validate(&service.id).unwrap();
    assert!(!bad.ok);
    assert!(bad.output.contains("Invalid command"), "{}", bad.output);
    match core.services.start(&service.id) {
        Err(AppError::ConfigInvalid { output, .. }) => assert!(output.contains("broken.conf"), "{output}"),
        other => panic!("expected ConfigInvalid, got {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&status.config_path).unwrap(), active_config);
    assert_eq!(core.services.status(&service.id).unwrap().health, Health::Stopped);
    std::fs::remove_file(sites.join("broken.conf")).unwrap();

    // 7. Port conflict: refused before spawning, with the owning process named.
    let blocker = std::net::TcpListener::bind((loopback, port)).unwrap();
    match core.services.start(&service.id) {
        Err(AppError::PortUnavailable { owners, .. }) => assert_eq!(owners[0].pid, std::process::id()),
        other => panic!("expected PortUnavailable, got {other:?}"),
    }
    drop(blocker);

    // 8. Deleting the service and registration leaves the installation intact.
    core.services.delete(&service.id).unwrap();
    runtime_manager::unregister(&core.db, &install.id).unwrap();
    assert!(install.executable_path.is_file());
    assert!(apache_root.join("conf").join("httpd.conf").is_file());
}
