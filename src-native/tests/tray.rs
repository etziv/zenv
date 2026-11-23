use std::path::PathBuf;

use eframe::egui::{self, pos2, vec2, Rect};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use tray_icon::dpi::{PhysicalPosition, PhysicalSize};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use zenv_core::process_manager::ProcessState;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::service_manager::{Health, Service, ServiceStatus};
use zenv_native::pages::dashboard::{service_key, START_APPS};
use zenv_native::pages::Action;
use zenv_native::panel::{self, Out, View};
use zenv_native::theme;
use zenv_native::tray::{self, Msg};

fn status(id: &str, name: &str, port: u16, health: Health) -> ServiceStatus {
    ServiceStatus {
        service: Service {
            id: id.into(),
            component: ComponentId::Apache,
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
        url: String::new(),
        config_path: PathBuf::new(),
        document_root: PathBuf::new(),
        kind: Category::WebServer,
        data_dir_info: None,
        connection: None,
        behind: None,
        needs_initialization: false,
    }
}

#[test]
fn one_home_gets_one_instance_name_however_it_is_typed() {
    let home = std::env::temp_dir().join("zenv-instance-test");
    std::fs::create_dir_all(&home).unwrap();
    let plain = zenv_native::os::instance_name(&home);
    let typed = home.display().to_string().to_uppercase() + "\\";
    assert_eq!(zenv_native::os::instance_name(std::path::Path::new(&typed)), plain);
    let object = plain.strip_prefix("Local\\zenv-native-").unwrap();
    assert!(!object.contains('\\') && !object.ends_with('/') && !object.starts_with("//?/"), "{plain}");
}

#[test]
fn tooltip_names_up_to_three_running_services() {
    let mut list = vec![status("a", "Apache", 80, Health::Stopped)];
    assert_eq!(tray::tooltip(&list), "Zenv · nothing running");
    list[0].health = Health::Running;
    list.push(status("b", "MySQL", 3306, Health::Running));
    assert_eq!(tray::tooltip(&list), "Zenv · Apache, MySQL");
    list.push(status("c", "Redis", 6379, Health::Running));
    list.push(status("d", "Nginx", 8080, Health::Running));
    assert_eq!(tray::tooltip(&list), "Zenv · 4 services running");
}

const WORK: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1920.0, 1032.0) };
const PANEL: egui::Vec2 = vec2(300.0, 320.0);

#[test]
fn panel_sits_above_a_bottom_taskbar_icon() {
    let icon = Rect::from_min_size(pos2(1700.0, 1040.0), vec2(24.0, 24.0));
    let pos = tray::place(icon, WORK, PANEL, 1.0);
    assert_eq!(pos, pos2(1712.0 - 150.0, 1032.0 - 320.0 - 10.0));
}

#[test]
fn panel_stays_inside_the_work_area_near_the_screen_edge() {
    let icon = Rect::from_min_size(pos2(1900.0, 1040.0), vec2(24.0, 24.0));
    let pos = tray::place(icon, WORK, PANEL, 1.0);
    assert_eq!(pos.x, 1920.0 - 300.0 - 10.0);
}

#[test]
fn panel_drops_below_a_top_taskbar_with_scaled_gap() {
    let work = Rect::from_min_max(pos2(0.0, 48.0), pos2(1920.0, 1080.0));
    let icon = Rect::from_min_size(pos2(1700.0, 8.0), vec2(32.0, 32.0));
    let pos = tray::place(icon, work, PANEL * 2.0, 2.0);
    assert_eq!(pos.y, 48.0 + 20.0);
}

#[test]
fn panel_sits_beside_a_side_taskbar() {
    let work = Rect::from_min_max(pos2(60.0, 0.0), pos2(1920.0, 1080.0));
    let icon = Rect::from_min_size(pos2(18.0, 900.0), vec2(24.0, 24.0));
    let pos = tray::place(icon, work, PANEL, 1.0);
    assert_eq!(pos, pos2(70.0, 1080.0 - 320.0 - 10.0));
}

fn click(button: MouseButton, state: MouseButtonState) -> TrayIconEvent {
    TrayIconEvent::Click {
        id: "zenv".into(),
        position: PhysicalPosition::new(1710.0, 1050.0),
        rect: tray_icon::Rect { size: PhysicalSize::new(24, 24), position: PhysicalPosition::new(1700.0, 1040.0) },
        button,
        button_state: state,
    }
}

#[test]
fn left_release_toggles_the_panel_and_double_click_opens_zenv() {
    let expected = Rect::from_min_size(pos2(1700.0, 1040.0), vec2(24.0, 24.0));
    assert_eq!(tray::on_tray(&click(MouseButton::Left, MouseButtonState::Up)), Some(Msg::TogglePanel(expected)));
    assert_eq!(tray::on_tray(&click(MouseButton::Left, MouseButtonState::Down)), None);
    assert_eq!(tray::on_tray(&click(MouseButton::Right, MouseButtonState::Up)), None);
    let double = TrayIconEvent::DoubleClick {
        id: "zenv".into(),
        position: PhysicalPosition::new(0.0, 0.0),
        rect: tray_icon::Rect::default(),
        button: MouseButton::Left,
    };
    assert_eq!(tray::on_tray(&double), Some(Msg::ShowMain));
}

