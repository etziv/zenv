use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::path_env::{PathConflict, PathStatus};
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::{self, Category, ComponentId, ComponentInfo};
use zenv_core::runtime_installer::{AvailableVersion, InstallJob, Phase};
use zenv_core::runtime_manager::{InstallSource, Installation};
use zenv_core::service_manager::{Health, Service, ServiceStatus};
use zenv_native::loader::Loader;
use zenv_native::pages::components::{self, Data, Dialog};
use zenv_native::pages::{Action, Route};
use zenv_native::shell;
use zenv_native::theme;

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

fn status(id: &str, name: &str, component: ComponentId, kind: Category, installation_id: &str, port: u16, health: Health) -> ServiceStatus {
    ServiceStatus {
        service: Service {
            id: id.into(),
            component,
            name: name.into(),
            installation_id: installation_id.into(),
            listen_address: "127.0.0.1".into(),
            http_port: port,
            autostart: false,
            created_at: String::new(),
            updated_at: String::new(),
            data_dir: None,
            https_port: (kind == Category::WebServer).then_some(443),
        },
        installation: None,
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

fn components_of(category: Category) -> Vec<ComponentInfo> {
    runtime_catalog::catalog().iter().filter(|c| c.category == category).cloned().collect()
}

struct Fixture {
    category: Category,
    title: &'static str,
    installations: Vec<Installation>,
    services: Vec<ServiceStatus>,
    jobs: Vec<zenv_core::runtime_installer::InstallJob>,
    busy: Vec<String>,
    picked: ComponentId,
    dialog: Dialog,
    scan_clicked: bool,
    actions: Vec<Action>,
    discover: Option<Loader<Vec<zenv_core::runtime_manager::DiscoveredInstallation>>>,
    available: Option<Loader<Vec<AvailableVersion>>>,
    path_status: HashMap<ComponentId, Loader<PathStatus>>,
    errors: HashMap<String, String>,
    routers: std::cell::RefCell<zenv_native::pages::tools::Routers>,
    rows: std::cell::RefCell<components::Rows>,
}

fn fixture(category: Category, title: &'static str, picked: ComponentId) -> Fixture {
    Fixture {
        category,
        title,
        installations: vec![],
        services: vec![],
        jobs: vec![],
        busy: vec![],
        picked,
        dialog: Dialog::None,
        scan_clicked: false,
        actions: vec![],
        discover: None,
        available: None,
        path_status: HashMap::new(),
        errors: HashMap::new(),
        routers: Default::default(),
        rows: Default::default(),
    }
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let components = components_of(f.category);
    let icons = HashMap::new();
    let data = Data {
        category: f.category,
        title: f.title,
        components: &components,
        installations: &f.installations,
        runtimes: &[],
        services: &f.services,
        icons: &icons,
        path_status: &f.path_status,
        jobs: &f.jobs,
        discover: f.discover.as_ref(),
        available: f.available.as_ref(),
        errors: &f.errors,
        routers: &f.routers,
        rows: &f.rows,
    };
    let mut out = Vec::new();
    components::view(ui, &data, &mut f.picked, &mut f.dialog, &f.busy, &mut f.scan_clicked, &mut out);
    f.actions.extend(out);
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 900.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

#[test]
fn renders_component_with_servers_and_versions() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    f.services = vec![status("apache", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 8080, Health::Running)];
    let h = harness(f);
    h.get_by_role_and_label(egui::Role::Button, "Apache HTTP Server");
    assert_eq!(h.get_all_by_role_and_label(egui::Role::Label, "Apache HTTP Server").count(), 2);
    h.get_by_label("VERSIONS & SERVERS · 1");
    h.get_by_label("Stop Apache");
    h.get_by_label("127.0.0.1:8080 · :443");
}

#[test]
fn switch_click_emits_the_service_task_key() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    f.services = vec![status("apache", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 8080, Health::Running)];
    let mut h = harness(f);
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
    assert_eq!(keys, ["service:apache"]);
}

#[test]
fn install_button_opens_dialog_then_version_row_emits_install_task() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    let mut h = harness(f);
    h.get_by_label("Install version").click();
    h.run_steps(2);
    h.state_mut().available = Some(Loader::ready(Ok(vec![AvailableVersion {
        component: ComponentId::Php,
        version: "8.4.1".into(),
        stable: true,
        filename: "php.zip".into(),
        url: "https://example.test/php.zip".into(),
        sha256: String::new(),
        size: 0,
        source: "windows.php.net",
        installed_id: None,
        verified_by: "SHA-256",
    }])));
    h.run_steps(2);
    h.get_by_label("8.4.1");
    h.get_by_label("Install").click();
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
    assert_eq!(keys, ["installer:install:php:8.4.1"]);
}

