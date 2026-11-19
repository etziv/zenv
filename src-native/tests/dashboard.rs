use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_services::NeededService;
use zenv_core::diagnostics::{Check, CheckStatus};
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::runtime_manager::{InstallSource, Installation};
use zenv_core::service_manager::{Health, Service, ServiceStatus};
use zenv_native::loader::Loader;
use zenv_native::pages::dashboard::{self, Data};
use zenv_native::pages::{Action, Route};
use zenv_native::{shell, theme};

fn install(component: ComponentId, version: &str, default: bool) -> Installation {
    Installation {
        id: format!("{component}-{version}"),
        component,
        version: version.into(),
        version_detail: String::new(),
        root_path: PathBuf::from(format!("C:/zenv/bin/{component}/{version}")),
        executable_path: PathBuf::new(),
        source: InstallSource::Managed,
        is_default: default,
        enabled: true,
        executable_exists: true,
        detected_at: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn status(id: &str, name: &str, component: ComponentId, kind: Category, version: Option<&str>, port: u16, health: Health) -> ServiceStatus {
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
            https_port: (kind == Category::WebServer).then_some(443),
        },
        installation: version.map(|v| install(component, v, false)),
        health,
        process: ProcessState::NotStarted,
        url: format!("http://localhost:{port}"),
        config_path: PathBuf::new(),
        document_root: PathBuf::new(),
        kind,
        data_dir_info: None,
        connection: None,
        behind: None,
        needs_initialization: false,
    }
}

fn services() -> Vec<ServiceStatus> {
    let mut nginx = status("nginx", "Nginx", ComponentId::Nginx, Category::WebServer, Some("1.27.3"), 80, Health::Stopped);
    nginx.behind = Some("Apache".into());
    vec![
        status("apache", "Apache", ComponentId::Apache, Category::WebServer, Some("2.4.62"), 80, Health::Running),
        nginx,
        status("mysql", "MySQL", ComponentId::Mysql, Category::Database, Some("9.7.0"), 3306, Health::Failed),
        status("mysql8", "MySQL 8", ComponentId::Mysql, Category::Database, Some("8.0.30"), 3307, Health::Stopped),
        status("pg", "PostgreSQL", ComponentId::Postgresql, Category::Database, None, 5432, Health::Running),
    ]
}

fn needed(running: bool) -> NeededService {
    NeededService {
        service_id: "mysql".into(),
        name: "MySQL".into(),
        kind: Category::Database,
        running,
        used_by: vec!["shop.test (DB_PORT=3306)".into(), "shop.test (REDIS_PORT=3306)".into(), "blog.test".into()],
        required: false,
    }
}

struct Fixture {
    services: Loader<Vec<ServiceStatus>>,
    installations: Loader<Vec<Installation>>,
    needed: Loader<Vec<NeededService>>,
    diagnostics: Loader<Vec<Check>>,
    errors: std::collections::HashMap<String, String>,
    busy: Vec<String>,
    actions: Vec<Action>,
}

