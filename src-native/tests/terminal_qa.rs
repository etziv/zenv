use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{self, Id, Key, ViewportId};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::studio::Agent;
use zenv_native::studio::terminals::{Host, Phase, Terminals};
use zenv_native::studio::{Colors, Site, Studio};
use zenv_native::theme;

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("zenv-native-terminal-qa-{}", std::process::id()));
        Arc::new(Core::initialize(AppPaths::new(dir)).expect("test core initializes"))
    })
    .clone()
}

#[test]
fn a_wide_terminal_never_covers_the_file_tree() {
    let root = std::env::temp_dir().join(format!("zenv-qa-layout-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.php"), "<?php\n").unwrap();
    let site = Site { id: "qa-layout".into(), domain: "qa.test".into(), root: root.clone(), url: "http://qa.test".into(), enabled: true };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(|ctx| {
            theme::install(ctx);
            ctx.data_mut(|d| {
                d.insert_persisted(Id::unique("zenv.studio.tree"), 520.0f32);
                d.insert_persisted(Id::unique("zenv.studio.terminal"), 70.0f32);
            });
        })
        .build_ui_state(|ui, s: &mut Studio| s.ui(ui), Studio::new(&egui::Context::default(), core(), site));
    h.run_steps(3);
    let plus = h.get_by_label("New tab").rect();
    let _ = std::fs::remove_dir_all(&root);
    assert!(plus.left() > 520.0, "terminal bar starts at x={} inside the 520 px file tree", plus.left());
}

fn create_site(core: &Core) -> String {
    let root = std::env::temp_dir().join(format!("zenv-qa-term-site-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = SiteInput {
        name: "qaterm".into(),
        domain: format!("qaterm{}.test", std::process::id()),
        project_root: root.display().to_string(),
        document_root: root.display().to_string(),
        project_type: ProjectType::Static,
        service_id: None,
        strategy: Strategy::Static,
        upstream_port: None,
        runtime_installation_id: None,
        php_engine: PhpEngine::Auto,
        https_enabled: false,
        https_redirect: false,
        enabled: false,
    };
    core.sites.create(&input).expect("site").site.id
}

fn text(t: &Terminals) -> String {
    t.tabs.iter().find(|x| x.key == t.active).and_then(|x| x.term()).map(|x| x.lock().text()).unwrap_or_default()
}

fn wait(h: &mut Harness<'static, Terminals>, what: &str, done: impl Fn(&Terminals) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}; screen:\n{}", text(h.state()));
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_resize_while_the_program_starts_reaches_the_pty() {
    let core = core();
    let site_id = create_site(&core);
    let host = Host { core: core.clone(), owner: "qa-resize".into(), site_id, viewport: ViewportId::ROOT };
    let mut t = Terminals::new(&egui::Context::default(), host);
    t.add(Agent::Shell, None);
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 500.0)).with_context_setup(theme::install).build_ui_state(
        |ui, t: &mut Terminals| {
            let rect = ui.max_rect();
            let c = Colors::of(ui);
            t.ui(ui, rect, &c);
        },
        t,
    );
    h.step();
    let phase = h.state().tabs[0].phase();
    h.set_size(egui::vec2(1200.0, 500.0));
    h.step();
    let cols = h.state().tabs[0].term().unwrap().lock().size().0;
    wait(&mut h, "the prompt", |t| t.tabs[0].phase() == Phase::Running && text(t).contains("PS "));
    h.event(egui::Event::Text("cls; echo \"w=$($Host.UI.RawUI.WindowSize.Width)\"".into()));
    h.key_press(Key::Enter);
    let start = Instant::now();
    while !text(h.state()).lines().any(|l| l.starts_with("w=")) {
        assert!(start.elapsed() < Duration::from_secs(30), "no width line:\n{}", text(h.state()));
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    h.step();
    let screen = text(h.state());
    let line = screen.lines().find(|l| l.starts_with("w=")).unwrap_or_default().to_owned();
    h.state_mut().close_all();
    assert_eq!(line, format!("w={cols}"), "phase at resize: {phase:?}; grid has {cols} columns, PTY reports {line}");
}