#[test]
fn empty_component_offers_install_hint() {
    let f = fixture(Category::Language, "Languages", ComponentId::Php);
    let h = harness(f);
    h.get_by_label("INSTALLED VERSIONS · 0");
    h.get_by_label("PHP is not installed yet");
}

#[test]
fn discovery_error_state_shows_the_message() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.discover = Some(Loader::ready(Err("scan failed: access denied".into())));
    let h = harness(f);
    h.get_by_label("scan failed: access denied");
}

#[test]
fn discovery_empty_state_offers_add_from_folder() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.discover = Some(Loader::ready(Ok(vec![])));
    let h = harness(f);
    h.get_by_label("Found 0 installations on this computer");
    h.get_by_label("Nothing was found on PATH or in common install locations. Use \"Add from folder\" to point Zenv at a folder.");
}

#[test]
fn open_terminal_menu_item_emits_the_terminal_task_key() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    let mut h = harness(f);
    h.get_by_label("More actions").click();
    h.run_steps(2);
    h.get_by_label("Open terminal with this version").click();
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
    assert_eq!(keys, ["installation:terminal:php-8.3.14"]);
}

#[test]
fn path_button_emits_add_to_path_task() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    f.path_status.insert(
        ComponentId::Php,
        Loader::ready(Ok(PathStatus { component: ComponentId::Php, eligible: true, installation_id: None, conflicts: vec![] })),
    );
    let mut h = harness(f);
    h.get_by_label("Add to PATH").click();
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
    assert_eq!(keys, ["path:add:php"]);
}

#[test]
fn inline_service_error_shows_under_the_switch_like_react_servicecard() {
    let mut f = fixture(Category::Database, "Databases", ComponentId::Mysql);
    f.installations = vec![install(ComponentId::Mysql, "9.7.0", true)];
    f.services = vec![status("mysql", "MySQL", ComponentId::Mysql, Category::Database, "mysql-9.7.0", 3306, Health::Failed)];
    f.errors.insert("service:mysql".into(), "the server exited unexpectedly".into());
    let h = harness(f);
    h.get_by_label("the server exited unexpectedly");
}

#[test]
fn inline_installation_error_shows_under_the_row_like_react_installationrow() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    f.errors.insert("installation:refresh:php-8.3.14".into(), "could not run php --version".into());
    let h = harness(f);
    h.get_by_label("could not run php --version");
}

fn job(component: ComponentId, version: &str, phase: Phase, downloaded: u64, total: u64) -> InstallJob {
    InstallJob {
        id: format!("{component}-{version}"),
        component,
        version: version.into(),
        phase,
        downloaded,
        total,
        error: None,
        installation_id: None,
        started_at: String::new(),
    }
}

#[test]
fn install_indicator_is_absent_without_active_jobs() {
    let mut h = Harness::builder().with_context_setup(theme::install).build_ui(|ui| {
        let clicked = components::install_indicator(ui, &[job(ComponentId::Php, "8.4.1", Phase::Done, 0, 0)]);
        assert_eq!(clicked, None);
    });
    h.run();
}

#[test]
fn install_indicator_shows_the_component_and_percent_for_one_job() {
    let mut h = Harness::builder().with_context_setup(theme::install).build_ui(|ui| {
        components::install_indicator(ui, &[job(ComponentId::Php, "8.4.1", Phase::Downloading, 50, 100)]);
    });
    h.run_steps(1);
    h.get_by_role_and_label(egui::Role::Button, "Installing PHP 8.4.1 · 50%");
}

