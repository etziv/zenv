//! Several versions of the same component side by side, without conflicts:
//! Go versions installed by Zenv from go.dev next to an external Go, two MySQL
//! versions running at once, two Redis builds running at once.
//!
//!   ZENV_TEST_NETWORK=1                      (downloads two Go releases)
//!   ZENV_TEST_GO_EXTERNAL=C:\Program Files\Go
//!   ZENV_TEST_MYSQL_ROOTS=C:\...\mysql-9.7.0-winx64;C:\...\mysql-9.3.0-winx64
//!   ZENV_TEST_REDIS=C:\...\redis-8.8.0\...;C:\...\redis-5.0.14
//!   npm run test:e2e

use std::path::{Path, PathBuf};
use std::process::Command;

use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_env;
use zenv_core::runtime_manager::{self, InstallSource, Installation};
use zenv_core::service_manager::{Health, Service, ServiceInput};

/// Runs `go <args>` the way a Zenv terminal or project process would.
fn go_with(install: &Installation, args: &[&str]) -> String {
    let env = runtime_env::build(std::slice::from_ref(install), std::env::var_os("PATH"));
    let mut cmd = Command::new("go");
    cmd.args(args).env_clear();
    for key in ["SystemRoot", "TEMP", "TMP", "USERPROFILE", "LOCALAPPDATA", "APPDATA"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    cmd.envs(env.iter().map(|(k, v)| (k, v)));
    // Resolve `go` through the environment's PATH, like a shell would.
    let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
    let exe = std::env::split_paths(&path).map(|d| d.join("go.exe")).find(|p| p.is_file()).expect("go on PATH");
    let out = Command::new(exe).args(args).envs(env.iter().map(|(k, v)| (k, v))).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn db_service(core: &Core, name: &str, inst: &Installation, dir: &Path) -> Service {
    core.services
        .create(&ServiceInput {
            name: name.into(),
            installation_id: inst.id.clone(),
            listen_address: "127.0.0.1".into(),
            http_port: port_manager::suggest_free_port().unwrap(),
            autostart: false,
            data_dir: Some(dir.display().to_string()),
            https_port: None,
        })
        .unwrap()
}

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1, ZENV_TEST_GO_EXTERNAL, ZENV_TEST_MYSQL_ROOTS and ZENV_TEST_REDIS"]
fn versions_coexist_without_conflicts() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    assert_eq!(env("ZENV_TEST_NETWORK"), "1");
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();

    // ----- Go: two versions installed by Zenv + the existing external one ----
    let offers = core.installer.available(ComponentId::Go).unwrap();
    let newest = offers.iter().find(|o| o.stable).unwrap().version.clone();
    let newest_minor: String = newest.split('.').take(2).collect::<Vec<_>>().join(".");
    let older = offers
        .iter()
        .find(|o| o.stable && !o.version.starts_with(&format!("{newest_minor}.")) && o.version != newest_minor)
        .unwrap()
        .version
        .clone();
    println!("installing Go {newest} and {older} from go.dev");
    let go_new = core.installer.install_blocking(ComponentId::Go, &newest).expect("install newest Go");
    let go_old = core.installer.install_blocking(ComponentId::Go, &older).expect("install older Go");
    for g in [&go_new, &go_old] {
        assert_eq!(g.source, InstallSource::Managed);
        assert!(g.root_path.starts_with(home.path().join("bin").join("go")), "{}", g.root_path.display());
    }
    assert!(core.installer.install_blocking(ComponentId::Go, &newest).is_err(), "same version twice is refused");

    let external = runtime_manager::register(&core.db, ComponentId::Go, Path::new(&env("ZENV_TEST_GO_EXTERNAL"))).unwrap();
    assert_eq!(external.source, InstallSource::External);
    // Discovery sees the Zenv home and the system install.
    let found = runtime_manager::discover(&core.db, Some(ComponentId::Go), &[home.path().join("bin")]).unwrap();
    assert!(found.iter().filter(|f| f.registered_id.is_some()).count() >= 3, "{found:?}");

    for g in [&go_new, &go_old, &external] {
        let version = go_with(g, &["version"]);
        let goroot = PathBuf::from(go_with(g, &["env", "GOROOT"]));
        println!("{:<9} {version}  GOROOT={}", format!("{:?}", g.source), goroot.display());
        assert!(version.contains(&format!("go{} ", g.version)), "{version}");
        assert_eq!(dunce::canonicalize(&goroot).unwrap(), dunce::canonicalize(&g.root_path).unwrap());
    }

    // ----- Two MySQL versions at the same time ------------------------------
    let roots: Vec<String> = env("ZENV_TEST_MYSQL_ROOTS").split(';').map(str::to_string).collect();
    let mut mysqls = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        let inst = runtime_manager::register(&core.db, ComponentId::Mysql, Path::new(root.trim())).unwrap();
        let svc = db_service(&core, &format!("MySQL {}", inst.version), &inst, &home.path().join("data").join(format!("mysql-{i}")));
        core.services.initialize_data_dir(&svc.id).unwrap();
        core.start_service(&svc.id).unwrap_or_else(|e| panic!("MySQL {}: {e}", inst.version));
        mysqls.push((inst, svc));
    }
    for (inst, svc) in &mysqls {
        assert_eq!(core.services.status(&svc.id).unwrap().health, Health::Running);
        let hello = core.services.probe(&svc.id).unwrap();
        assert_eq!(hello, format!("MySQL {}", inst.version), "each port answers with its own version");
    }

    // ----- Two Redis builds at the same time --------------------------------
    let mut redises = Vec::new();
    for (i, root) in env("ZENV_TEST_REDIS").split(';').enumerate() {
        let inst = runtime_manager::register(&core.db, ComponentId::Redis, Path::new(root.trim())).unwrap();
        let svc = db_service(&core, &format!("Redis {}", inst.version), &inst, &home.path().join("data").join(format!("redis-{i}")));
        core.start_service(&svc.id).unwrap();
        redises.push(svc);
    }
    for svc in &redises {
        assert!(core.services.probe(&svc.id).unwrap().contains("PONG"));
    }
    // Everything above is still running together.
    for (_, svc) in &mysqls {
        assert_eq!(core.services.status(&svc.id).unwrap().health, Health::Running);
    }

    core.shutdown();

    // Removing a Zenv-installed version deletes its folder; the external one stays.
    core.installer.uninstall(&go_old.id).unwrap();
    assert!(!go_old.root_path.exists());
    assert!(external.executable_path.is_file());
}
