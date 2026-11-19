use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::diagnostics::{Check, CheckStatus};
use zenv_core::local_sites::{ConfigStatus, PhpEngine, ProjectType, Site, SitePage, SiteView, Strategy};
use zenv_core::local_sites::compat::ServerCompat;
use zenv_core::domain_manager::{hosts::DomainState, Resolution};
use zenv_core::log_manager::LogTail;
use zenv_core::php_runtime::RuntimeLogGroup;
use zenv_core::port_manager::{PortInspection, PortStatus};
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::service_manager::{Health, LogSource, Service, ServiceStatus};
use zenv_native::loader::Loader;
use zenv_native::pages::logs::{self, diagnostics, project_logs, service_logs};
use zenv_native::pages::{Action, Route};
use zenv_native::{shell, theme};

fn service(id: &str, name: &str, component: ComponentId, kind: Category, port: u16, health: Health) -> ServiceStatus {
    ServiceStatus {
        service: Service {
            id: id.into(),
            component,
            name: name.into(),
            installation_id: String::new(),
            listen_address: "127.0.0.1".into(),
            http_port: port,
            autostart: false,
            created_at: String::new(),
            updated_at: String::new(),
            data_dir: None,
            https_port: None,
        },
        installation: None,
        health,
        process: ProcessState::NotStarted,
        url: format!("http://127.0.0.1:{port}"),
        config_path: PathBuf::new(),
        document_root: PathBuf::new(),
        kind,
        data_dir_info: None,
        connection: None,
        behind: None,
        needs_initialization: false,
    }
}

fn site(id: &str, domain: &str, service_id: Option<&str>) -> SiteView {
    SiteView {
        site: Site {
            id: id.into(),
            name: domain.into(),
            domain: domain.into(),
            project_root: PathBuf::from(format!("C:/sites/{domain}")),
            document_root: PathBuf::from(format!("C:/sites/{domain}/public")),
            project_type: ProjectType::Php,
            service_id: service_id.map(str::to_owned),
            strategy: Strategy::Php,
            upstream_port: None,
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled: true,
            config_status: ConfigStatus::Applied,
            config_message: None,
            created_at: String::new(),
            updated_at: String::new(),
        },
        service_name: "Apache".into(),
        url: format!("http://{domain}/"),
        resolution: Resolution::Loopback,
        hosts_state: DomainState::NotNeeded,
        document_root_exists: true,
        php_version: None,
        php_backend: None,
        compat: ServerCompat::default(),
    }
}

fn log_source(id: &str, label: &str, exists: bool, size_bytes: u64) -> LogSource {
    LogSource { id: id.into(), label: label.into(), path: PathBuf::from(format!("C:/zenv/logs/{id}.log")), exists, size_bytes }
}

fn with_install(mut s: ServiceStatus, component: ComponentId, version: &str) -> ServiceStatus {
    s.installation = Some(zenv_core::runtime_manager::Installation {
        id: format!("{component}-{version}"),
        component,
        version: version.into(),
        version_detail: String::new(),
        root_path: PathBuf::from(format!("C:/zenv/bin/{component}/{version}")),
        executable_path: PathBuf::new(),
        source: zenv_core::runtime_manager::InstallSource::Managed,
        is_default: true,
        enabled: true,
        executable_exists: true,
        detected_at: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    });
    s
}

struct Fixture {
    tab: logs::Tab,
    projects: project_logs::State,
    services_state: service_logs::State,
    diagnostics: diagnostics::State,
    services: Option<Vec<ServiceStatus>>,
    actions: Vec<Action>,
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let mut out = Vec::new();
    let services = f.services.as_deref();
    let jump = logs::view(ui, f.tab, &mut f.projects, &mut f.services_state, &mut f.diagnostics, services, &mut out);
    if let Some(id) = jump {
        f.services_state.target = service_logs::Target::Service(id);
        f.tab = logs::Tab::Services;
    }
    f.actions.extend(out);
}

fn shell_and_page(ui: &mut egui::Ui, f: &mut Fixture) {
    let services = f.services.clone();
    let actions = shell::frame(ui, Route::Logs, "1.0.0", services.as_deref(), |ui, _| {
        zenv_native::widgets::page_header(ui, "Logs", "", |_ui| {});
        logs::tab_bar(ui, &mut f.tab);
        page(ui, f);
    });
    drop(actions);
}

fn log_lines(n: usize, label: &str) -> Vec<String> {
    (0..n).map(|i| format!("[2026-11-18 10:{:02}:00] {label} line {}: request handled", 10 + i, i + 1)).collect()
}

