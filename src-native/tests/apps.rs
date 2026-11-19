use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::Core;
use zenv_core::docker_apps::{ComposeService, DockerContainer, PublishedPort};
use zenv_core::domain_manager::hosts::{DomainState, HostsConflict, HostsEntry, HostsPlan};
use zenv_core::domain_manager::Resolution;
use zenv_core::editors::{EditorId, EditorInfo, Editors};
use zenv_core::local_sites::compat::ServerCompat;
use zenv_core::local_sites::{ConfigStatus, HostsStatus, PhpEngine, ProjectType, Site, SitePage, SiteView, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::runtime_manager::{InstallSource, Installation};
use zenv_core::service_manager::{Health, Service, ServiceStatus};
use zenv_core::tls::CaStatus;
use zenv_native::loader::Loader;
use zenv_native::pages::apps::docker_picker::PickedContainer;
use zenv_native::pages::apps::form::Source;
use zenv_native::pages::apps::hosts_dialog::{Applied, HostsApply};
use zenv_native::pages::apps::{form::FormState, AppsPage, DetailsTab, Overlay};
use zenv_native::pages::{Action, Cx, Route, Shared};
use zenv_native::{shell, theme};

fn dummy_core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("zenv-native-apps-test-{}", std::process::id()));
        let paths = AppPaths::new(dir);
        Arc::new(Core::initialize(paths).expect("test core initializes"))
    })
    .clone()
}

fn site(id: &str, domain: &str, strategy: Strategy, enabled: bool) -> SiteView {
    SiteView {
        site: Site {
            id: id.into(),
            name: id.into(),
            domain: domain.into(),
            project_root: PathBuf::from(format!("C:/projects/{id}")),
            document_root: PathBuf::from(format!("C:/projects/{id}/public")),
            project_type: ProjectType::Static,
            service_id: None,
            strategy,
            upstream_port: None,
            runtime_installation_id: None,
            php_engine: PhpEngine::Auto,
            https_enabled: false,
            https_redirect: false,
            enabled,
            config_status: ConfigStatus::Applied,
            config_message: None,
            created_at: String::new(),
            updated_at: String::new(),
        },
        service_name: "Apache".into(),
        url: format!("http://{domain}"),
        resolution: Resolution::HostsFile,
        hosts_state: DomainState::Managed,
        document_root_exists: true,
        php_version: None,
        php_backend: None,
        compat: ServerCompat::default(),
    }
}

fn web_server(id: &str, name: &str) -> ServiceStatus {
    ServiceStatus {
        service: Service {
            id: id.into(),
            component: ComponentId::Apache,
            name: name.into(),
            installation_id: String::new(),
            listen_address: "127.0.0.1".into(),
            http_port: 80,
            autostart: false,
            created_at: String::new(),
            updated_at: String::new(),
            data_dir: None,
            https_port: Some(443),
        },
        installation: None,
        health: Health::Running,
        process: zenv_core::process_manager::ProcessState::NotStarted,
        url: "http://localhost:80".into(),
        config_path: PathBuf::new(),
        document_root: PathBuf::new(),
        kind: Category::WebServer,
        data_dir_info: None,
        connection: None,
        behind: None,
        needs_initialization: false,
    }
}

struct Fixture {
    core: Arc<Core>,
    shared: Shared,
    page: AppsPage,
    busy: Vec<String>,
    actions: Vec<Action>,
}

fn fixture(sites: Vec<SiteView>, web_servers: Vec<ServiceStatus>) -> Fixture {
    let total = sites.len() as u64;
    let mut shared = Shared::default();
    shared.services = Loader::ready(Ok(web_servers));
    let page = AppsPage { sites: Loader::ready(Ok(SitePage { items: sites, total, offset: 0, limit: 48 })), ..Default::default() };
    Fixture { core: dummy_core(), shared, page, busy: vec![], actions: vec![] }
}

