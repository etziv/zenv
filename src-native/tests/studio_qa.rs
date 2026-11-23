use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use eframe::egui::{self, Key, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_native::studio::{Site, Studio};
use zenv_native::theme;

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("zenv-native-studio-qa-{}", std::process::id()));
        Arc::new(Core::initialize(AppPaths::new(dir)).expect("test core initializes"))
    })
    .clone()
}

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn project(files: &[(&str, &str)]) -> Project {
    static N: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!("zenv-studio-qa-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body) in files {
        std::fs::write(root.join(name), body).unwrap();
    }
    Project(root)
}

fn studio(root: &Path) -> Harness<'static, Studio> {
    let site = Site { id: "qa".into(), domain: "qa.test".into(), root: root.to_owned(), url: "http://qa.test".into(), enabled: true };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .build_ui_state(|ui, s: &mut Studio| s.ui(ui), Studio::new(&egui::Context::default(), core(), site));
    h.run_steps(3);
    h
}

#[test]
fn typing_while_the_find_bar_is_open_keeps_the_window_alive() {
    let p = project(&[("index.php", "<?php\necho 'hi';\n")]);
    let mut h = studio(&p.0);
    h.state_mut().open_file("index.php");
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run_steps(2);
    h.get_by_role(egui::Role::TextInput).type_text("hi';");
    h.run_steps(2);
    h.get_by_role(egui::Role::MultilineTextInput).focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    assert_eq!(h.state().editor.docs[0].text, "x");
}

#[test]
fn the_active_tab_stays_in_view_when_many_files_are_open() {
    let names: Vec<String> = (0..30).map(|i| format!("controller_number_{i:02}.php")).collect();
    let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "<?php\n")).collect();
    let p = project(&files);
    let mut h = studio(&p.0);
    for n in &names {
        h.state_mut().open_file(n);
    }
    h.run_steps(4);
    let last = names.last().unwrap();
    assert_eq!(h.state().editor.active.as_deref(), Some(last.as_str()));
    let tab = h.get_by_role_and_label(egui::Role::Tab, last).rect();
    assert!(tab.right() <= 1280.0, "active tab {last} drawn at x {:.0}..{:.0}, outside the 1280 px window", tab.left(), tab.right());
}

