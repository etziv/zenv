use std::collections::HashMap;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::runtime_catalog::{self, Category, ComponentId, ComponentInfo};
use zenv_core::runtime_installer::{InstallJob, Phase};
use zenv_native::pages::components::{self, Data, Dialog};
use zenv_native::theme;

struct State {
    components: Vec<ComponentInfo>,
    jobs: Vec<InstallJob>,
    picked: ComponentId,
    dialog: Dialog,
    errors: HashMap<String, String>,
    routers: std::cell::RefCell<zenv_native::pages::tools::Routers>,
    rows: std::cell::RefCell<components::Rows>,
}

fn page(ui: &mut egui::Ui, s: &mut State) {
    let icons = HashMap::new();
    let path_status = HashMap::new();
    let data = Data {
        category: Category::Language,
        title: "Languages",
        components: &s.components,
        installations: &[],
        runtimes: &[],
        services: &[],
        icons: &icons,
        path_status: &path_status,
        jobs: &s.jobs,
        discover: None,
        available: None,
        errors: &s.errors,
        routers: &s.routers,
        rows: &s.rows,
    };
    let mut out = Vec::new();
    let mut scan = false;
    components::view(ui, &data, &mut s.picked, &mut s.dialog, &[], &mut scan, &mut out);
}

fn state(phase: Phase, error: Option<&str>) -> State {
    State {
        components: runtime_catalog::catalog().iter().filter(|c| c.category == Category::Language).cloned().collect(),
        jobs: vec![InstallJob {
            id: "job-1".into(),
            component: ComponentId::Php,
            version: "8.4.1".into(),
            phase,
            downloaded: 0,
            total: 0,
            error: error.map(Into::into),
            installation_id: None,
            started_at: zenv_core::util::now_rfc3339(),
        }],
        picked: ComponentId::Php,
        dialog: Dialog::None,
        errors: HashMap::new(),
        routers: Default::default(),
        rows: Default::default(),
    }
}

#[test]
fn control_an_active_install_shows_its_progress_row() {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 900.0)).with_context_setup(theme::install).build_ui_state(page, state(Phase::Downloading, None));
    h.run_steps(3);
    assert!(h.query_by_label_contains("8.4.1").is_some());
}

#[test]
fn a_failed_install_shows_its_error_like_react_component_installs() {
    let state = State {
        components: runtime_catalog::catalog().iter().filter(|c| c.category == Category::Language).cloned().collect(),
        jobs: vec![InstallJob {
            id: "job-1".into(),
            component: ComponentId::Php,
            version: "8.4.1".into(),
            phase: Phase::Failed,
            downloaded: 0,
            total: 0,
            error: Some("checksum mismatch".into()),
            installation_id: None,
            started_at: zenv_core::util::now_rfc3339(),
        }],
        picked: ComponentId::Php,
        dialog: Dialog::None,
        errors: HashMap::new(),
        routers: Default::default(),
        rows: Default::default(),
    };
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 900.0)).with_context_setup(theme::install).build_ui_state(page, state);
    h.run_steps(3);
    assert!(h.query_by_label_contains("checksum mismatch").is_some(), "the failed job's error is shown nowhere");
}