fn fixture_with_error(sites: Vec<SiteView>, web_servers: Vec<ServiceStatus>, key: &str, message: &str) -> Fixture {
    let mut f = fixture(sites, web_servers);
    f.shared.errors.insert(key.to_owned(), message.to_owned());
    f
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let mut cx = Cx { core: &f.core, shared: &f.shared, busy: &f.busy, tasks_finished: false, actions: Vec::new() };
    f.page.ui(ui, &mut cx);
    f.actions.extend(cx.actions);
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

fn task_keys(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Run(t) => Some(t.key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn renders_grid_and_panel_for_selected_app() {
    let h = harness(fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]));
    h.get_by_label("Open");
    h.get_by_label("C:/projects/s1");
    h.get_by_label("Open in browser");
}

#[test]
fn selecting_a_card_switches_the_panel() {
    let mut h = harness(fixture(
        vec![site("s1", "a.test", Strategy::Static, true), site("s2", "b.test", Strategy::Static, true)],
        vec![web_server("apache", "Apache")],
    ));
    assert!(h.query_by_label("C:/projects/s2").is_none());
    h.get_by_label("b.test").click();
    h.run_steps(2);
    h.get_by_label("C:/projects/s2");
}

#[test]
fn enable_switch_emits_the_right_task_key() {
    let mut h = harness(fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]));
    h.get_by_role_and_label(egui::Role::Switch, "Serve this app").click();
    h.run_steps(2);
    assert!(task_keys(&h.state().actions).contains(&"site:s1:enable".to_owned()));
}

#[test]
fn delete_confirm_emits_the_right_task_key() {
    let mut f = fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]);
    f.page.overlay = Some(Overlay::Delete { site_id: "s1".into(), domain: "a.test".into() });
    let mut h = harness(f);
    h.get_by_label("Remove app").click();
    h.run_steps(2);
    assert!(task_keys(&h.state().actions).contains(&"site:s1:delete".to_owned()));
}

#[test]
fn create_emits_the_right_task_key() {
    let mut f = fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]);
    let mut form = FormState::create(&[]);
    form.step = 3;
    form.name = "shop".into();
    form.domain = "shop.test".into();
    form.project_root = "C:/projects/shop".into();
    form.strategy = Strategy::Static;
    f.page.overlay = Some(Overlay::Form(Box::new(form)));
    let mut h = harness(f);
    h.get_by_label("Create app").click();
    h.run_steps(2);
    assert!(task_keys(&h.state().actions).contains(&"site:create".to_owned()));
}

#[test]
fn empty_state_without_web_server() {
    let h = harness(fixture(vec![], vec![]));
    h.get_by_label("Go to Web Servers");
}

#[test]
fn empty_state_with_web_server_but_no_apps() {
    let h = harness(fixture(vec![], vec![web_server("apache", "Apache")]));
    h.get_by_label("No apps yet: pick a project folder to get started.");
}

#[test]
fn inline_save_error_shows_under_the_panel() {
    let f = fixture_with_error(
        vec![site("s1", "a.test", Strategy::Static, true)],
        vec![web_server("apache", "Apache")],
        "site:s1:save",
        "domain already in use",
    );
    let h = harness(f);
    h.get_by_label("domain already in use");
}

#[test]
fn inline_delete_error_shows_below_the_grid_with_the_dialog_closed() {
    let f = fixture_with_error(
        vec![site("s1", "a.test", Strategy::Static, true)],
        vec![web_server("apache", "Apache")],
        "site:s1:delete",
        "app is still running",
    );
    let h = harness(f);
    h.get_by_label("app is still running");
    assert!(h.query_by_label("Remove app").is_none());
}

#[test]
fn inline_create_error_keeps_the_dialog_open() {
    let mut f = fixture_with_error(
        vec![site("s1", "a.test", Strategy::Static, true)],
        vec![web_server("apache", "Apache")],
        "site:create",
        "a site with that domain already exists",
    );
    let mut form = FormState::create(&[]);
    form.step = 3;
    form.name = "shop".into();
    form.domain = "shop.test".into();
    form.project_root = "C:/projects/shop".into();
    form.strategy = Strategy::Static;
    f.page.overlay = Some(Overlay::Form(Box::new(form)));
    let h = harness(f);
    h.get_by_label("a site with that domain already exists");
    h.get_by_label("Create app");
}

