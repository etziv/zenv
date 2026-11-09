//! AI routers as Zenv services: installs the release before the newest from
//! the npm registry (with Zenv's Node.js), runs it on 127.0.0.1 only, then
//! lets the updater move the service to the newest release.
//!
//!   ZENV_TEST_NETWORK=1   (9router: about 100 MB; OmniRoute: about 1 GB)
//!   npm run test:e2e

use std::time::Instant;

use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::service_manager::ServiceInput;

fn listeners(port: u16) -> Vec<String> {
    let out = std::process::Command::new("netstat").args(["-ano", "-p", "TCP"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("LISTENING") && l.split_whitespace().nth(1).is_some_and(|a| a.ends_with(&format!(":{port}"))))
        .map(|l| l.split_whitespace().nth(1).unwrap_or("").to_string())
        .collect()
}

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn ninerouter_runs_locally_and_updates() {
    router_runs_locally_and_updates(ComponentId::Ninerouter);
}

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn omniroute_runs_locally_and_updates() {
    router_runs_locally_and_updates(ComponentId::Omniroute);
}

fn router_runs_locally_and_updates(component: ComponentId) {
    assert_eq!(std::env::var("ZENV_TEST_NETWORK").as_deref(), Ok("1"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    // Zenv's own Node.js (newest LTS) satisfies both routers.
    let node = core.installer.available(ComponentId::Node).unwrap().into_iter().find(|o| o.stable).unwrap();
    core.installer.install_blocking(ComponentId::Node, &node.version).unwrap_or_else(|e| panic!("node: {e}"));
    println!("node {} installed", node.version);
    let offers = core.installer.available(component).unwrap();
    println!("offers: {}", offers.iter().map(|o| o.version.as_str()).collect::<Vec<_>>().join(", "));
    let older = &offers[1];

    let t = Instant::now();
    let inst = core.installer.install_blocking(component, &older.version).unwrap_or_else(|e| panic!("install {}: {e}", older.version));
    println!("installed {} in {:.0?}", inst.version, t.elapsed());

    let port = port_manager::suggest_free_port().unwrap();
    let svc = core
        .services
        .create(&ServiceInput { name: "Router".into(), installation_id: inst.id.clone(), listen_address: "127.0.0.1".into(), http_port: port, autostart: false, data_dir: None, https_port: None })
        .unwrap();
    let data_dir = core.services.get(&svc.id).unwrap().data_dir.unwrap();
    println!("data folder: {data_dir:?}");
    let t = Instant::now();
    core.start_service(&svc.id).unwrap_or_else(|e| panic!("start: {e}"));
    println!("started in {:.0?}; probe: {:?}", t.elapsed(), core.services.probe(&svc.id));
    let addrs = listeners(port);
    println!("listening on {addrs:?}");
    assert!(!addrs.is_empty() && addrs.iter().all(|a| a.starts_with("127.0.0.1:")), "only this computer: {addrs:?}");
    assert!(std::fs::read_dir(&data_dir).map(|mut d| d.next().is_some()).unwrap_or(false), "data written to the service's folder");
    let url = core.router_url(&svc.id, Some("providers")).unwrap();
    assert!(url.ends_with("/dashboard/providers"), "{url}");

    let t = Instant::now();
    let status = core.router_update(component, true).unwrap_or_else(|e| panic!("update: {e}"));
    println!("update in {:.0?}: installed={:?} latest={:?} error={:?}", t.elapsed(), status.installed, status.latest, status.last_error);
    assert_eq!(status.installed.as_deref(), Some(offers[0].version.as_str()));
    let now = core.services.get(&svc.id).unwrap();
    let on = zenv_core::runtime_manager::get(&core.db, &now.installation_id).unwrap();
    assert_eq!(on.version, offers[0].version, "service moved to the new release");
    assert!(core.services.is_running(&svc.id), "restarted on it");
    println!("probe after update: {:?}", core.services.probe(&svc.id));
    assert!(!inst.root_path.exists(), "old release removed");
    core.shutdown();
}