fn fixture(services: Result<Vec<ServiceStatus>, String>) -> Fixture {
    Fixture {
        services: Loader::ready(services),
        installations: Loader::ready(Ok(vec![
            install(ComponentId::Php, "8.3.14", true),
            install(ComponentId::Php, "8.4.1", false),
            install(ComponentId::Node, "22.11.0", true),
        ])),
        needed: Loader::ready(Ok(vec![needed(false)])),
        diagnostics: Loader::ready(Ok(vec![Check {
            id: "data_dir".into(),
            label: "Data directory".into(),
            status: CheckStatus::Warning,
            detail: String::new(),
        }])),
        errors: Default::default(),
        busy: vec![],
        actions: vec![],
    }
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let mut out = Vec::new();
    let data = Data {
        services: &f.services,
        installations: &f.installations,
        needed: &f.needed,
        diagnostics: &f.diagnostics,
        errors: &f.errors,
    };
    dashboard::view(ui, &data, &f.busy, &mut out);
    f.actions.extend(out);
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 900.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

#[test]
fn renders_groups_tiles_and_meta() {
    let h = harness(fixture(Ok(services())));
    h.get_by_label("2 of 5 services running");
    for heading in ["WEB SERVERS", "DATABASES", "LANGUAGES", "HEALTH"] {
        h.get_by_label(heading);
    }
    h.get_by_label("Stop Apache");
    h.get_by_label("Start MySQL");
    h.get_by_label(":80 via Apache");
    h.get_by_label("1 of 1 checks need attention.");
    h.get_by_label("Start apps (1)");
    h.get_by_label("Stop all (2)");
}

#[test]
fn switch_emits_start_and_stop_for_that_service() {
    let mut h = harness(fixture(Ok(services())));
    h.get_by_label("Start MySQL").click();
    h.run_steps(2);
    h.get_by_label("Stop Apache").click();
    h.run_steps(2);
    let keys: Vec<String> = h
        .state()
        .actions
        .iter()
        .map(|a| match a {
            Action::Run(t) => t.key.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(keys, ["service:mysql", "service:apache"]);
}

#[test]
fn failed_start_shows_inline_under_its_tile() {
    let mut f = fixture(Ok(services()));
    f.errors.insert(dashboard::service_key("mysql"), "port 3306 is in use".into());
    let h = harness(f);
    h.get_by_label("port 3306 is in use");
}

#[test]
fn switch_tasks_report_errors_inline() {
    let mut h = harness(fixture(Ok(services())));
    h.get_by_label("Start MySQL").click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.as_slice(), [Action::Run(t)] if t.inline));
}

#[test]
fn busy_switch_ignores_clicks() {
    let mut f = fixture(Ok(services()));
    f.busy = vec![dashboard::service_key("mysql")];
    let mut h = harness(f);
    h.get_by_label("Start MySQL").click();
    h.run_steps(2);
    assert!(h.state().actions.is_empty());
}

#[test]
fn empty_group_offers_to_add_and_navigates() {
    let mut h = harness(fixture(Ok(vec![])));
    h.get_by_label("No services yet");
    h.get_by_label("Add Redis").click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.as_slice(), [Action::Navigate(Route::Cache)]));
    assert!(h.query_by_label("Stop all (0)").is_none());
}

#[test]
fn error_and_loading_states() {
    let h = harness(fixture(Err("database is locked".into())));
    h.get_by_label("database is locked");
    let mut f = fixture(Ok(vec![]));
    f.installations = Loader::default();
    let h = harness(f);
    h.get_by_label("Loading…");
}

#[test]
fn start_apps_label_follows_state() {
    assert_eq!(dashboard::start_apps_label(&[needed(false)], false), "Start apps (1)");
    assert_eq!(dashboard::start_apps_label(&[needed(true)], false), "Apps running");
    assert_eq!(dashboard::start_apps_label(&[needed(false)], true), "Starting…");
}

#[test]
fn versions_sort_newest_first_numerically() {
    let mut v = vec!["8.0.30", "9.3.0", "10.0", "9.7.0"];
    v.sort_by(|a, b| dashboard::compare_versions_desc(a, b));
    assert_eq!(v, ["10.0", "9.7.0", "9.3.0", "8.0.30"]);
}

#[test]
fn services_sort_by_component_then_version_then_name() {
    let all = services();
    let names: Vec<&str> = dashboard::sorted(all.iter().collect()).iter().map(|s| s.service.name.as_str()).collect();
    assert_eq!(names, ["Apache", "MySQL", "MySQL 8", "Nginx", "PostgreSQL"]);
}

#[test]
fn rail_shows_only_live_services() {
    let all = services();
    let live: Vec<&str> = shell::rail_entries(&all).iter().map(|s| s.service.id.as_str()).collect();
    assert_eq!(live, ["apache", "mysql", "pg"]);
    assert_eq!(shell::rail_name(ComponentId::Postgresql), "postgres");
    assert_eq!(shell::page_for(Category::Database), Route::Databases);
}

#[test]
fn port_text_and_shared_port() {
    let all = services();
    assert_eq!(dashboard::port_text(&all[0]), ":80 · :443");
    assert_eq!(dashboard::port_text(&all[1]), ":80 via Apache");
    assert_eq!(dashboard::port_text(&all[2]), ":3306");
    assert_eq!(dashboard::shares_port_with(&all[0], &all), ["Nginx"]);
    assert!(dashboard::shares_port_with(&all[2], &all).is_empty());
}

fn shell_and_page(ui: &mut egui::Ui, f: &mut Fixture) {
    let services = f.services.ok().cloned();
    let actions = shell::frame(ui, Route::Dashboard, "1.0.0", services.as_deref(), |ui, _| page(ui, f));
    drop(actions);
}

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test dashboard -- --ignored"]
fn render_dashboard_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, mode) in [("dark", egui::Theme::Dark), ("light", egui::Theme::Light)] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 820.0))
            .with_theme(mode)
            .with_context_setup(theme::install)
            .build_ui_state(shell_and_page, fixture(Ok(services())));
        h.run();
        h.render().unwrap().save(dir.join(format!("dashboard-{name}.png"))).unwrap();
    }
}