#[test]
fn error_state_shows_the_message() {
    let mut f = fixture(vec![], vec![web_server("apache", "Apache")]);
    f.page.sites = Loader::ready(Err("database is locked".into()));
    let h = harness(f);
    h.get_by_label("database is locked");
}

// ---------- render pngs

fn php_site(id: &str, domain: &str, enabled: bool) -> SiteView {
    let mut sv = site(id, domain, Strategy::Php, enabled);
    sv.site.name = domain.split('.').next().unwrap_or(domain).into();
    sv.site.project_type = ProjectType::Laravel;
    sv.site.service_id = Some("apache".into());
    sv.site.runtime_installation_id = Some("php-8.3.14".into());
    sv.site.https_enabled = true;
    sv.php_version = Some("8.3.14".into());
    sv.compat = ServerCompat { frankenphp_ready: true, needs_apache: vec![] };
    sv
}

fn php_install() -> Installation {
    Installation {
        id: "php-8.3.14".into(),
        component: ComponentId::Php,
        version: "8.3.14".into(),
        version_detail: String::new(),
        root_path: PathBuf::from("C:/zenv/bin/php/8.3.14"),
        executable_path: PathBuf::new(),
        source: InstallSource::Managed,
        is_default: true,
        enabled: true,
        executable_exists: true,
        detected_at: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn editors() -> Editors {
    Editors {
        editors: vec![EditorInfo { id: EditorId::Vscode, name: "VS Code", program: PathBuf::from("code"), icon: None }],
        preferred: Some(EditorId::Vscode),
    }
}

fn tls_status() -> CaStatus {
    CaStatus {
        exists: true,
        cert_path: PathBuf::from("C:/zenv/data/ca/zenv-ca.pem"),
        subject: Some("Zenv Local CA".into()),
        sha256: Some("ab12cd34".into()),
        not_after: Some("2030-01-01".into()),
        trusted_user: true,
        trusted_machine: false,
        suffixes: vec!["test", "localhost"],
    }
}

fn hosts_status(changed: bool) -> HostsStatus {
    let plan = if changed {
        HostsPlan {
            content: String::new(),
            changed: true,
            add: vec![HostsEntry { ip: "127.0.0.1".into(), domain: "shop.test".into() }, HostsEntry { ip: "127.0.0.1".into(), domain: "blog.test".into() }],
            remove: vec![HostsEntry { ip: "127.0.0.1".into(), domain: "old.test".into() }],
            already_mapped: vec![],
            conflicts: vec![HostsConflict { domain: "api.test".into(), mapped_to: "10.0.0.5".into() }],
            block: vec!["127.0.0.1 shop.test".into(), "127.0.0.1 blog.test".into()],
        }
    } else {
        HostsPlan { content: String::new(), changed: false, add: vec![], remove: vec![], already_mapped: vec![], conflicts: vec![], block: vec![] }
    };
    HostsStatus { path: PathBuf::from("C:/Windows/System32/drivers/etc/hosts"), plan }
}

fn docker_container() -> DockerContainer {
    DockerContainer {
        name: "shop_web_1".into(),
        image: "node:20-alpine".into(),
        state: "running".into(),
        status: "Up 2 hours".into(),
        ports: vec![PublishedPort { host_port: 3000, container_port: 3000 }],
        compose: Some(ComposeService { project: "shop".into(), service: "web".into(), dir: Some("D:/projects/shop".into()), files: vec!["docker-compose.yml".into()] }),
    }
}

fn render_fixture(sites: Vec<SiteView>) -> Fixture {
    let mut f = fixture(sites, vec![web_server("apache", "Apache")]);
    f.page.installations = Loader::ready(Ok(vec![php_install()]));
    f.page.editors = Loader::ready(Ok(editors()));
    f.page.tls = Loader::ready(Ok(tls_status()));
    f.page.hosts = Loader::ready(Ok(hosts_status(false)));
    f.page.docker_containers = Loader::ready(Ok(vec![docker_container()]));
    f
}

fn shell_and_page(ui: &mut egui::Ui, f: &mut Fixture) {
    let services = f.shared.services.ok().cloned();
    let mut cx = Cx { core: &f.core, shared: &f.shared, busy: &f.busy, tasks_finished: false, actions: Vec::new() };
    let actions = shell::frame(ui, Route::Apps, "1.0.0", services.as_deref(), |ui, _out| f.page.ui(ui, &mut cx));
    f.actions.extend(cx.actions);
    drop(actions);
}

fn docker_picked() -> PickedContainer {
    PickedContainer { container: docker_container(), port: PublishedPort { host_port: 3000, container_port: 3000 } }
}

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test apps -- --ignored"]
fn render_apps_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();

    let three_sites = || vec![php_site("s1", "shop.test", true), php_site("s2", "blog.test", true), php_site("s3", "api.test", false)];

    let mut grid = render_fixture(three_sites());
    grid.page.selected = Some("s1".into());

    let empty = render_fixture(vec![]);

    let mut hosts = render_fixture(three_sites());
    hosts.page.selected = Some("s1".into());
    hosts.page.hosts = Loader::ready(Ok(hosts_status(true)));
    hosts.page.overlay = Some(Overlay::Hosts(HostsApply::default()));

    let mut wizard1 = render_fixture(three_sites());
    wizard1.page.selected = Some("s1".into());
    wizard1.page.overlay = Some(Overlay::Form(Box::new(FormState::create(&[php_install()]))));

    let mut wizard2 = render_fixture(three_sites());
    wizard2.page.selected = Some("s1".into());
    let mut form2 = FormState::create(&[php_install()]);
    form2.step = 2;
    form2.source = Source::Docker;
    form2.picked = Some(docker_picked());
    form2.name = "shop-web".into();
    form2.domain = "shop-web.test".into();
    form2.project_type = ProjectType::Custom;
    form2.strategy = Strategy::ReverseProxy;
    form2.upstream_port = "3000".into();
    wizard2.page.overlay = Some(Overlay::Form(Box::new(form2)));

    let mut wizard3 = render_fixture(three_sites());
    wizard3.page.selected = Some("s1".into());
    let mut form3 = FormState::create(&[php_install()]);
    form3.step = 3;
    form3.source = Source::Docker;
    form3.picked = Some(docker_picked());
    form3.name = "shop-web".into();
    form3.domain = "shop-web.test".into();
    form3.project_type = ProjectType::Custom;
    form3.strategy = Strategy::ReverseProxy;
    form3.upstream_port = "3000".into();
    wizard3.page.overlay = Some(Overlay::Form(Box::new(form3)));

    let mut details = render_fixture(three_sites());
    details.page.selected = Some("s1".into());
    details.page.details_shown = Some(("s1".into(), DetailsTab::Diagnose));
    details.page.diagnose = Loader::ready(Ok(vec![
        zenv_core::diagnostics::check("writable", "Document root is writable", zenv_core::diagnostics::CheckStatus::Ok, "D:/projects/shop/public"),
        zenv_core::diagnostics::check("hosts", "Hosts file entry", zenv_core::diagnostics::CheckStatus::Ok, "shop.test -> 127.0.0.1"),
        zenv_core::diagnostics::check(
            "php",
            "PHP version",
            zenv_core::diagnostics::CheckStatus::Warning,
            "Using PHP 8.3.14 by default; no version pinned for this app.",
        ),
    ]));
    details.page.overlay = Some(Overlay::Details { site_id: "s1".into(), tab: DetailsTab::Diagnose });

    let mut banner = render_fixture(three_sites());
    banner.page.selected = Some("s1".into());
    banner.page.hosts = Loader::ready(Ok(hosts_status(true)));

    let mut delete = render_fixture(three_sites());
    delete.page.selected = Some("s1".into());
    delete.page.overlay = Some(Overlay::Delete { site_id: "s1".into(), domain: "shop.test".into() });

    for (name, f) in [
        ("grid", grid),
        ("empty", empty),
        ("hosts", hosts),
        ("hosts-banner", banner),
        ("wizard-step1", wizard1),
        ("wizard-step2", wizard2),
        ("wizard-step3", wizard3),
        ("details", details),
        ("delete", delete),
    ] {
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 820.0)).with_theme(egui::Theme::Dark).with_context_setup(theme::install).build_ui_state(shell_and_page, f);
        h.run_steps(3);
        h.render().unwrap().save(dir.join(format!("native-Apps-{name}.png"))).unwrap();
    }
}