#[test]
fn install_indicator_counts_versions_when_several_are_active() {
    let mut h = Harness::builder().with_context_setup(theme::install).build_ui(|ui| {
        components::install_indicator(
            ui,
            &[
                job(ComponentId::Php, "8.4.1", Phase::Downloading, 10, 100),
                job(ComponentId::Mysql, "9.7.0", Phase::Extracting, 0, 0),
            ],
        );
    });
    h.run_steps(1);
    h.get_by_role_and_label(egui::Role::Button, "Installing 2 versions");
}

#[test]
fn install_indicator_click_returns_the_first_active_component() {
    let mut h = Harness::builder().with_context_setup(theme::install).build_ui_state(
        |ui, clicked: &mut Option<ComponentId>| {
            *clicked = components::install_indicator(ui, &[job(ComponentId::Php, "8.4.1", Phase::Extracting, 0, 0)]).or(*clicked);
        },
        None,
    );
    h.run_steps(1);
    h.get_by_role_and_label(egui::Role::Button, "Installing PHP 8.4.1").click();
    h.run_steps(1);
    assert_eq!(*h.state(), Some(ComponentId::Php));
}

#[test]
fn path_conflicts_panel_renders_the_blocking_entries() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    f.path_status.insert(
        ComponentId::Php,
        Loader::ready(Ok(PathStatus {
            component: ComponentId::Php,
            eligible: true,
            installation_id: None,
            conflicts: vec![PathConflict { entry: r"C:\xampp\php".into(), scope: "system", program: "php.exe".into() }],
        })),
    );
    let h = harness(f);
    h.get_by_label(r"C:\xampp\php (system PATH, php.exe)");
}

fn shell_and_page(ui: &mut egui::Ui, route: Route, f: &mut Fixture) {
    let services = f.services.clone();
    let actions = shell::frame(ui, route, "1.0.0", Some(services.as_slice()), |ui, _| page(ui, f));
    drop(actions);
}

fn available(component: ComponentId, versions: &[(&str, bool)]) -> Loader<Vec<AvailableVersion>> {
    Loader::ready(Ok(versions
        .iter()
        .map(|(v, installed)| AvailableVersion {
            component,
            version: (*v).into(),
            stable: true,
            filename: format!("{component}-{v}.zip"),
            url: format!("https://example.test/{component}-{v}.zip"),
            sha256: String::new(),
            size: 24_000_000,
            source: "windows.php.net",
            installed_id: installed.then(|| format!("{component}-{v}")),
            verified_by: "SHA-256",
        })
        .collect()))
}

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test components -- --ignored"]
fn render_components_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();

    let mut languages = fixture(Category::Language, "Languages", ComponentId::Php);
    languages.installations = vec![
        install(ComponentId::Php, "8.3.14", true),
        install(ComponentId::Php, "8.4.1", false),
        install(ComponentId::Node, "22.11.0", true),
    ];

    let mut web_servers = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    web_servers.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    web_servers.services = vec![status("apache", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 80, Health::Running)];

    let mut databases = fixture(Category::Database, "Databases", ComponentId::Mysql);
    databases.installations = vec![install(ComponentId::Mysql, "9.7.0", true), install(ComponentId::Mysql, "8.0.30", false)];
    databases.services = vec![
        status("mysql", "MySQL", ComponentId::Mysql, Category::Database, "mysql-9.7.0", 3306, Health::Failed),
        status("mysql8", "MySQL 8", ComponentId::Mysql, Category::Database, "mysql-8.0.30", 3307, Health::Stopped),
    ];

    let cache = fixture(Category::Cache, "Cache & Queue", ComponentId::Redis);

    let mut install_dialog = fixture(Category::Language, "Languages", ComponentId::Php);
    install_dialog.installations = languages.installations.clone();
    install_dialog.dialog = Dialog::install(ComponentId::Php);
    install_dialog.available = Some(available(ComponentId::Php, &[("8.4.1", true), ("8.3.14", true), ("8.2.25", false)]));

    let mut register_dialog = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    register_dialog.dialog = Dialog::register(ComponentId::Apache);

    for (name, route, f) in [
        ("Languages-php", Route::Languages, languages),
        ("WebServers-apache", Route::WebServers, web_servers),
        ("Databases-mysql", Route::Databases, databases),
        ("Cache-empty", Route::Cache, cache),
        ("install-dialog", Route::Languages, install_dialog),
        ("register-dialog", Route::WebServers, register_dialog),
    ] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 820.0))
            .with_theme(egui::Theme::Dark)
            .with_context_setup(theme::install)
            .with_accessibility_check(false)
            .build_ui_state(move |ui, f| shell_and_page(ui, route, f), f);
        h.run();
        h.render().unwrap().save(dir.join(format!("native-{name}.png"))).unwrap();
    }
}

