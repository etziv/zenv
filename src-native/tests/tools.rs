use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::ai_router::{RouterModel, RouterPage, RouterStatus};
use zenv_core::codex::CodexStatus;
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::service_manager::{Health, Service, ServiceStatus};
use zenv_native::loader::Loader;
use zenv_native::pages::tools::Routers;
use zenv_native::pages::{Action, Route};
use zenv_native::{shell, theme};

fn service(id: &str, name: &str, component: ComponentId, port: u16, health: Health) -> ServiceStatus {
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
        kind: Category::Tool,
        data_dir_info: None,
        connection: None,
        behind: None,
        needs_initialization: false,
    }
}

fn router_status(installed: Option<&str>) -> RouterStatus {
    RouterStatus {
        component: ComponentId::Ninerouter,
        installed: installed.map(str::to_owned),
        latest: None,
        auto_from: None,
        auto_update: true,
        last_checked: None,
        updating: false,
        last_error: None,
        pages: vec![RouterPage { id: "home", label: "Dashboard" }],
    }
}

fn codex_status(configured: bool, is_default: bool, model: Option<&str>) -> CodexStatus {
    CodexStatus {
        config_path: PathBuf::from(r"C:\Users\x\.codex\config.toml"),
        exists: configured,
        configured,
        base_url: configured.then(|| "http://127.0.0.1:20128/v1".to_owned()),
        is_default,
        top_level_provider: None,
        default_profile: None,
        profile_model: model.map(str::to_owned),
        model: model.map(str::to_owned),
        context_window: None,
        max_output: None,
    }
}

struct Fixture {
    services: Loader<Vec<ServiceStatus>>,
    routers: Routers,
    busy: Vec<String>,
    actions: Vec<Action>,
}

fn fixture(services: Result<Vec<ServiceStatus>, String>) -> Fixture {
    Fixture { services: Loader::ready(services), routers: Routers::default(), busy: vec![], actions: vec![] }
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let mut out = Vec::new();
    for status in f.services.ok().into_iter().flatten() {
        f.routers.panel(ui, status, &f.busy, &mut out);
    }
    f.actions.extend(out);
}

fn full_router_status(component: ComponentId, installed: &str, latest: Option<&str>, auto_from: Option<&str>) -> RouterStatus {
    let pages = match component {
        ComponentId::Omniroute => vec![
            RouterPage { id: "providers", label: "Providers" },
            RouterPage { id: "combos", label: "Combos" },
            RouterPage { id: "api-manager", label: "API manager" },
            RouterPage { id: "api-endpoints", label: "API endpoints" },
            RouterPage { id: "usage", label: "Usage" },
            RouterPage { id: "quota", label: "Quota" },
            RouterPage { id: "costs", label: "Costs" },
            RouterPage { id: "logs", label: "Logs" },
            RouterPage { id: "health", label: "Health" },
        ],
        _ => vec![
            RouterPage { id: "providers", label: "Providers" },
            RouterPage { id: "combos", label: "Combos" },
            RouterPage { id: "endpoint", label: "Endpoint" },
            RouterPage { id: "cli-tools", label: "CLI tools" },
            RouterPage { id: "usage", label: "Usage" },
            RouterPage { id: "quota", label: "Quota" },
            RouterPage { id: "settings", label: "Settings" },
        ],
    };
    RouterStatus {
        component,
        installed: Some(installed.to_owned()),
        latest: latest.map(str::to_owned),
        auto_from: auto_from.map(str::to_owned),
        auto_update: true,
        last_checked: Some("2026-11-18T09:00:00Z".into()),
        updating: false,
        last_error: None,
        pages,
    }
}

fn full_codex_status(configured: bool, is_default: bool, model: Option<&str>, top_level_provider: Option<&str>) -> CodexStatus {
    CodexStatus {
        config_path: PathBuf::from(r"C:\Users\dev\.codex\config.toml"),
        exists: configured,
        configured,
        base_url: configured.then(|| "http://127.0.0.1:20128/v1".to_owned()),
        is_default,
        top_level_provider: top_level_provider.map(str::to_owned),
        default_profile: None,
        profile_model: model.map(str::to_owned),
        model: model.map(str::to_owned),
        context_window: configured.then_some(400_000),
        max_output: configured.then_some(128_000),
    }
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 1000.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

#[test]
fn stopped_router_ignores_open_dashboard_click() {
    let mut f = fixture(Ok(vec![service("ninerouter", "9router", ComponentId::Ninerouter, 20128, Health::Stopped)]));
    f.routers.0[1].status = Loader::ready(Ok(router_status(Some("0.5.104"))));
    f.routers.0[1].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let mut h = harness(f);
    h.get_by_label("Not set up");
    h.get_by_label("Open dashboard").click();
    h.run_steps(2);
    assert!(h.state().actions.is_empty(), "a stopped router's dashboard button must not open anything");
}

#[test]
fn open_dashboard_emits_open_router_when_running() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Ok(router_status(Some("1.0.0"))));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let mut h = harness(f);
    h.get_by_label("Open dashboard").click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.as_slice(), [Action::OpenRouter(id, None)] if id == "omniroute"));
}

#[test]
fn page_shortcut_opens_that_router_page() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Ok(router_status(Some("1.0.0"))));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let mut h = harness(f);
    h.get_by_label("usage").click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.as_slice(), [Action::OpenRouter(id, Some(p))] if id == "omniroute" && p == "usage"));
}

#[test]
fn codex_add_button_emits_setup_task() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Ok(router_status(Some("1.0.0"))));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let mut h = harness(f);
    h.get_by_label("Add to Codex").click();
    h.run_steps(2);
    let keys: Vec<String> = h
        .state()
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Run(t) => Some(t.key.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["codex:omniroute:setup"]);
}