fn tail_at(path: &str, lines: Vec<String>) -> LogTail {
    LogTail { path: PathBuf::from(path), exists: !lines.is_empty(), size_bytes: 2048, lines, truncated: false }
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 1000.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

fn base_fixture(tab: logs::Tab) -> Fixture {
    Fixture {
        tab,
        projects: project_logs::State::default(),
        services_state: service_logs::State::default(),
        diagnostics: diagnostics::State::default(),
        services: Some(vec![]),
        actions: vec![],
    }
}

// ---------- Projects

#[test]
fn projects_empty_state_when_there_are_no_sites() {
    let mut f = base_fixture(logs::Tab::Projects);
    f.projects.sites = Loader::ready(Ok(SitePage { items: vec![], total: 0, offset: 0, limit: 200 }));
    let h = harness(f);
    h.get_by_label("No sites yet. Each site gets its own access, error and PHP log here, plus the app's log (such as Laravel's) when it has one.");
}

#[test]
fn projects_lists_sites_and_shows_the_selected_one() {
    let mut f = base_fixture(logs::Tab::Projects);
    f.projects.sites = Loader::ready(Ok(SitePage { items: vec![site("shop", "shop.test", None), site("blog", "blog.test", None)], total: 2, offset: 0, limit: 200 }));
    f.projects.sources = Loader::ready(Ok(vec![log_source("access", "Access", true, 512), log_source("error", "Error", false, 0)]));
    let h = harness(f);
    h.get_by_role_and_label(egui::Role::Button, "shop.test");
    h.get_by_role_and_label(egui::Role::Button, "blog.test");
    h.get_by_label("Access 512 B");
    h.get_by_label("No web server is serving this site right now, so nothing new is being logged.");
}

#[test]
fn projects_failed_server_offers_to_jump_to_its_service_log() {
    let mut f = base_fixture(logs::Tab::Projects);
    f.projects.sites = Loader::ready(Ok(SitePage { items: vec![site("shop", "shop.test", Some("apache"))], total: 1, offset: 0, limit: 200 }));
    f.projects.sources = Loader::ready(Ok(vec![]));
    f.services = Some(vec![service("apache", "Apache", ComponentId::Apache, Category::WebServer, 80, Health::Failed)]);
    let mut h = harness(f);
    h.get_by_label("Apache stopped unexpectedly, so this site is not being served.");
    h.get_by_label("View service log").click();
    h.run_steps(2);
    assert_eq!(h.state().tab, logs::Tab::Services);
    assert_eq!(h.state().services_state.target, service_logs::Target::Service("apache".into()));
}

// ---------- Services

#[test]
fn services_tab_always_lists_the_zenv_log() {
    let mut f = base_fixture(logs::Tab::Services);
    f.services_state.sources = Loader::ready(Ok(vec![log_source("app", "Zenv log", true, 2048)]));
    let h = harness(f);
    h.get_by_label("Zenv log");
    h.get_by_label("Zenv");
}

#[test]
fn clicking_a_service_switches_the_target_and_opens_its_log_file() {
    let mut f = base_fixture(logs::Tab::Services);
    f.services = Some(vec![service("mysql", "MySQL", ComponentId::Mysql, Category::Database, 3306, Health::Running)]);
    f.services_state.sources = Loader::ready(Ok(vec![log_source("app", "Zenv log", true, 0)]));
    let mut h = harness(f);
    h.get_by_label("MySQL :3306").click();
    h.run_steps(2);
    assert_eq!(h.state().services_state.target, service_logs::Target::Service("mysql".into()));
    h.state_mut().services_state.sources = Loader::ready(Ok(vec![log_source("process", "Process output (stdout/stderr)", true, 10)]));
    h.state_mut().services_state.viewer.tail = Loader::ready(Ok(LogTail { path: PathBuf::from("x"), exists: true, size_bytes: 10, lines: vec!["booted".into()], truncated: false }));
    h.run_steps(2);
    h.get_by_label("Open file").click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.last(), Some(Action::OpenUrl(_))));
}

#[test]
fn service_log_tail_renders_its_lines() {
    let mut f = base_fixture(logs::Tab::Services);
    f.services_state.sources = Loader::ready(Ok(vec![log_source("app", "Zenv log", true, 10)]));
    f.services_state.viewer.tail =
        Loader::ready(Ok(LogTail { path: PathBuf::from("zenv.log"), exists: true, size_bytes: 10, lines: vec!["starting mysql".into(), "stopping apache".into()], truncated: false }));
    let h = harness(f);
    h.get_by_label("starting mysql");
    h.get_by_label("stopping apache");
}

// ---------- Diagnostics

#[test]
fn diagnostics_lists_checks_by_status() {
    let mut f = base_fixture(logs::Tab::Diagnostics);
    f.diagnostics.checks = Loader::ready(Ok(vec![
        Check { id: "a".into(), label: "Data directory".into(), status: CheckStatus::Ok, detail: String::new() },
        Check { id: "b".into(), label: "Hosts file".into(), status: CheckStatus::Warning, detail: "not applied".into() },
    ]));
    let h = harness(f);
    h.get_by_label("Data directory");
    h.get_by_label("Hosts file");
    h.get_by_label("not applied");
}

#[test]
fn diagnostics_error_state() {
    let mut f = base_fixture(logs::Tab::Diagnostics);
    f.diagnostics.checks = Loader::ready(Err("checks failed".into()));
    let h = harness(f);
    h.get_by_label("checks failed");
}