fn task_keys(h: &Harness<'static, Fixture>) -> Vec<String> {
    h.state().actions.iter().filter_map(|a| if let Action::Run(t) = a { Some(t.key.clone()) } else { None }).collect()
}

fn open_menu(h: &mut Harness<'static, Fixture>, trigger: &str, item: &str) {
    h.get_by_label(trigger).click();
    h.run_steps(2);
    h.get_by_label(item).click();
    h.run_steps(2);
}

#[test]
fn language_versions_keep_the_core_order_like_react() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true), install(ComponentId::Php, "8.4.1", false)];
    let h = harness(f);
    let older = h.get_by_label("8.3.14").rect().top();
    let newer = h.get_by_label("8.4.1").rect().top();
    assert!(older < newer, "8.3.14 comes first, as the core lists it");
}

#[test]
fn uninstall_asks_first_then_confirm_emits_the_task_and_cancel_does_not() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    f.installations = vec![install(ComponentId::Php, "8.3.14", true)];
    let mut h = harness(f);
    open_menu(&mut h, "More actions", "Uninstall 8.3.14");
    h.get_by_label("Uninstall php 8.3.14?");
    assert!(task_keys(&h).is_empty(), "nothing runs before the confirm");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.query_by_label("Uninstall php 8.3.14?").is_none());
    assert!(task_keys(&h).is_empty());

    open_menu(&mut h, "More actions", "Uninstall 8.3.14");
    h.get_by_label("Uninstall").click();
    h.run_steps(3);
    assert_eq!(task_keys(&h), ["installation:uninstall:php-8.3.14"]);
}

#[test]
fn registered_install_offers_remove_instead_of_uninstall() {
    let mut f = fixture(Category::Language, "Languages", ComponentId::Php);
    let mut inst = install(ComponentId::Php, "8.2.0", true);
    inst.source = InstallSource::External;
    f.installations = vec![inst];
    let mut h = harness(f);
    open_menu(&mut h, "More actions", "Remove 8.2.0 from Zenv");
    h.get_by_label("Remove php 8.2.0 from Zenv?");
    h.get_by_label("Remove").click();
    h.run_steps(3);
    assert_eq!(task_keys(&h), ["installation:unregister:php-8.2.0"]);
}

#[test]
fn version_row_menu_sets_up_a_server_and_switches_frankenphp() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Frankenphp);
    f.installations = vec![install(ComponentId::Frankenphp, "1.4.0", false)];
    let mut h = harness(f);
    open_menu(&mut h, "More for 1.4.0", "Set up a server on 1.4.0");
    assert!(matches!(h.state().dialog, Dialog::Service(_)));
    h.state_mut().dialog = Dialog::None;
    h.run_steps(2);
    open_menu(&mut h, "More for 1.4.0", "Switch 1.4.0 off");
    assert_eq!(task_keys(&h), ["installation:enable:frankenphp-1.4.0"]);
}

