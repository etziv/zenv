//! Installs the newest MySQL and PostgreSQL from their official sources into a
//! throwaway Zenv home, then initializes a data folder, starts each server and
//! talks to it. MySQL is checked by Oracle's code signature on every program
//! and library (Oracle publishes no usable checksum); PostgreSQL by the SHA-256
//! Scoop recorded for the same zip (EDB publishes none).
//!
//!   ZENV_TEST_NETWORK=1   (downloads about 750 MB)
//!   npm run test:e2e

use std::time::Instant;

use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager::InstallSource;
use zenv_core::service_manager::ServiceInput;

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn mysql_and_postgresql_install_and_run() {
    assert_eq!(std::env::var("ZENV_TEST_NETWORK").as_deref(), Ok("1"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();

    for component in [ComponentId::Mysql, ComponentId::Postgresql] {
        let offers = core.installer.available(component).unwrap_or_else(|e| panic!("{component}: {e}"));
        println!("{component}: {}", offers.iter().map(|o| format!("{}{} ({} MB)", o.version, if o.stable { "*" } else { "" }, o.size / 1_048_576)).collect::<Vec<_>>().join(", "));
        assert!(!offers.is_empty(), "{component}: no offers");
        assert!(offers.iter().all(|o| !o.verified_by.is_empty()));
        let offer = &offers[0];
        let started = Instant::now();
        let inst = core.installer.install_blocking(component, &offer.version).unwrap_or_else(|e| panic!("{component} {}: {e}", offer.version));
        println!("  installed {} in {:.0?} -> {} (verified by: {})", inst.version, started.elapsed(), inst.root_path.display(), offer.verified_by);
        assert_eq!(inst.source, InstallSource::Managed);
        if component == ComponentId::Postgresql {
            assert!(offer.sha256.len() == 64, "the newest PostgreSQL has an independent checksum ({})", offer.verified_by);
            assert!(!inst.root_path.join("pgAdmin 4").exists() && !inst.root_path.join("StackBuilder").exists(), "bundled tools left out");
            assert!(inst.root_path.join("bin").join("initdb.exe").is_file());
        }

        let port = port_manager::suggest_free_port().unwrap();
        let svc = core
            .services
            .create(&ServiceInput {
                name: format!("{component} test"),
                installation_id: inst.id.clone(),
                listen_address: "127.0.0.1".into(),
                http_port: port,
                autostart: false,
                data_dir: Some(home.path().join("data").join(component.as_str()).display().to_string()),
                https_port: None,
            })
            .unwrap();
        println!("  init: {}", core.services.initialize_data_dir(&svc.id).unwrap_or_else(|e| panic!("{component} init: {e}")).lines().next().unwrap_or(""));
        core.start_service(&svc.id).unwrap_or_else(|e| panic!("{component} start: {e}"));
        let answer = core.services.probe(&svc.id).unwrap_or_else(|e| panic!("{component} probe: {e}"));
        println!("  probe: {answer}");
        core.services.stop(&svc.id).unwrap();
    }
    core.shutdown();
}