struct Fixture {
    services: Option<Vec<ServiceStatus>>,
    busy: Vec<String>,
    error: Option<(String, String)>,
    outs: Vec<Out>,
    height: f32,
}

fn harness(services: Option<Vec<ServiceStatus>>, busy: Vec<String>, error: Option<(&str, &str)>) -> Harness<'static, Fixture> {
    let error = error.map(|(t, d)| (t.to_owned(), d.to_owned()));
    let f = Fixture { services, busy, error, outs: vec![], height: 0.0 };
    let mut h = Harness::builder().with_size(vec2(panel::WIDTH, 460.0)).with_context_setup(theme::install).build_ui_state(
        |ui, f: &mut Fixture| {
            let view = View {
                services: f.services.as_deref(),
                version: "1.0.0",
                busy: &f.busy,
                error: f.error.as_ref().map(|(t, d)| (t.as_str(), d.as_str())),
            };
            let (outs, height) = panel::ui(ui, &view);
            f.outs.extend(outs);
            f.height = height;
        },
        f,
    );
    h.run_steps(3);
    h
}

fn sample() -> Vec<ServiceStatus> {
    let mut nginx = status("nginx", "Nginx", 80, Health::Stopped);
    nginx.behind = Some("Apache".into());
    vec![status("apache", "Apache", 80, Health::Running), nginx, status("mysql", "MySQL", 3306, Health::Failed)]
}

fn disabled(h: &Harness<'_, Fixture>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

#[test]
fn groups_running_first_with_counts_and_fits_its_content() {
    let h = harness(Some(sample()), vec![], None);
    h.get_by_label("RUNNING · 1");
    h.get_by_label("STOPPED · 2");
    h.get_by_label("1 running");
    h.get_by_label("Stop Apache");
    h.get_by_label("Start MySQL");
    h.get_by_label("via Apache");
    assert!(!disabled(&h, "Start apps"));
    assert!(!disabled(&h, "Stop all"));
    let height = h.state().height;
    assert!(height > 150.0 && height < 460.0, "{height}");
}

#[test]
fn nothing_running_disables_stop_all_and_names_the_group_services() {
    let list = vec![status("apache", "Apache", 80, Health::Stopped)];
    let h = harness(Some(list), vec![], None);
    h.get_by_label("all stopped");
    h.get_by_label("SERVICES · 1");
    assert!(h.query_by_label("RUNNING · 0").is_none());
    assert!(disabled(&h, "Stop all"));
}

#[test]
fn empty_and_loading_states_disable_both_actions() {
    let h = harness(Some(vec![]), vec![], None);
    h.get_by_label("No services yet. Open Zenv to add a web server or database.");
    assert!(disabled(&h, "Start apps"));
    assert!(disabled(&h, "Stop all"));
    let h = harness(None, vec![], None);
    h.get_by_label("Loading services…");
}

#[test]
fn buttons_and_switches_emit_their_actions() {
    let mut h = harness(Some(sample()), vec![], None);
    for label in ["Stop Apache", "Start apps", "Open Zenv", "Quit"] {
        h.get_by_label(label).click();
        h.run_steps(2);
    }
    let outs = &h.state().outs;
    assert!(matches!(&outs[0], Out::Act(Action::Run(t)) if t.key == service_key("apache") && !t.inline));
    assert!(matches!(&outs[1], Out::Act(Action::Run(t)) if t.key == START_APPS));
    assert!(matches!(outs[2], Out::OpenMain));
    assert!(matches!(outs[3], Out::Quit));
}

#[test]
fn a_pending_start_shows_starting_and_blocks_a_second_one() {
    let h = harness(Some(sample()), vec![START_APPS.into()], None);
    assert!(disabled(&h, "Starting…"));
}

#[test]
fn a_long_error_is_cut_to_three_lines_and_leaves_room_for_the_list() {
    let short = harness(Some(sample()), vec![], Some(("Couldn't start MySQL", "port 3306 is in use"))).state().height;
    let long = "the service wrote a very long error message to its log ".repeat(40);
    let h = harness(Some(sample()), vec![], Some(("Couldn't start MySQL", &long)));
    let grown = h.state().height - short;
    assert!(grown > 0.0 && grown < 3.0 * 16.0, "error box grew by {grown}");
}

#[test]
fn the_newest_error_shows_and_can_be_dismissed() {
    let mut h = harness(Some(sample()), vec![], Some(("Couldn't start MySQL", "port 3306 is in use")));
    h.get_by_label("Couldn't start MySQL");
    h.get_by_label("port 3306 is in use");
    h.get_by_label("Dismiss").click();
    h.run_steps(2);
    assert!(h.state().outs.iter().any(|o| matches!(o, Out::Dismiss)));
}
