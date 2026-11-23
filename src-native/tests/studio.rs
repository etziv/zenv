use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui::{self, Color32, Key, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::project_files::NewKind;
use zenv_native::studio::editor::{find_all, indent_of, Doc};
use zenv_native::studio::highlight::{syntax_for, Highlight, COLOUR_MAX};
use zenv_native::studio::icons::{file_icon, folder_icon, icon_name};
use zenv_native::studio::tree::Tree;
use zenv_native::studio::{Site, Studio, Windows};
use zenv_native::theme;

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("zenv-native-studio-test-{}", std::process::id()));
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

fn project() -> Project {
    static N: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!("zenv-studio-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(root.join("app/Http")).unwrap();
    std::fs::write(root.join("app/Http/Kernel.php"), "<?php\r\nclass Kernel {}\r\n").unwrap();
    std::fs::write(root.join("index.php"), "<?php\necho 'hi';\n").unwrap();
    std::fs::write(root.join(".env"), "APP_KEY=x\n").unwrap();
    Project(root)
}

fn icon(i: u16) -> &'static str {
    icon_name(i)
}

#[test]
fn file_icons_follow_the_react_map() {
    assert_eq!(icon(file_icon("composer.json")), "php");
    assert_eq!(icon(file_icon("Dockerfile.dev")), "docker");
    assert_eq!(icon(file_icon(".env.staging")), "tune");
    assert_eq!(icon(file_icon("welcome.blade.php")), "laravel");
    assert_eq!(icon(file_icon("index.php")), "php");
    assert_eq!(icon(file_icon("types.d.ts")), "typescript-def");
    assert_eq!(icon(file_icon("App.TSX")), "react_ts");
    assert_eq!(icon(file_icon("main.go")), "go");
    assert_eq!(icon(file_icon("Cargo.toml")), "rust");
    assert_eq!(icon(file_icon("notes")), "file");
    assert_eq!(icon(file_icon("archive.unknownext")), "file");
    assert_eq!(icon(folder_icon("src", false)), "folder-src");
    assert_eq!(icon(folder_icon("node_modules", true)), "folder-node-open");
    assert_eq!(icon(folder_icon("Routes", false)), "folder-routes");
    assert_eq!(icon(folder_icon("whatever", true)), "folder-open");
}

#[test]
fn syntaxes_cover_the_web_stack() {
    let name = |f: &str| syntax_for(f).map(|s| s.name.as_str());
    assert_eq!(name("index.php"), Some("PHP"));
    assert_eq!(name("welcome.blade.php"), Some("PHP"));
    assert_eq!(name("artisan"), Some("PHP"));
    assert_eq!(name("App.tsx"), Some("JavaScript"));
    assert_eq!(name("main.rs"), Some("Rust"));
    assert_eq!(name("app.py"), Some("Python"));
    assert_eq!(name("style.css"), Some("CSS"));
    assert_eq!(name("composer.json"), Some("JSON"));
    assert_eq!(name("notes"), None);
}

fn colour_at(job: &egui::text::LayoutJob, byte: usize) -> Color32 {
    job.sections.iter().find(|s| s.byte_range.start.0 <= byte && byte < s.byte_range.end.0).map(|s| s.format.color).unwrap()
}

#[test]
fn php_tokens_get_dark_plus_colours() {
    let text = "<?php\n$name = \"zenv\"; // greet\nfunction hi() { return 42; }\n";
    let job = Highlight::new("index.php", text).job(text);
    let at = |needle: &str| colour_at(&job, text.find(needle).unwrap() + 1);
    assert_eq!(at("$name"), Color32::from_rgb(0x9c, 0xdc, 0xfe));
    assert_eq!(at("\"zenv\""), Color32::from_rgb(0xce, 0x91, 0x78));
    assert_eq!(at("// greet"), Color32::from_rgb(0x6a, 0x99, 0x55));
    assert_eq!(at("function"), Color32::from_rgb(0x56, 0x9c, 0xd6));
    assert_eq!(at("42"), Color32::from_rgb(0xb5, 0xce, 0xa8));
    assert_eq!(at("return"), Color32::from_rgb(0xc5, 0x86, 0xc0));
}

