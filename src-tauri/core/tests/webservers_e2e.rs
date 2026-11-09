//! Installs the newest stable Apache (Apache Lounge), Nginx, Caddy and
//! FrankenPHP from their official Windows sources into a throwaway Zenv home,
//! then checks Zenv can configure and run them.
//!
//!   ZENV_TEST_NETWORK=1
//!   npm run test:e2e

use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager::{self, InstallSource};
use zenv_core::service_manager::ServiceInput;

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn web_servers_install_from_official_sources() {
    assert_eq!(std::env::var("ZENV_TEST_NETWORK").as_deref(), Ok("1"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();

    for c in [ComponentId::Apache, ComponentId::Nginx, ComponentId::Caddy, ComponentId::Frankenphp] {
        let offers = core.installer.available(c).unwrap_or_else(|e| panic!("{c}: {e}"));
        assert!(offers.iter().all(|o| o.sha256.len() == 64), "{c}: every offer has a SHA-256");
        let version = offers.iter().find(|o| o.stable).unwrap_or_else(|| panic!("{c}: no stable offer")).version.clone();
        let started = std::time::Instant::now();
        let inst = core.installer.install_blocking(c, &version).unwrap_or_else(|e| panic!("{c} {version}: {e}"));
        println!("{c} {version} installed in {:.0?} -> {}", started.elapsed(), inst.executable_path.display());
        assert_eq!((inst.source, inst.version.as_str()), (InstallSource::Managed, version.as_str()));
        assert!(inst.root_path.starts_with(home.path().join("bin")));

        if matches!(c, ComponentId::Apache | ComponentId::Nginx) {
            let svc = core
                .services
                .create(&ServiceInput {
                    name: format!("{c} test"),
                    installation_id: inst.id.clone(),
                    listen_address: "127.0.0.1".into(),
                    http_port: port_manager::suggest_free_port().unwrap(),
                    autostart: false,
                    data_dir: None,
                    https_port: None,
                })
                .unwrap();
            let report = core.services.validate(&svc.id).unwrap();
            assert!(report.ok, "{c}: {}", report.output);
            core.start_service(&svc.id).unwrap_or_else(|e| panic!("{c} start: {e}"));
            println!("  {c} started on :{}", svc.http_port);
            core.services.stop(&svc.id).unwrap();
        }
        if c == ComponentId::Frankenphp {
            let php = runtime_manager::list(&core.db, Some(ComponentId::Php)).unwrap();
            assert!(php.iter().any(|p| p.root_path == inst.root_path), "FrankenPHP's own PHP is registered: {php:?}");
            println!("  embedded PHP {}", php[0].version);
        }
    }
    core.shutdown();
}
