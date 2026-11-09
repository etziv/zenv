//! Task 3: `follow_tailnet_names` never wipes the saved tailnet names when
//! Tailscale cannot be asked (not installed / offline), with or without an app
//! open on the tailnet; the panel read (`tailnet_apps`) keeps them too.
//! Tailscale is hidden from this test process (PATH and Program Files point at
//! an empty folder), so nothing here touches the real tailnet.

use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;

const STALE: &str = "desktop-test desktop-test.tail1234.ts.net 100.64.0.1";

fn saved(core: &Core) -> String {
    core.db.read(|c| Ok(c.query_row("SELECT value FROM app_settings WHERE key = 'tailnet.hosts'", [], |r| r.get::<_, String>(0))?)).unwrap()
}

#[test]
fn offline_tailscale_keeps_the_saved_names() {
    let empty = tempfile::tempdir().unwrap();
    std::env::set_var("PATH", empty.path());
    std::env::set_var("ProgramFiles", empty.path());
    std::env::set_var("ProgramW6432", empty.path());
    assert!(!zenv_core::tailnet::status().online, "Tailscale must look offline here");

    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();
    core.db
        .write(|tx| {
            tx.execute("INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('tailnet.hosts', ?1, 't')", [STALE])?;
            Ok(())
        })
        .unwrap();

    // No app open on the tailnet: a no-op.
    core.follow_tailnet_names().unwrap();
    assert_eq!(saved(&core), STALE, "names changed with no app open");

    // An app open on the tailnet, Tailscale offline: names kept.
    let site = core
        .sites
        .create(&SiteInput {
            name: "qa.localhost".into(),
            domain: "qa.localhost".into(),
            project_root: projects.path().display().to_string(),
            document_root: projects.path().display().to_string(),
            project_type: ProjectType::Custom,
            service_id: None,
            strategy: Strategy::ReverseProxy,
            upstream_port: Some(65000),
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
        })
        .unwrap();
    core.db
        .write(|tx| {
            tx.execute("INSERT INTO tailnet_apps (site_id, port, service, created_at) VALUES (?1, 9123, 'qasvc', 't')", [&site.site.id])?;
            Ok(())
        })
        .unwrap();
    core.follow_tailnet_names().unwrap();
    assert_eq!(saved(&core), STALE, "offline Tailscale wiped the names (background follow)");
    let apps = core.tailnet_apps().unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(saved(&core), STALE, "offline Tailscale wiped the names (panel read)");
    core.shutdown();
}