#[test]
fn an_edit_recolours_only_the_lines_near_it() {
    let lines: Vec<String> = (0..2000).map(|i| format!("$v{i} = {i}; // line {i}")).collect();
    let mut text = format!("<?php\n{}\n", lines.join("\n"));
    let mut h = Highlight::new("a.php", "");
    h.job(&text);
    assert!(h.recoloured > 1990);

    text = text.replacen("$v1000 = 1000;", "$v1000 = 'x';", 1);
    h.job(&text);
    assert!(h.recoloured <= 3, "changed one line, recoloured {}", h.recoloured);

    text = text.replacen("$v500 = 500;", "$v500 = 500;\n$extra = 1;", 1);
    let job = h.job(&text);
    assert!(h.recoloured <= 4, "inserted one line, recoloured {}", h.recoloured);
    let at = text.find("// line 1999").unwrap() + 1;
    assert_eq!(colour_at(&job, at), Color32::from_rgb(0x6a, 0x99, 0x55));

    text = text.replacen("// line 700", "/* open comment", 1);
    let job = h.job(&text);
    let at = text.find("$v1500").unwrap() + 1;
    assert_eq!(colour_at(&job, at), Color32::from_rgb(0x6a, 0x99, 0x55), "an unclosed comment colours the rest");
}

#[test]
fn indent_follows_the_file() {
    assert_eq!(indent_of("a\n\tb\n"), "\t");
    assert_eq!(indent_of("a\n  b\n"), "    ");
}

#[test]
fn saving_keeps_crlf_and_clears_unsaved() {
    let p = project();
    let mut d = Doc::open(&p.0, "app/Http/Kernel.php");
    assert_eq!(d.text, "<?php\nclass Kernel {}\n");
    assert!(!d.dirty());
    d.text.push_str("// more\n");
    assert!(d.dirty());
    d.write(&p.0, false);
    assert!(!d.dirty());
    assert_eq!(std::fs::read_to_string(p.0.join("app/Http/Kernel.php")).unwrap(), "<?php\r\nclass Kernel {}\r\n// more\r\n");
}

fn touch_later(path: &Path, content: &str) {
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(path, content).unwrap();
}

#[test]
fn an_outside_change_conflicts_with_unsaved_edits() {
    let p = project();
    let mut d = Doc::open(&p.0, "index.php");
    d.text.push_str("// mine\n");
    touch_later(&p.0.join("index.php"), "<?php\n// theirs\n");
    d.poll_disk(&p.0);
    assert!(d.conflict);
    d.write(&p.0, false);
    assert!(d.conflict, "a plain save must not overwrite their change");
    d.write(&p.0, true);
    assert!(!d.conflict && !d.dirty());
    assert!(std::fs::read_to_string(p.0.join("index.php")).unwrap().contains("// mine"));

    touch_later(&p.0.join("index.php"), "<?php\n// theirs again\n");
    d.text.push_str("x");
    d.poll_disk(&p.0);
    d.reload(&p.0);
    assert_eq!(d.text, "<?php\n// theirs again\n");
    assert!(!d.conflict && !d.dirty());
}

#[test]
fn a_clean_file_reloads_an_outside_change() {
    let p = project();
    let mut d = Doc::open(&p.0, "index.php");
    touch_later(&p.0.join("index.php"), "<?php\n// claude\n");
    d.poll_disk(&p.0);
    assert_eq!(d.text, "<?php\n// claude\n");
    assert!(!d.conflict);
}

#[test]
fn env_files_open_read_only_and_a_deleted_file_reports_on_save() {
    let p = project();
    let mut env = Doc::open(&p.0, ".env");
    assert!(env.read_only);
    env.text.push('x');
    env.write(&p.0, false);
    assert_eq!(std::fs::read_to_string(p.0.join(".env")).unwrap(), "APP_KEY=x\n");

    let mut d = Doc::open(&p.0, "index.php");
    std::fs::remove_file(p.0.join("index.php")).unwrap();
    d.text.push('x');
    d.write(&p.0, false);
    assert!(d.save_error.is_some());
    let missing = Doc::open(&p.0, "nope.php");
    assert!(missing.error.is_some());
}

