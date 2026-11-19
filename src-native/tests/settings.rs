use std::path::PathBuf;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::AppInfo;
use zenv_core::paths::AppPaths;
use zenv_core::tls::CaStatus;
use zenv_native::loader::Loader;
use zenv_native::pages::settings::{self, https};
use zenv_native::pages::Route;
use zenv_native::{shell, theme};

fn info() -> AppInfo {
    AppInfo { version: "1.2.3", schema_version: 7, os: "windows", arch: "x86_64", paths: AppPaths::new(r"C:\zenv") }
}

fn ca(exists: bool, trusted_user: bool) -> CaStatus {
    CaStatus {
        exists,
        cert_path: std::path::PathBuf::from(r"C:\zenv\etc\tls\zenv-ca.pem"),
        subject: exists.then(|| "CN=Zenv Local CA".to_owned()),
        sha256: exists.then(|| "ab12cd34".to_owned()),
        not_after: exists.then(|| "2036-01-01T00:00:00Z".to_owned()),
        trusted_user,
        trusted_machine: false,
        suffixes: vec!["test", "localhost", "example"],
    }
}

struct Fixture {
    info: Loader<AppInfo>,
    tls: Loader<CaStatus>,
    https: https::HttpsState,
    busy: Vec<String>,
    actions: Vec<zenv_native::pages::Action>,
}

fn fixture() -> Fixture {
    Fixture { info: Loader::ready(Ok(info())), tls: Loader::ready(Ok(ca(false, false))), https: https::HttpsState::default(), busy: vec![], actions: vec![] }
}

fn page(ui: &mut egui::Ui, f: &mut Fixture) {
    let mut out = Vec::new();
    settings::view(ui, &f.info, &f.tls, &mut f.https, &f.busy, &mut out);
    f.actions.extend(out);
}

fn shell_and_page(ui: &mut egui::Ui, f: &mut Fixture) {
    let actions = shell::frame(ui, Route::Settings, "1.0.0", None, |ui, _| page(ui, f));
    drop(actions);
}

fn harness(f: Fixture) -> Harness<'static, Fixture> {
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 1000.0)).with_context_setup(theme::install).build_ui_state(page, f);
    h.run_steps(3);
    h
}

#[test]
fn shows_version_and_storage_paths() {
    let h = harness(fixture());
    h.get_by_label("1.2.3");
    h.get_by_label("v7");
    h.get_by_label("windows / x86_64");
    h.get_by_label(r"C:\zenv");
    h.get_by_label(r"C:\zenv\logs");
}

#[test]
fn opening_zenv_home_emits_a_file_url() {
    let f = fixture();
    let mut h = harness(f);
    let opens = h.get_all_by_label("Open").count();
    assert!(opens >= 2, "Zenv home and Logs both offer Open");
    h.get_all_by_label("Open").next().unwrap().click();
    h.run_steps(2);
    assert!(matches!(h.state().actions.as_slice(), [zenv_native::pages::Action::OpenUrl(u)] if u.starts_with("file:///")));
}

#[test]
fn info_loading_and_error_states() {
    let mut f = fixture();
    f.info = Loader::default();
    let h = harness(f);
    h.get_by_label("Loading…");
    let mut f = fixture();
    f.info = Loader::ready(Err("cannot read app info".into()));
    let h = harness(f);
    h.get_by_label("cannot read app info");
}

#[test]
fn https_not_created_offers_to_create_it() {
    let mut f = fixture();
    f.tls = Loader::ready(Ok(ca(false, false)));
    let h = harness(f);
    h.get_by_label("Not created");
    h.get_by_label("Create certificate authority");
}

#[test]
fn https_trusted_offers_to_remove_trust() {
    let mut f = fixture();
    f.tls = Loader::ready(Ok(ca(true, true)));
    let h = harness(f);
    h.get_by_label("Trusted");
    h.get_by_label("Remove trust…");
}

#[test]
fn trusting_requires_confirmation_then_emits_a_task() {
    let mut f = fixture();
    f.tls = Loader::ready(Ok(ca(true, false)));
    let mut h = harness(f);
    h.get_by_label("Trust in Windows…").click();
    h.run_steps(2);
    h.get_by_label("Trust Zenv's certificate authority?");
    h.get_by_label("Continue to Windows").click();
    h.run_steps(2);
    let keys: Vec<String> = h
        .state()
        .actions
        .iter()
        .filter_map(|a| match a {
            zenv_native::pages::Action::Run(t) => Some(t.key.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [https::TRUST_KEY]);
}

#[test]
#[ignore = "renders PNGs for a look review: cargo test -p zenv-native --test settings -- --ignored"]
fn render_settings_png() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/target/native-shots");
    std::fs::create_dir_all(&dir).unwrap();

    let mut missing = fixture();
    missing.tls = Loader::ready(Ok(ca(false, false)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 820.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .with_accessibility_check(false)
        .build_ui_state(shell_and_page, missing);
    h.run();
    h.render().unwrap().save(dir.join("native-Settings-ca-missing.png")).unwrap();

    let mut trusted = fixture();
    trusted.tls = Loader::ready(Ok(ca(true, true)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 820.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .with_accessibility_check(false)
        .build_ui_state(shell_and_page, trusted);
    h.run();
    h.render().unwrap().save(dir.join("native-Settings-ca-trusted.png")).unwrap();

    let mut pending = fixture();
    pending.tls = Loader::ready(Ok(ca(true, false)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 820.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .with_accessibility_check(false)
        .build_ui_state(shell_and_page, pending);
    h.run();
    h.get_by_label("Trust in Windows…").click();
    h.run_steps(2);
    h.render().unwrap().save(dir.join("native-Settings-ca-confirm.png")).unwrap();
}

#[test]
fn cancelling_the_confirmation_emits_nothing() {
    let mut f = fixture();
    f.tls = Loader::ready(Ok(ca(true, false)));
    let mut h = harness(f);
    h.get_by_label("Trust in Windows…").click();
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().actions.is_empty());
    assert!(h.query_by_label("Trust Zenv's certificate authority?").is_none());
}