#[test]
fn crash_notice_only_on_running_to_failed() {
    let mut last = std::collections::HashMap::new();
    let mut all = services();
    assert!(zenv_native::app::crash_toasts(&mut last, &all).is_empty());
    all[0].health = Health::Failed;
    all[2].health = Health::Stopped;
    let toasts = zenv_native::app::crash_toasts(&mut last, &all);
    let titles: Vec<&str> = toasts.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["Apache stopped unexpectedly"]);
    assert!(matches!(toasts[0].action, Some(("View logs", Route::Logs))));
    assert!(zenv_native::app::crash_toasts(&mut last, &all).is_empty());
}

fn outcome(name: &str, result: &'static str, error: Option<&str>) -> zenv_core::app_services::StartOutcome {
    zenv_core::app_services::StartOutcome { service_id: name.to_lowercase(), name: name.into(), result, error: error.map(Into::into) }
}

fn titles(notes: &[zenv_native::loader::Note]) -> Vec<(zenv_native::loader::Tone, String)> {
    notes.iter().map(|n| (n.tone, n.title.clone())).collect()
}

#[test]
fn start_apps_reports_each_failure_and_the_started_services() {
    use zenv_native::loader::Tone::*;
    let notes = dashboard::start_notes(&[
        outcome("MySQL", "failed", Some("port 3306 is in use")),
        outcome("Apache", "started", None),
        outcome("Redis", "started", None),
        outcome("PHP", "already_running", None),
    ]);
    assert_eq!(titles(&notes), vec![(Error, "Couldn't start MySQL".into()), (Success, "Started Apache, Redis".into())]);
    assert_eq!(notes[0].description, "port 3306 is in use");
}

#[test]
fn start_apps_when_everything_fails_shows_no_success() {
    use zenv_native::loader::Tone::*;
    let notes = dashboard::start_notes(&[outcome("MySQL", "failed", Some("x")), outcome("Apache", "failed", Some("y"))]);
    assert_eq!(titles(&notes), vec![(Error, "Couldn't start MySQL".into()), (Error, "Couldn't start Apache".into())]);
}

#[test]
fn start_apps_with_nothing_to_do_says_so() {
    use zenv_native::loader::Tone::*;
    assert_eq!(titles(&dashboard::start_notes(&[])), vec![(Info, "No app needs a service yet".into())]);
    assert_eq!(titles(&dashboard::start_notes(&[outcome("Apache", "already_running", None)])), vec![(Info, "Everything your apps need is already running".into())]);
}

#[test]
fn stop_all_counts_stopped_and_reports_failures() {
    use zenv_native::loader::Tone::*;
    let notes = dashboard::stop_notes(&[outcome("Apache", "stopped", None), outcome("MySQL", "failed", Some("access denied"))]);
    assert_eq!(titles(&notes), vec![(Error, "Couldn't stop MySQL".into()), (Success, "Stopped 1 service".into())]);
    assert!(dashboard::stop_notes(&[]).is_empty());
}

fn job(id: &str, phase: zenv_core::runtime_installer::Phase, error: Option<&str>) -> zenv_core::runtime_installer::InstallJob {
    zenv_core::runtime_installer::InstallJob {
        id: id.into(),
        component: ComponentId::Php,
        version: "8.4.1".into(),
        phase,
        downloaded: 0,
        total: 0,
        error: error.map(Into::into),
        installation_id: None,
        started_at: zenv_core::util::now_rfc3339(),
    }
}

#[test]
fn install_end_is_announced_once_and_old_jobs_are_not_news() {
    use zenv_core::runtime_installer::Phase::*;
    use zenv_native::loader::Tone;
    let mut seen = None;
    assert!(zenv_native::app::install_notes(&mut seen, &[job("old", Failed, Some("x")), job("a", Downloading, None), job("b", Downloading, None)]).is_empty());

    let notes = zenv_native::app::install_notes(&mut seen, &[job("old", Failed, Some("x")), job("a", Done, None), job("b", Failed, Some("checksum mismatch"))]);
    assert_eq!(titles(&notes), vec![(Tone::Success, "PHP 8.4.1 is installed".into()), (Tone::Error, "PHP 8.4.1 was not installed".into())]);
    assert_eq!(notes[1].description, "checksum mismatch");

    assert!(zenv_native::app::install_notes(&mut seen, &[job("a", Done, None), job("b", Failed, Some("checksum mismatch"))]).is_empty());
}