#[test]
fn the_tree_lists_expands_and_creates() {
    let p = project();
    let mut t = Tree::new(&p.0);
    assert!(t.error.is_none());
    t.toggle(&p.0, "app");
    assert!(t.is_expanded("app"));
    assert_eq!(t.selected_dir, "app");
    t.start_draft(&p.0, NewKind::File);
    assert_eq!(t.finish_draft(&p.0, "routes.php"), Some("app/routes.php".to_owned()));
    assert!(p.0.join("app/routes.php").is_file());
    t.start_draft(&p.0, NewKind::Folder);
    assert_eq!(t.finish_draft(&p.0, "Models"), None);
    assert!(p.0.join("app/Models").is_dir());
    t.start_draft(&p.0, NewKind::File);
    assert_eq!(t.finish_draft(&p.0, "../escape.php"), None);
    assert!(t.error.is_some());
    t.start_draft(&p.0, NewKind::File);
    assert_eq!(t.finish_draft(&p.0, "  "), None);
}

fn studio(root: &Path) -> Harness<'static, Studio> {
    let site = Site { id: "s1".into(), domain: "shop.test".into(), root: root.to_owned(), url: "http://shop.test".into(), enabled: true };
    let core = core();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_theme(egui::Theme::Dark)
        .with_context_setup(theme::install)
        .build_ui_state(|ui, s: &mut Studio| s.ui(ui), Studio::new(&egui::Context::default(), core, site));
    h.run_steps(3);
    h
}

#[test]
fn open_edit_and_save_from_the_window() {
    let p = project();
    let mut h = studio(&p.0);
    h.get_by_label("shop.test");
    h.get_by_label("Pick a file on the left to edit it.");
    h.get_by_role_and_label(egui::Role::Button, "index.php").click();
    h.run_steps(3);
    assert_eq!(h.state().editor.active.as_deref(), Some("index.php"));
    h.get_by_role_and_label(egui::Role::Tab, "index.php");
    h.get_by_label("saved");
    h.get_by_role(egui::Role::MultilineTextInput).type_text("// edit\n");
    h.run_steps(2);
    h.get_by_label("unsaved");
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(2);
    h.get_by_label("saved");
    assert!(std::fs::read_to_string(p.0.join("index.php")).unwrap().contains("// edit"));
}

#[test]
fn closing_an_unsaved_tab_asks_first() {
    let p = project();
    let mut h = studio(&p.0);
    h.state_mut().open_file("index.php");
    h.run_steps(2);
    h.state_mut().editor.docs[0].text.push('x');
    h.run_steps(2);
    h.get_by_label("Close index.php").click();
    h.run_steps(2);
    assert_eq!(h.state().editor.docs.len(), 1);
    h.get_by_label("Close anyway").click();
    h.run_steps(2);
    assert!(h.state().editor.docs.is_empty());
}

