use std::time::Duration;

use super::*;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{probe, InstallSource};
use crate::service_manager::ServiceInput;

/// A minimal PHP site record for checks that only read files.
pub(crate) fn sample_site(project_root: &Path, document_root: &Path) -> Site {
    Site {
        id: "s1".into(),
        name: "shop".into(),
        domain: "shop.test".into(),
        project_root: project_root.to_path_buf(),
        document_root: document_root.to_path_buf(),
        project_type: ProjectType::Laravel,
        service_id: None,
        strategy: Strategy::Php,
        upstream_port: None,
        runtime_installation_id: Some("php".into()),
        php_engine: PhpEngine::Auto,
        https_enabled: false,
        https_redirect: false,
        enabled: true,
        config_status: ConfigStatus::Applied,
        config_message: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    sites: SiteManager,
    service_id: String,
    db: Arc<Db>,
}

/// A service whose Apache executable does not exist, so every attempt to
/// activate configuration fails (exercises rollback without Apache).
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path().join("data"));
    paths.ensure().unwrap();
    let db = Arc::new(Db::open_in_memory().unwrap());
    let install = runtime_manager::insert(
        &db,
        ComponentId::Apache,
        &probe::Located { root: dir.path().join("Apache24"), executable: dir.path().join("Apache24/bin/httpd.exe") },
        &probe::VersionInfo { version: "2.4.67".into(), detail: String::new() },
        InstallSource::External,
    )
    .unwrap();
    let services = Arc::new(ServiceManager::new(db.clone(), paths.clone(), Arc::new(crate::tls::Tls::new(&paths))));
    let runtimes = Arc::new(PhpRuntimes::new(db.clone(), paths.clone()));
    let svc = services
        .create(&ServiceInput {
            name: "Apache".into(),
            installation_id: install.id,
            listen_address: "127.0.0.1".into(),
            http_port: 8080,
            autostart: false, data_dir: None, https_port: None
        })
        .unwrap();
    Fixture { sites: SiteManager::new(db.clone(), paths, services, runtimes), service_id: svc.id, db, dir }
}

fn project(f: &Fixture, name: &str) -> PathBuf {
    let root = f.dir.path().join("projects").join(name);
    std::fs::create_dir_all(root.join("public")).unwrap();
    root
}

fn input(f: &Fixture, root: &Path, domain: &str) -> SiteInput {
    SiteInput {
        name: "Shop".into(),
        domain: domain.into(),
        project_root: root.display().to_string(),
        document_root: "public".into(),
        project_type: ProjectType::Static,
        service_id: Some(f.service_id.clone()),
        strategy: Strategy::Static,
        upstream_port: None,
        runtime_installation_id: None,
        php_engine: PhpEngine::Auto,        https_enabled: false,
        https_redirect: false,
        enabled: true,
    }
}