// ---------- Port inspector

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test logs -- --ignored"]
fn render_logs_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();

    let all_services = |mysql_health: Health| {
        vec![
            with_install(service("omniroute", "OmniRoute", ComponentId::Omniroute, Category::Tool, 20128, Health::Running), ComponentId::Omniroute, "1.0.0"),
            with_install(service("ninerouter", "9router", ComponentId::Ninerouter, Category::Tool, 8787, Health::Stopped), ComponentId::Ninerouter, "1.0.0"),
            with_install(service("apache", "Apache", ComponentId::Apache, Category::WebServer, 80, Health::Running), ComponentId::Apache, "1.0.0"),
            with_install(service("mysql", "MySQL", ComponentId::Mysql, Category::Database, 3306, mysql_health), ComponentId::Mysql, "1.0.0"),
        ]
    };
    let php_runtimes = || vec![RuntimeLogGroup { installation_id: "php-8.3.14".into(), label: "PHP 8.3.14".into(), sources: vec![log_source("php-error", "PHP error log", true, 1536)] }];

    let mut projects = base_fixture(logs::Tab::Projects);
    projects.projects.sites = Loader::ready(Ok(SitePage {
        items: vec![site("shop", "shop.test", Some("apache")), site("blog", "blog.test", Some("apache")), site("api", "api.test", Some("apache"))],
        total: 3,
        offset: 0,
        limit: 200,
    }));
    projects.projects.selected = Some("blog".into());
    projects.projects.sources = Loader::ready(Ok(vec![log_source("access", "Access", true, 2048), log_source("error", "Error", false, 0)]));
    projects.projects.viewer.tail = Loader::ready(Ok(tail_at("D:/projects/blog/storage/access.log", log_lines(6, "blog.test"))));
    projects.services = Some(all_services(Health::Running));

    let mut services_f = base_fixture(logs::Tab::Services);
    services_f.services = Some(all_services(Health::Running));
    services_f.services_state.runtimes = Loader::ready(Ok(php_runtimes()));
    services_f.services_state.target = service_logs::Target::Service("mysql".into());
    services_f.services_state.sources = Loader::ready(Ok(vec![log_source("process", "Process output (stdout/stderr)", true, 8192)]));
    services_f.services_state.viewer.tail = Loader::ready(Ok(tail_at("C:/zenv/logs/mysql/process.log", log_lines(8, "MySQL"))));

    let mut diag = base_fixture(logs::Tab::Diagnostics);
    diag.services = Some(all_services(Health::Running));
    diag.diagnostics.checks = Loader::ready(Ok(vec![
        Check { id: "data_dir".into(), label: "Data directory is writable".into(), status: CheckStatus::Ok, detail: "C:/zenv".into() },
        Check { id: "hosts".into(), label: "Hosts file".into(), status: CheckStatus::Warning, detail: "shop.test is missing from the hosts file".into() },
        Check { id: "port80".into(), label: "Port 80 is reachable".into(), status: CheckStatus::Error, detail: "Nothing is listening on 127.0.0.1:80".into() },
    ]));

    for (name, f) in [("Logs-projects", projects), ("Logs-services", services_f), ("Logs-diagnostics", diag)] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 820.0))
            .with_theme(egui::Theme::Dark)
            .with_context_setup(theme::install)
            .with_accessibility_check(false)
            .build_ui_state(shell_and_page, f);
        h.run();
        h.render().unwrap().save(dir.join(format!("native-{name}.png"))).unwrap();
    }
}

#[test]
fn port_inspector_shows_free_in_use_and_unavailable() {
    for (result, text) in [
        (PortInspection { port: 80, address: "127.0.0.1".into(), status: PortStatus::Free }, "127.0.0.1:80 is free."),
        (PortInspection { port: 80, address: "127.0.0.1".into(), status: PortStatus::Unavailable { reason: "access denied".into() } }, "Cannot bind: access denied"),
    ] {
        let mut f = base_fixture(logs::Tab::Ports);
        f.diagnostics.result = Loader::ready(Ok(result));
        let h = harness(f);
        h.get_by_label(text);
    }
}

#[test]
fn project_list_reloads_when_a_task_finished_or_the_page_is_entered() {
    let home = std::env::temp_dir().join(format!("zenv-native-logs-test-{}", std::process::id()));
    let core = std::sync::Arc::new(zenv_core::app_core::Core::initialize(zenv_core::paths::AppPaths::new(home.clone())).unwrap());
    let shared = zenv_native::pages::Shared::default();
    let ctx = egui::Context::default();
    let mut state = project_logs::State::default();
    state.sites = Loader::ready(Ok(SitePage { items: vec![site("shop", "shop.test", None)], total: 1, offset: 0, limit: 200 }));
    for (finished, reloads) in [(false, false), (true, true)] {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let cx = zenv_native::pages::Cx { core: &core, shared: &shared, busy: &[], tasks_finished: finished, actions: Vec::new() };
            project_logs::poll(&mut state, ui, &cx);
        });
        out.textures_delta.clear();
        assert_eq!(state.sites.loading(), reloads);
    }
    drop(core);
    let _ = std::fs::remove_dir_all(home);
}