#[test]
fn go_to_file_finds_and_opens() {
    let p = project();
    let mut h = studio(&p.0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run_steps(2);
    assert!(h.state().quick.is_some());
    h.get_by_role(egui::Role::TextInput).type_text("kern");
    for _ in 0..100 {
        h.run_steps(1);
        if h.state().quick.as_ref().is_some_and(|q| !q.hits.is_empty()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run_steps(1);
    h.get_by_role_and_label(egui::Role::Button, "app/Http/Kernel.php");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().quick.is_none());
    assert_eq!(h.state().editor.active.as_deref(), Some("app/Http/Kernel.php"));
}

#[test]
fn ctrl_b_hides_the_files() {
    let p = project();
    let mut h = studio(&p.0);
    h.get_by_label("New file");
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    assert!(!h.state().show_tree);
    assert!(h.query_by_label("New file").is_none());
}

#[test]
fn new_file_from_the_tree_opens_it() {
    let p = project();
    let mut h = studio(&p.0);
    h.get_by_label("New file").click();
    h.run_steps(2);
    h.get_by_role(egui::Role::TextInput).type_text("hello.php");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(p.0.join("hello.php").is_file());
    assert_eq!(h.state().editor.active.as_deref(), Some("hello.php"));
}

#[test]
fn big_files_go_plain_then_read_only() {
    let p = project();
    let line = "$a = 1; // x\n";
    std::fs::write(p.0.join("mid.php"), line.repeat(COLOUR_MAX / line.len() + 10)).unwrap();
    std::fs::write(p.0.join("huge.php"), line.repeat(1_200_000 / line.len())).unwrap();
    let mid = Doc::open(&p.0, "mid.php");
    assert!(!mid.read_only && !mid.large);
    assert_eq!(mid.language(), Some("PHP"));
    let huge = Doc::open(&p.0, "huge.php");
    assert!(huge.read_only && huge.large);
    let plain = Highlight::new("mid.php", &"x\n".repeat(COLOUR_MAX));
    assert!(!plain.coloured());
    assert_eq!(plain.language(), Some("PHP"));
}

#[test]
fn find_folds_unicode_case_and_returns_char_boundaries() {
    let text = "Grüße ÉCOLE école İstanbul";
    let hits = find_all(text, "école");
    assert_eq!(hits.len(), 2);
    for (a, b) in &hits {
        assert!(text.is_char_boundary(*a) && text.is_char_boundary(*b));
        assert_eq!(text[*a..*b].to_lowercase(), "école");
    }
    assert_eq!(find_all(text, "GRÜSSE").len(), 0);
    assert_eq!(find_all(text, "grüße").len(), 1);
    assert!(find_all(text, "").is_empty());
}

#[test]
fn minified_lines_stay_plain_and_very_long_lines_open_read_only() {
    let p = project();
    std::fs::write(p.0.join("app.min.js"), format!("var a={};{}", "{}", "x=1;".repeat(5000))).unwrap();
    std::fs::write(p.0.join("huge.min.js"), "y=2;".repeat(12_000)).unwrap();
    let min = Doc::open(&p.0, "app.min.js");
    assert!(!min.read_only);
    assert!(!Highlight::new("app.min.js", &min.text).coloured());
    let huge = Doc::open(&p.0, "huge.min.js");
    assert!(huge.read_only && huge.large);
}

#[test]
fn a_file_gone_from_disk_is_flagged() {
    let p = project();
    let mut d = Doc::open(&p.0, "index.php");
    std::fs::remove_file(p.0.join("index.php")).unwrap();
    d.poll_disk(&p.0);
    assert!(d.gone);
    std::fs::write(p.0.join("index.php"), "<?php\n").unwrap();
    d.poll_disk(&p.0);
    assert!(!d.gone);
}

#[test]
fn quitting_with_unsaved_studio_edits_asks_first() {
    let p = project();
    let ctx = egui::Context::default();
    let mut windows = Windows::default();
    let site = Site { id: "s1".into(), domain: "shop.test".into(), root: p.0.clone(), url: String::new(), enabled: true };
    let studio = windows.open(&ctx, &core(), site);
    assert!(!windows.ask_quit(&ctx), "nothing unsaved: quit at once");
    {
        let mut s = studio.lock().unwrap();
        s.open_file("index.php");
        s.editor.docs[0].text.push('x');
    }
    assert!(windows.ask_quit(&ctx));
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_context_setup(theme::install).build_ui_state(|ui, s: &mut std::sync::Arc<std::sync::Mutex<Studio>>| s.lock().unwrap().ui(ui), studio.clone());
    h.run_steps(2);
    h.get_by_label("Quit Zenv with unsaved changes?");
    h.get_by_label("Quit anyway").click();
    h.run_steps(2);
    assert!(windows.take_quit());
    assert!(!windows.ask_quit(&ctx), "after the answer quit goes through");
}
#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test studio -- --ignored --nocapture"]
fn typing_frame_time_by_file_size() {
    let line = "    $total = $order->items()->sum('price') * 1.11; // tax\n";
    for kb in [100, 1000, 1500] {
        let p = project();
        let body: String = std::iter::repeat_n(line, kb * 1000 / line.len()).collect();
        std::fs::write(p.0.join("a.php"), format!("<?php\n{body}")).unwrap();
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let site = Site { id: "s".into(), domain: "d.test".into(), root: p.0.clone(), url: String::new(), enabled: true };
        let mut s = Studio::new(&ctx, core(), site);
        let input = |events: Vec<egui::Event>| egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))), events, ..Default::default() };
        let t = std::time::Instant::now();
        s.open_file("a.php");
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
        println!("{kb} KB: open {open:?}, typing median {:.1} ms, max {:.1} ms, edited {}, read-only {}", frames[10], frames[19], doc.dirty(), doc.read_only);
    }
}