fn hosts_fixture(apply: &HostsApply) -> Fixture {
    let mut f = fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]);
    f.page.hosts = Loader::ready(Ok(hosts_status(true)));
    f.page.overlay = Some(Overlay::Hosts(apply.clone()));
    f
}

#[test]
fn hosts_dialog_lists_changes_conflicts_and_the_preview() {
    let mut h = harness(hosts_fixture(&HostsApply::default()));
    h.get_by_label("127.0.0.1 → shop.test");
    h.get_by_label("Left alone: your own lines point these domains elsewhere");
    h.get_by_label("api.test → 10.0.0.5");
    h.get_by_label("127.0.0.1 shop.test\n127.0.0.1 blog.test");
    h.get_by_label("Cancel");
    h.get_by_label("Update hosts file").click();
    h.run_steps(2);
    assert!(task_keys(&h.state().actions).contains(&"hosts:apply".to_owned()));
}

#[test]
fn hosts_dialog_after_success_shows_the_backup_and_only_close() {
    let apply = HostsApply::default();
    apply.set(Applied::Done { unchanged: false, backup: Some(PathBuf::from("C:/zenv/hosts/hosts.bak")) });
    let h = harness(hosts_fixture(&apply));
    h.get_by_label("Hosts file updated. New domains work immediately.");
    h.get_by_label("Previous version saved to C:/zenv/hosts/hosts.bak");
    assert_eq!(h.query_all_by_label("Close").count(), 2);
    assert!(h.query_by_label("Update hosts file").is_none());
}

