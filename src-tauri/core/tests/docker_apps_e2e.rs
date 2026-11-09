//! Docker apps against the real Docker engine: a plain container and a
//! compose service (both from the local `redis:7-alpine` image, so nothing is
//! downloaded) are added as apps, started and stopped through Zenv, and
//! removed again. The user's own containers are not touched.
//!
//!   ZENV_TEST_DOCKER=1   (Docker Desktop installed; started if needed)
//!   npm run test:e2e

use std::path::Path;
use std::process::Command;

use zenv_core::app_core::Core;
use zenv_core::docker_apps::{docker_exe, ComposeService, DockerBinding};
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;

fn docker(args: &[&str]) -> String {
    let out = Command::new(docker_exe().unwrap()).args(args).output().unwrap();
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn input(name: &str, port: u16) -> SiteInput {
    SiteInput {
        name: name.into(),
        domain: format!("{name}.test"),
        project_root: String::new(),
        document_root: String::new(),
        project_type: ProjectType::Custom,
        service_id: None,
        strategy: Strategy::ReverseProxy,
        upstream_port: Some(port),
        runtime_installation_id: None,
        php_engine: PhpEngine::Auto,
        https_enabled: false,
        https_redirect: false,
        enabled: true,
    }
}

fn wait_state(core: &Core, site: &str, want: &str) -> String {
    let mut last = String::new();
    for _ in 0..40 {
        last = core.app_process_status(site).unwrap().state.to_string();
        if last == want {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    last
}

#[test]
#[ignore = "requires ZENV_TEST_DOCKER=1"]
fn containers_start_and_stop_with_their_apps() {
    assert_eq!(std::env::var("ZENV_TEST_DOCKER").as_deref(), Ok("1"));
    zenv_core::docker_apps::ensure_engine().unwrap();
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    let tag = std::process::id();

    // 1. A plain container.
    let name = format!("zenv-e2e-plain-{tag}");
    let port = port_manager::suggest_free_port().unwrap();
    println!("{}", docker(&["create", "--name", &name, "-p", &format!("127.0.0.1:{port}:6379"), "redis:7-alpine"]).trim());
    let listed = core.docker_containers().unwrap();
    let mine = listed.iter().find(|c| c.name == name).expect("listed");
    assert_eq!(mine.ports.first().map(|p| (p.host_port, p.container_port)), Some((port, 6379)));

    let binding = DockerBinding { container: name.clone(), compose: None, autostart: true };
    let view = core.create_docker_app(&input("plaine2e", port), &binding).unwrap();
    assert_eq!(view.site.project_root, core.paths.dev_dir, "no compose folder: Zenv's dev folder");
    let id = view.site.id.clone();
    assert_eq!(core.app_process_status(&id).unwrap().state, "stopped");
    assert!(core.start_app_process(&id).unwrap(), "started");
    assert_eq!(wait_state(&core, &id, "running"), "running");
    let st = core.app_process_status(&id).unwrap();
    println!("plain: {:?} {:?}", st.command, st.docker.as_ref().map(|d| &d.state));
    assert!(!core.start_app_process(&id).unwrap(), "already running");
    core.set_app_command(&id, None, false).unwrap();
    assert!(!core.app_process_status(&id).unwrap().autostart);
    assert!(core.autostart_apps(None).unwrap().iter().all(|s| s.id != id), "no autostart now");
    core.stop_app_process(&id).unwrap();
    assert_eq!(wait_state(&core, &id, "exited"), "exited");

    // The output log is the container's log.
    core.refresh_docker_log(&id).unwrap();
    let log = std::fs::read_to_string(zenv_core::app_process::log_file(&core.paths, &id)).unwrap();
    assert!(log.contains("Ready to accept connections"), "{log}");

    // 2. A compose service, started through compose (and "Stop all").
    let project = format!("zenve2e{tag}");
    let dir = home.path().join("compose");
    std::fs::create_dir_all(&dir).unwrap();
    let cport = port_manager::suggest_free_port().unwrap();
    let file = dir.join("docker-compose.yml");
    std::fs::write(&file, format!("services:\n  cache:\n    image: redis:7-alpine\n    ports:\n      - \"127.0.0.1:{cport}:6379\"\n")).unwrap();
    let f = file.display().to_string();
    println!("{}", docker(&["compose", "-p", &project, "-f", &f, "create"]).trim());
    let listed = core.docker_containers().unwrap();
    let c = listed.iter().find(|c| c.compose.as_ref().is_some_and(|x| x.project == project)).expect("compose container listed");
    let compose = c.compose.clone().unwrap();
    assert_eq!((compose.service.as_str(), compose.dir.as_deref().map(Path::new)), ("cache", Some(dir.as_path())));
    let binding = DockerBinding {
        container: c.name.clone(),
        compose: Some(ComposeService { files: vec![f.clone()], ..compose }),
        autostart: true,
    };
    let view = core.create_docker_app(&input("composee2e", cport), &binding).unwrap();
    assert_eq!(view.site.project_root, dir, "the compose project's folder");
    let cid = view.site.id.clone();
    assert!(core.start_app_process(&cid).unwrap());
    assert_eq!(wait_state(&core, &cid, "running"), "running");
    assert!(core.app_process_status(&cid).unwrap().command.unwrap().starts_with("docker compose"));
    core.stop_all_services().unwrap();
    assert_eq!(wait_state(&core, &cid, "exited"), "exited", "Stop all stops containers Zenv started");

    // Deleting the app leaves the container alone.
    core.sites.delete(&cid).unwrap();
    assert!(core.docker_binding(&cid).unwrap().is_none());
    assert!(core.docker_containers().unwrap().iter().any(|x| x.name == c.name));

    println!("{}", docker(&["rm", "-f", &name]).trim());
    println!("{}", docker(&["compose", "-p", &project, "-f", &f, "down"]).trim());
    core.shutdown();
}