#[test]
fn server_menu_delete_confirms_first_and_is_disabled_while_running() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    f.services = vec![status("svc-a", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 8080, Health::Stopped)];
    let mut h = harness(f);
    open_menu(&mut h, "More for Apache", "Delete server");
    h.get_by_label("Delete Apache?");
    assert!(task_keys(&h).is_empty());
    h.get_by_label("Delete web server").click();
    h.run_steps(3);
    assert_eq!(task_keys(&h), ["service:delete:svc-a"]);

    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    f.services = vec![status("svc-a", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 8080, Health::Running)];
    let mut h = harness(f);
    h.get_by_label("More for Apache").click();
    h.run_steps(2);
    h.get_by_label("Reload configuration");
    h.get_by_label("Restart");
    h.get_by_label("Delete server (stop it first)").click();
    h.run_steps(2);
    open_menu(&mut h, "More for Apache", "Edit (stop it first)");
    h.run_steps(2);
    assert!(h.query_by_label("Delete Apache?").is_none());
    assert!(matches!(h.state().dialog, Dialog::None));
    assert!(task_keys(&h).is_empty());
}

#[test]
fn mysql_without_data_offers_initialize_and_blocks_the_switch() {
    use zenv_core::web_server::{DataDirInfo, DataDirState};
    let mut f = fixture(Category::Database, "Databases", ComponentId::Mysql);
    f.installations = vec![install(ComponentId::Mysql, "8.4.3", true)];
    let mut s = status("db-1", "MySQL", ComponentId::Mysql, Category::Database, "mysql-8.4.3", 3306, Health::Stopped);
    s.needs_initialization = true;
    s.data_dir_info = Some(DataDirInfo::new(DataDirState::Empty, "empty"));
    s.service.data_dir = Some(PathBuf::from("C:/zenv/data/mysql"));
    f.services = vec![s];
    let mut h = harness(f);
    h.get_by_label("The data folder is not initialized yet. Initialize it once, then switch the server on.");
    h.get_by_label("Start MySQL").click();
    h.run_steps(2);
    assert!(task_keys(&h).is_empty(), "the switch is disabled until the folder exists");
    h.get_by_label("Initialize").click();
    h.run_steps(2);
    h.get_by_label("Initialize the data folder?");
    h.get_by_label("C:/zenv/data/mysql");
    h.get_all_by_label("Initialize").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(task_keys(&h), ["service:init:db-1"]);
}

#[test]
fn foreign_data_folder_shows_the_error_notice() {
    use zenv_core::web_server::{DataDirInfo, DataDirState};
    let mut f = fixture(Category::Database, "Databases", ComponentId::Mysql);
    f.installations = vec![install(ComponentId::Mysql, "8.4.3", true)];
    let mut s = status("db-1", "MySQL", ComponentId::Mysql, Category::Database, "mysql-8.4.3", 3306, Health::Stopped);
    s.data_dir_info = Some(DataDirInfo::new(DataDirState::Foreign, "it holds other files"));
    f.services = vec![s];
    let h = harness(f);
    h.get_by_label("Zenv will not use this data folder: it holds other files");
}

#[test]
fn logs_and_activity_toggles_the_details_panel() {
    let mut f = fixture(Category::WebServer, "Web Servers", ComponentId::Apache);
    f.installations = vec![install(ComponentId::Apache, "2.4.62", true)];
    f.services = vec![status("svc-a", "Apache", ComponentId::Apache, Category::WebServer, "apache-2.4.62", 8080, Health::Stopped)];
    let mut h = harness(f);
    assert!(h.query_by_label("Activity").is_none());
    open_menu(&mut h, "More for Apache", "Logs & activity");
    h.get_by_label("Activity").click();
    h.run_steps(2);
    open_menu(&mut h, "More for Apache", "Hide logs & activity");
    assert!(h.query_by_label("Activity").is_none());
}

#[test]
fn notices_store_sets_and_clears_per_service() {
    let rows = components::Rows::default();
    rows.notices.set("a", components::Notice::Success("Configuration is valid.".into()));
    assert_eq!(rows.notices.get("a"), Some(components::Notice::Success("Configuration is valid.".into())));
    assert_eq!(rows.notices.get("b"), None);
    rows.notices.clear("a");
    assert_eq!(rows.notices.get("a"), None);
}