#[test]
fn model_picker_opens_and_save_emits_model_task() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Ok(router_status(Some("1.0.0"))));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(true, true, Some("auto"))));
    f.routers.0[0].picker.models = Loader::ready(Ok(vec![RouterModel { id: "gpx".into(), owned_by: Some("combo".into()), combo: true, context_window: Some(1_000_000), max_output: Some(384_000) }]));
    let mut h = harness(f);
    h.get_by_label("Choose model").click();
    h.run_steps(2);
    h.get_by_label("Save to config.toml");
    h.get_by_label("gpx · 1M context").click();
    h.run_steps(2);
    assert_eq!(h.state().routers.0[0].picker.model, "gpx");
    h.get_by_label("Save to config.toml").click();
    h.run_steps(2);
    let keys: Vec<String> = h
        .state()
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Run(t) => Some(t.key.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["codex:omniroute:model"]);
    assert!(!h.state().routers.0[0].picker.open, "the picker closes after saving");
}

#[test]
fn copy_button_marks_the_url_copied() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Ok(router_status(Some("1.0.0"))));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let mut h = harness(f);
    h.get_by_label("http://127.0.0.1:20128/v1").click();
    h.run_steps(2);
    assert!(h.state().routers.0[0].copied_url.is_some());
}

#[test]
fn router_status_error_is_shown() {
    let mut f = fixture(Ok(vec![service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running)]));
    f.routers.0[0].status = Loader::ready(Err("npm registry unreachable".into()));
    f.routers.0[0].codex = Loader::ready(Ok(codex_status(false, false, None)));
    let h = harness(f);
    h.get_by_label("npm registry unreachable");
}

fn router_install(component: ComponentId, version: &str) -> zenv_core::runtime_manager::Installation {
    zenv_core::runtime_manager::Installation {
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
    }
}

struct PageFixture {
    installations: Vec<zenv_core::runtime_manager::Installation>,
    services: Vec<ServiceStatus>,
    routers: std::cell::RefCell<Routers>,
    picked: ComponentId,
}

fn tools_page(ui: &mut egui::Ui, f: &mut PageFixture) {
    use zenv_native::pages::components::{self, Data, Dialog};
    let catalog: Vec<_> = zenv_core::runtime_catalog::catalog().iter().filter(|c| c.category == Category::Tool).cloned().collect();
    let (icons, path_status, errors) = (Default::default(), Default::default(), Default::default());
    let services = f.services.clone();
    shell::frame(ui, Route::Tools, "1.0.0", Some(&services), |ui, out| {
        let data = Data {
            category: Category::Tool,
            title: "Tools",
            components: &catalog,
            installations: &f.installations,
            runtimes: &[],
            services: &f.services,
            icons: &icons,
            path_status: &path_status,
            jobs: &[],
            discover: None,
            available: None,
            errors: &errors,
            routers: &f.routers,
            rows: &Default::default(),
        };
        let (mut dialog, mut scan) = (Dialog::None, false);
        components::view(ui, &data, &mut f.picked, &mut dialog, &[], &mut scan, out);
    });
}

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test tools -- --ignored"]
fn render_tools_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();
    let mut omni = service("omniroute", "OmniRoute", ComponentId::Omniroute, 20128, Health::Running);
    omni.service.installation_id = "omniroute-1.4.2".into();
    let routers = Routers::default();
    let routers = std::cell::RefCell::new(routers);
    {
        let mut r = routers.borrow_mut();
        r.0[0].status = Loader::ready(Ok(full_router_status(ComponentId::Omniroute, "1.4.2", Some("1.5.0"), Some("2026-11-20T00:00:00Z"))));
        r.0[0].codex = Loader::ready(Ok(full_codex_status(true, true, Some("gpt-5-codex"), None)));
    }
    let f = PageFixture {
        installations: vec![router_install(ComponentId::Omniroute, "1.4.2")],
        services: vec![omni],
        routers,
        picked: ComponentId::Omniroute,
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 820.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .with_accessibility_check(false)
        .build_ui_state(tools_page, f);
    h.run();
    h.render().unwrap().save(dir.join("native-Tools-omniroute.png")).unwrap();
}

#[test]
fn router_update_note_follows_react() {
    use zenv_native::loader::Tone;
    use zenv_native::pages::tools::update_note;
    let newest = full_router_status(ComponentId::Omniroute, "3.2.0", Some("3.2.0"), None);
    assert_eq!((update_note("OmniRoute", &newest).tone, update_note("OmniRoute", &newest).title), (Tone::Success, "OmniRoute 3.2.0 is the newest".to_owned()));

    let waiting = full_router_status(ComponentId::Omniroute, "3.1.0", Some("3.2.0"), Some("2026-11-25T00:00:00Z"));
    let note = update_note("OmniRoute", &waiting);
    assert_eq!(note.title, "OmniRoute 3.2.0 is available");
    assert!(note.description.contains("(from 11/25/2026)"), "{}", note.description);

    let mut failed = full_router_status(ComponentId::Omniroute, "3.1.0", None, None);
    failed.last_error = Some("npm exited with code 1".into());
    let note = update_note("OmniRoute", &failed);
    assert_eq!((note.tone, note.title, note.description), (Tone::Error, "OmniRoute was not updated".to_owned(), "npm exited with code 1".to_owned()));

    let unknown = full_router_status(ComponentId::Omniroute, "3.1.0", None, None);
    assert_eq!(update_note("OmniRoute", &unknown).title, "OmniRoute checked");
}