fn raw_insert(f: &Fixture, i: usize, root: &Path) {
    f.db.write(|tx| {
        tx.execute(
            "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
             enabled, config_status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4, 'static', ?5, 'static', 1, 'applied', 't', 't')",
            params![format!("id{i}"), format!("Site {i:05}"), format!("site{i}.test"), root.display().to_string(), f.service_id],
        )?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn validation_explains_each_problem() {
    let f = fixture();
    let root = project(&f, "shop");
    let ok = f.sites.validate(&input(&f, &root, "Shop.Test"), None).unwrap();
    assert_eq!(ok.domain, "shop.test");
    assert!(ok.document_root.ends_with("public"), "relative document root resolved against the project");

    let mut bad = input(&f, &root, "shop.test");
    bad.document_root = f.dir.path().display().to_string();
    assert!(f.sites.validate(&bad, None).unwrap_err().to_string().contains("inside the project folder"));

    let mut bad = input(&f, &root, "shop.test");
    bad.document_root = "missing".into();
    assert!(matches!(f.sites.validate(&bad, None), Err(AppError::InvalidPath { .. })));

    let mut bad = input(&f, &root, "shop.test");
    bad.strategy = Strategy::ReverseProxy;
    assert!(f.sites.validate(&bad, None).unwrap_err().to_string().contains("needs the port"));
    bad.upstream_port = Some(8080);
    assert!(f.sites.validate(&bad, None).unwrap_err().to_string().contains("web server's own port"));
    bad.upstream_port = Some(3000);
    assert_eq!(f.sites.validate(&bad, None).unwrap().input.upstream_port, Some(3000));

    let mut bad = input(&f, &root, "shop.dev");
    assert!(f.sites.validate(&bad, None).unwrap_err().to_string().contains("HTTPS"));
    bad.domain = "ok.test".into();
    bad.project_root = "relative/path".into();
    assert!(f.sites.validate(&bad, None).unwrap_err().to_string().contains("full path"));
}

#[test]
fn domains_are_unique_regardless_of_case() {
    let f = fixture();
    let root = project(&f, "shop");
    raw_insert(&f, 1, &root);
    let err = f.sites.validate(&input(&f, &root, "SITE1.test"), None).unwrap_err();
    assert!(err.to_string().contains("already used by site 'Site 00001'"), "{err}");
    assert!(f.sites.validate(&input(&f, &root, "site1.test"), Some("id1")).is_ok(), "editing itself is fine");
}

#[test]
fn failed_activation_rolls_back_the_new_record() {
    let f = fixture();
    let root = project(&f, "shop");
    let err = f.sites.create(&input(&f, &root, "shop.test")).unwrap_err();
    assert!(matches!(err, AppError::InvalidPath { .. }), "missing httpd.exe: {err}");
    assert_eq!(f.sites.list(&SiteQuery::default()).unwrap().total, 0);
    assert!(root.join("public").is_dir(), "project untouched");
}

#[test]
fn failed_activation_restores_deleted_and_toggled_sites() {
    let f = fixture();
    let root = project(&f, "shop");
    raw_insert(&f, 1, &root);
    assert!(f.sites.delete("id1").is_err());
    assert_eq!(f.sites.get("id1").unwrap().domain, "site1.test", "delete rolled back");
    assert!(f.sites.set_enabled("id1", false).is_err());
    assert!(f.sites.get("id1").unwrap().enabled, "toggle rolled back");
}

#[test]
fn duplicate_picks_a_free_domain_and_starts_disabled() {
    let f = fixture();
    let root = project(&f, "shop");
    raw_insert(&f, 1, &root);
    let a = f.sites.duplicate("id1").unwrap();
    let b = f.sites.duplicate("id1").unwrap();
    assert_eq!((a.site.domain.as_str(), a.site.enabled), ("site1-copy.test", false));
    assert_eq!(b.site.domain, "site1-copy-2.test");
    assert_eq!(a.site.name, "Site 00001 (copy)");
}

#[test]
fn large_collections_page_and_search_quickly() {
    let f = fixture();
    let root = project(&f, "shop");
    f.db.write(|tx| {
        let mut stmt = tx.prepare(
            "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
             enabled, config_status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4, 'static', ?5, 'static', 1, 'applied', 't', 't')",
        )?;
        for i in 0..5000 {
            stmt.execute(params![format!("id{i}"), format!("Site {i:05}"), format!("site{i}.test"), root.display().to_string(), f.service_id])?;
        }
        Ok(())
    })
    .unwrap();

    let started = std::time::Instant::now();
    let page = f.sites.list(&SiteQuery { search: String::new(), offset: 4950, limit: 50 }).unwrap();
    assert_eq!((page.total, page.items.len()), (5000, 50));
    assert_eq!(page.items[0].site.name, "Site 04950");
    let found = f.sites.list(&SiteQuery { search: "site4999.".into(), offset: 0, limit: 50 }).unwrap();
    assert_eq!(found.total, 1);
    let wildcard = f.sites.list(&SiteQuery { search: "%".into(), offset: 0, limit: 10 }).unwrap();
    assert_eq!(wildcard.total, 0, "LIKE wildcards in the search are literal");
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
    assert_eq!(f.sites.desired_hosts_entries().unwrap().len(), 5000);
}

#[test]
fn hosts_entries_skip_localhost_names_and_disabled_sites() {
    let f = fixture();
    let root = project(&f, "shop");
    raw_insert(&f, 1, &root);
    raw_insert(&f, 2, &root);
    f.db.write(|tx| {
        tx.execute("UPDATE sites SET domain = 'api.localhost' WHERE id = 'id1'", [])?;
        tx.execute("UPDATE sites SET enabled = 0 WHERE id = 'id2'", [])?;
        Ok(())
    })
    .unwrap();
    raw_insert(&f, 3, &root);
    assert_eq!(
        f.sites.desired_hosts_entries().unwrap(),
        vec![HostsEntry { ip: "127.0.0.1".into(), domain: "site3.test".into() }]
    );
}

#[test]
fn services_with_sites_cannot_be_deleted() {
    let f = fixture();
    let root = project(&f, "shop");
    raw_insert(&f, 1, &root);
    let err = f.sites.services.delete(&f.service_id).unwrap_err();
    assert!(err.to_string().contains("local site"), "{err}");
}