#[test]
fn quitting_asks_about_unsaved_edits_in_every_studio() {
    use zenv_native::studio::Windows;
    let (pa, pb) = (project(&[("a.php", "<?php\n")]), project(&[("b.php", "<?php\n")]));
    let ctx = egui::Context::default();
    let mut windows = Windows::default();
    let mut studios = Vec::new();
    for (id, p, file) in [("sa", &pa, "a.php"), ("sb", &pb, "b.php")] {
        let site = Site { id: id.into(), domain: format!("{id}.test"), root: p.0.clone(), url: String::new(), enabled: true };
        let s = windows.open(&ctx, &core(), site);
        {
            let mut g = s.lock().unwrap();
            g.open_file(file);
            g.editor.docs[0].text.push('x');
        }
        studios.push(s);
    }
    assert!(windows.ask_quit(&ctx));
    let mut answered = false;
    for s in &studios {
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_context_setup(theme::install).build_ui_state(|ui, s: &mut Arc<std::sync::Mutex<Studio>>| s.lock().unwrap().ui(ui), s.clone());
        h.run_steps(2);
        if h.query_by_label("Quit anyway").is_some() && !answered {
            let names_both = h.query_by_label_contains("a.php (sa.test)").is_some() && h.query_by_label_contains("b.php (sb.test)").is_some();
            h.get_by_label("Quit anyway").click();
            h.run_steps(2);
            answered = true;
            if names_both {
                return;
            }
        }
    }
    assert!(answered, "no Studio showed the quit question");
    assert!(windows.take_quit());
    assert!(windows.ask_quit(&ctx), "the other Studio still has an unsaved file, but quit goes through without naming it");
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test studio_qa -- --ignored --nocapture"]
fn typing_frame_time_with_the_find_bar_open() {
    let line = "    $total = $order->items()->sum('price') * 1.11; // tax\n";
    for (kb, query) in [(100, "total"), (1000, "zzqq"), (1000, "total")] {
        let body: String = std::iter::repeat_n(line, kb * 1000 / line.len()).collect();
        let p = project(&[("a.php", &format!("<?php\n{body}"))]);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let site = Site { id: "s".into(), domain: "d.test".into(), root: p.0.clone(), url: String::new(), enabled: true };
        let mut s = Studio::new(&ctx, core(), site);
        let input = |events: Vec<egui::Event>| egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), events, ..Default::default() };
        s.open_file("a.php");
        let ctrl_f = egui::Event::Key { key: Key::F, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND };
        let at = egui::pos2(900.0, 500.0);
        let click = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        for ev in [vec![], vec![], vec![ctrl_f], vec![], vec![egui::Event::Text(query.into())], vec![], vec![egui::Event::PointerMoved(at), click(true)], vec![click(false)], vec![]] {
            let out = ctx.run_ui(input(ev), |ui| s.ui(ui));
            ctx.tessellate(out.shapes, 1.0);
        }
        let before = s.editor.docs[0].text.len();
        let mut frames = Vec::new();
        for _ in 0..20 {
            let t = std::time::Instant::now();
            let out = ctx.run_ui(input(vec![egui::Event::Text("x".into())]), |ui| s.ui(ui));
            ctx.tessellate(out.shapes, 1.0);
            frames.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        frames.sort_by(f64::total_cmp);
        let typed = s.editor.docs[0].text.len() - before;
        let t = std::time::Instant::now();
        let hits = zenv_native::studio::editor::find_all(&s.editor.docs[0].text, query).len();
        println!("{kb} KB find '{query}' ({hits} hits, find_all {:.1} ms): typing median {:.1} ms, max {:.1} ms, typed {typed}", t.elapsed().as_secs_f64() * 1000.0, frames[10], frames[19]);
    }
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test studio_qa -- --ignored --nocapture"]
fn typing_frame_time_on_a_minified_file() {
    let chunk = "function a(b,c){if(b>c){return\"x\"+b}var d=[1,2,3].map(function(e){return e*2});return d.join(\",\")}";
    for (kb, name) in [(20, "app.min.js"), (95, "app.min.js"), (20, "app.min.txt"), (95, "app.min.txt")] {
        let body: String = std::iter::repeat_n(chunk, kb * 1000 / chunk.len()).collect();
        let p = project(&[(name, &body)]);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let site = Site { id: "m".into(), domain: "m.test".into(), root: p.0.clone(), url: String::new(), enabled: true };
        let mut s = Studio::new(&ctx, core(), site);
        let input = |events: Vec<egui::Event>| egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), events, ..Default::default() };
        let t = std::time::Instant::now();
        s.open_file(name);
        for _ in 0..3 {
            let out = ctx.run_ui(input(vec![]), |ui| s.ui(ui));
            ctx.tessellate(out.shapes, 1.0);
        }
        let open = t.elapsed();
        let mut frames = Vec::new();
        for _ in 0..20 {
            let t = std::time::Instant::now();
            let out = ctx.run_ui(input(vec![egui::Event::Text("x".into())]), |ui| s.ui(ui));
            ctx.tessellate(out.shapes, 1.0);
            frames.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        frames.sort_by(f64::total_cmp);
        let doc = &s.editor.docs[0];
        println!("minified {name} {kb} KB one line: open {open:?}, typing median {:.1} ms, max {:.1} ms, edited {}", frames[10], frames[19], doc.dirty());
    }
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test studio_qa -- --ignored --nocapture"]
fn tree_refresh_time_with_a_big_expanded_folder() {
    let p = project(&[]);
    std::fs::create_dir_all(p.0.join("node_modules")).unwrap();
    for i in 0..1500 {
        std::fs::create_dir_all(p.0.join(format!("node_modules/pkg-{i}"))).unwrap();
    }
    let mut tree = zenv_native::studio::tree::Tree::new(&p.0);
    tree.toggle(&p.0, "node_modules");
    let mut runs = Vec::new();
    for _ in 0..5 {
        let t = std::time::Instant::now();
        tree.refresh(&p.0, &eframe::egui::Context::default());
        runs.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    runs.sort_by(f64::total_cmp);
    println!("tree refresh UI-thread cost with node_modules (1500 folders) expanded: median {:.1} ms, max {:.1} ms", runs[2], runs[4]);
}