#[test]
fn hosts_dialog_when_permission_declined_explains_and_allows_retry() {
    let apply = HostsApply::default();
    apply.set(Applied::Declined);
    let h = harness(hosts_fixture(&apply));
    h.get_by_label("Nothing was changed because permission was declined.");
    h.get_by_label("Update hosts file");
    h.get_by_label("Cancel");
}

fn editor_fixture(list: Vec<EditorInfo>) -> Fixture {
    let mut f = fixture(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")]);
    f.page.selected = Some("s1".into());
    f.page.editors = Loader::ready(Ok(Editors { editors: list, preferred: None }));
    f
}

#[test]
fn editor_menu_lists_every_editor_and_marks_the_default() {
    let cursor = EditorInfo { id: EditorId::Cursor, name: "Cursor", program: PathBuf::from("cursor"), icon: None };
    let mut h = harness(editor_fixture(vec![editors().editors.remove(0), cursor]));
    h.get_by_label("Choose editor").click();
    h.run_steps(2);
    h.get_by_label(&zenv_native::widgets::tracked("OPEN IN"));
    h.get_by_label_contains("VS Code default");
    h.get_by_label_contains("Cursor").click();
    h.run_steps(2);
    assert!(task_keys(&h.state().actions).contains(&"editor:s1".to_owned()));
}

#[test]
fn editor_button_without_editors_is_disabled_and_says_so() {
    let mut h = harness(editor_fixture(vec![]));
    h.get_by_label_contains("No editor").click();
    h.run_steps(2);
    assert!(!task_keys(&h.state().actions).contains(&"editor:s1".to_owned()));
    h.get_by_label("Choose editor").click();
    h.run_steps(2);
    h.get_by_label("No editor found: VS Code, Cursor, Windsurf, Zed, PhpStorm, Sublime…");
}

#[test]
fn a_failed_preview_shows_quietly_in_the_panel() {
    let mut f = fixture_with_error(vec![site("s1", "a.test", Strategy::Static, true)], vec![web_server("apache", "Apache")], "thumbnail:s1", "no browser found");
    f.page.selected = Some("s1".into());
    let h = harness(f);
    h.get_by_label("Preview: no browser found");
}
