use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use alacritty_terminal::index::{Column, Line};
use eframe::egui::{self, Color32, Key, Modifiers, ViewportId};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_core::studio::Agent;
use zenv_native::studio::terminal::{colors, grid_size, keys, Outbox, Screen, Terminal};
use zenv_native::studio::terminals::{Host, Phase, Terminals};
use zenv_native::studio::Colors;
use zenv_native::theme;

const NONE: Modifiers = Modifiers::NONE;

fn enc(key: Key, m: Modifiers, mode: TermMode) -> String {
    String::from_utf8(keys::encode(key, m, mode).expect("key sends bytes")).unwrap()
}

#[test]
fn keys_send_what_xterm_sends() {
    let n = TermMode::empty();
    assert_eq!(enc(Key::Enter, NONE, n), "\r");
    assert_eq!(enc(Key::Enter, Modifiers::SHIFT, n), "\x1b\r", "Shift+Enter is a new line in the agent's prompt");
    assert_eq!(enc(Key::ArrowUp, NONE, n), "\x1b[A");
    assert_eq!(enc(Key::ArrowUp, NONE, TermMode::APP_CURSOR), "\x1bOA");
    assert_eq!(enc(Key::ArrowLeft, Modifiers::CTRL, TermMode::APP_CURSOR), "\x1b[1;5D");
    assert_eq!(enc(Key::ArrowRight, Modifiers::SHIFT | Modifiers::ALT, n), "\x1b[1;4C");
    assert_eq!(enc(Key::Home, NONE, n), "\x1b[H");
    assert_eq!(enc(Key::End, NONE, TermMode::APP_CURSOR), "\x1bOF");
    assert_eq!(enc(Key::Delete, NONE, n), "\x1b[3~");
    assert_eq!(enc(Key::PageDown, Modifiers::CTRL, n), "\x1b[6;5~");
    assert_eq!(enc(Key::Tab, NONE, n), "\t");
    assert_eq!(enc(Key::Tab, Modifiers::SHIFT, n), "\x1b[Z");
    assert_eq!(enc(Key::Backspace, NONE, n), "\x7f");
    assert_eq!(enc(Key::Backspace, Modifiers::CTRL, n), "\x08");
    assert_eq!(enc(Key::Escape, NONE, n), "\x1b");
    assert_eq!(enc(Key::F1, NONE, n), "\x1bOP");
    assert_eq!(enc(Key::F4, Modifiers::SHIFT, n), "\x1b[1;2S");
    assert_eq!(enc(Key::F5, NONE, n), "\x1b[15~");
    assert_eq!(enc(Key::F12, NONE, n), "\x1b[24~");
    assert_eq!(enc(Key::A, Modifiers::CTRL, n), "\x01");
    assert_eq!(enc(Key::Z, Modifiers::CTRL, n), "\x1a");
    assert_eq!(enc(Key::Space, Modifiers::CTRL, n), "\0");
    assert_eq!(enc(Key::OpenBracket, Modifiers::CTRL, n), "\x1b");
    assert_eq!(enc(Key::D, Modifiers::CTRL | Modifiers::ALT, n), "\x1b\x04");
    assert_eq!(keys::encode(Key::A, NONE, n), None, "letters arrive as text");
    assert_eq!(keys::encode(Key::A, Modifiers::SHIFT, n), None);
}

#[test]
fn paste_uses_carriage_returns_and_brackets_when_asked() {
    assert_eq!(keys::paste("a\r\nb\nc", false), b"a\rb\rc");
    assert_eq!(keys::paste("ls\n", true), b"\x1b[200~ls\r\x1b[201~");
    assert_eq!(keys::paste("x\x1b[201~rm\x03", true), b"\x1b[200~x[201~rm\x1b[201~", "a paste cannot end the bracket early");
}

#[test]
fn mouse_reports_in_sgr_and_x10() {
    let sgr = TermMode::SGR_MOUSE;
    assert_eq!(keys::mouse(keys::button::LEFT, 4, 2, true, NONE, sgr), b"\x1b[<0;5;3M");
    assert_eq!(keys::mouse(keys::button::LEFT, 4, 2, false, NONE, sgr), b"\x1b[<0;5;3m");
    assert_eq!(keys::mouse(keys::button::WHEEL_DOWN, 0, 0, true, Modifiers::CTRL, sgr), b"\x1b[<81;1;1M");
    assert_eq!(keys::mouse(keys::button::LEFT, 0, 0, true, NONE, TermMode::empty()), [0x1b, b'[', b'M', 32, 33, 33]);
    assert_eq!(keys::mouse(keys::button::LEFT, 0, 0, false, NONE, TermMode::empty()), [0x1b, b'[', b'M', 35, 33, 33]);
}

#[test]
fn colours_follow_the_react_theme_and_xterm_tables() {
    let set = alacritty_terminal::term::color::Colors::default();
    assert_eq!(colors::resolve(Color::Named(NamedColor::Background), &set, false, false), colors::BACKGROUND);
    assert_eq!(colors::resolve(Color::Named(NamedColor::Foreground), &set, true, false), colors::FOREGROUND);
    assert_eq!(colors::resolve(Color::Named(NamedColor::Green), &set, false, false), Color32::from_rgb(0x3e, 0xe8, 0xa0));
    assert_eq!(colors::resolve(Color::Named(NamedColor::Red), &set, true, false), Color32::from_rgb(0xff, 0x8a, 0x7e), "bold is bright");
    assert_eq!(colors::resolve(Color::Indexed(1), &set, true, false), Color32::from_rgb(0xff, 0x8a, 0x7e));
    assert_eq!(colors::indexed(16), Color32::from_rgb(0, 0, 0));
    assert_eq!(colors::indexed(196), Color32::from_rgb(0xff, 0, 0));
    assert_eq!(colors::indexed(231), Color32::from_rgb(0xff, 0xff, 0xff));
    assert_eq!(colors::indexed(232), Color32::from_rgb(8, 8, 8));
    assert_eq!(colors::indexed(255), Color32::from_rgb(238, 238, 238));
    assert_eq!(colors::resolve(Color::Spec(Rgb { r: 10, g: 200, b: 30 }), &set, true, false), Color32::from_rgb(10, 200, 30));
    assert_eq!(colors::resolve(Color::Spec(Rgb { r: 100, g: 200, b: 30 }), &set, false, true), Color32::from_rgb(66, 132, 19));
    assert_eq!(colors::resolve(Color::Named(NamedColor::DimRed), &set, false, false), Color32::from_rgb(168, 59, 48));
}

fn screen(cols: u16, rows: u16) -> (Screen, Arc<Outbox>) {
    let out = Arc::new(Outbox::default());
    (Screen::new(cols, rows, out.clone()), out)
}

fn sent(out: &Outbox) -> Vec<u8> {
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    out.connect(Box::new(move |b| g.lock().unwrap().extend_from_slice(b)));
    let v = got.lock().unwrap().clone();
    v
}

#[test]
fn screen_shows_output_colours_and_wide_characters() {
    let (mut s, _) = screen(40, 6);
    s.feed(b"hello\r\n\x1b[38;2;255;100;0mtrue\x1b[0m \x1b[1;31mred\x1b[0m\r\n\xe4\xbd\xa0\xe5\xa5\xbd!");
    assert_eq!(s.text(), "hello\ntrue red\n你好!");
    let grid = s.term.grid();
    assert_eq!(grid[Line(1)][Column(0)].fg, Color::Spec(Rgb { r: 255, g: 100, b: 0 }), "truecolor kept");
    assert!(grid[Line(1)][Column(5)].flags.contains(Flags::BOLD));
    assert!(grid[Line(2)][Column(0)].flags.contains(Flags::WIDE_CHAR));
}

#[test]
fn resize_keeps_the_text_and_reset_clears_it() {
    let (mut s, _) = screen(40, 6);
    s.feed(b"one\r\ntwo");
    s.resize(60, 10);
    assert_eq!(s.size(), (60, 10));
    assert_eq!(s.text(), "one\ntwo");
    s.reset();
    assert_eq!(s.text(), "");
}

#[test]
fn queries_are_answered_and_held_until_a_session_connects() {
    let (mut s, out) = screen(40, 6);
    s.feed(b"ab\x1b[6n");
    out.send(b"typed");
    assert_eq!(sent(&out), b"\x1b[1;3Rtyped", "ConPTY's cursor query is answered first, then what was typed early");
    out.disconnect();
    out.send(b"later");
    assert_eq!(sent(&out), b"later", "a dropped session does not keep the old sink");
}

#[test]
fn programs_cannot_write_the_clipboard() {
    let mut h = widget();
    h.state().term.lock().feed(b"]52;c;cGFzdGVk");
    h.step();
    let copied = h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(_)));
    assert!(!copied, "OSC 52 is ignored, as in the React terminal");
}

#[test]
fn grid_size_fits_the_rect_and_stays_in_bounds() {
    let cell = egui::vec2(7.8, 15.0);
    assert_eq!(grid_size(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)), cell), (100, 39));
    assert_eq!(grid_size(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(50.0, 20.0)), cell), (20, 5));
}

struct Widget {
    term: Terminal,
    input: Vec<u8>,
}

fn widget() -> Harness<'static, Widget> {
    let w = Widget { term: Terminal::new(egui::Id::unique("t")), input: Vec::new() };
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 320.0)).with_context_setup(theme::install).build_ui_state(
        |ui, w: &mut Widget| {
            let rect = ui.max_rect();
            let f = w.term.ui(ui, rect);
            w.input.extend(f.input);
        },
        w,
    );
    h.run_steps(2);
    h
}

#[test]
fn typing_copy_and_paste_become_terminal_input() {
    let mut h = widget();
    h.event(egui::Event::Text("ls".into()));
    h.key_press(Key::Enter);
    h.key_press_modifiers(Modifiers::CTRL, Key::L);
    h.event(egui::Event::Copy);
    h.event(egui::Event::Paste("a\nb".into()));
    h.run_steps(2);
    assert_eq!(h.state().input, b"ls\r\x0c\x03a\rb", "Ctrl+C with nothing selected interrupts");
    h.state().term.lock().feed(b"\x1b[?2004h");
    h.state_mut().input.clear();
    h.event(egui::Event::Paste("x".into()));
    h.run_steps(2);
    assert_eq!(h.state().input, b"\x1b[200~x\x1b[201~");
}

#[test]
fn an_alt_chord_without_text_does_not_turn_the_next_letter_into_alt() {
    let mut h = widget();
    h.key_press_modifiers(Modifiers::ALT, Key::Space);
    h.key_press(Key::Escape);
    h.event(egui::Event::Text("a".into()));
    h.run_steps(2);
    assert_eq!(h.state().input, b"\x1ba");
}

#[test]
fn clicks_on_a_popup_over_the_terminal_are_not_reported_to_the_program() {
    let w = Widget { term: Terminal::new(egui::Id::unique("covered")), input: Vec::new() };
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 320.0)).with_context_setup(theme::install).build_ui_state(
        |ui, w: &mut Widget| {
            let rect = ui.max_rect();
            let f = w.term.ui(ui, rect);
            w.input.extend(f.input);
            egui::Area::new(egui::Id::unique("menu")).order(egui::Order::Foreground).fixed_pos(egui::pos2(100.0, 100.0)).show(ui.ctx(), |ui| {
                ui.add_sized(egui::vec2(200.0, 100.0), egui::Button::new("PowerShell"));
            });
        },
        w,
    );
    h.run_steps(2);
    h.state().term.lock().feed(b"\x1b[?1000h\x1b[?1006h");
    h.run_steps(2);
    h.get_by_label("PowerShell").click();
    h.run_steps(2);
    assert!(h.state().input.is_empty(), "sent {:?}", String::from_utf8_lossy(&h.state().input));
    h.hover_at(egui::pos2(500.0, 250.0));
    h.event(egui::Event::PointerButton { pos: egui::pos2(500.0, 250.0), button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().input.starts_with(b"\x1b[<0;"), "an uncovered press is still reported");
}

#[test]
fn the_widget_resizes_the_screen_to_its_rect() {
    let mut h = widget();
    let first = h.state().term.lock().size();
    h.set_size(egui::vec2(900.0, 500.0));
    h.run_steps(2);
    let second = h.state().term.lock().size();
    assert!(second.0 > first.0 && second.1 > first.1, "{first:?} -> {second:?}");
}

// ---------- real pseudo console

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("zenv-native-terminal-test-{}", std::process::id()));
        Arc::new(Core::initialize(AppPaths::new(dir)).expect("test core initializes"))
    })
    .clone()
}

fn site(core: &Core) -> String {
    static SITE: OnceLock<String> = OnceLock::new();
    SITE.get_or_init(|| create_site(core)).clone()
}

fn create_site(core: &Core) -> String {
    let root = std::env::temp_dir().join(format!("zenv-terminal-site-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = SiteInput {
        name: "term".into(),
        domain: format!("term{}.test", std::process::id()),
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

fn wait(h: &mut Harness<'static, Terminals>, what: &str, done: impl Fn(&Terminals) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}; screen:\n{}", text(h.state()));
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn text(t: &Terminals) -> String {
    t.tabs.iter().find(|x| x.key == t.active).map(|x| x.term.lock().text()).unwrap_or_default()
}

fn typed(h: &mut Harness<'static, Terminals>, line: &str) {
    h.event(egui::Event::Text(line.into()));
    h.key_press(Key::Enter);
    h.step();
}

#[test]
fn powershell_runs_resizes_exits_and_restarts() {
    let core = core();
    let site_id = site(&core);
    let host = Host { core: core.clone(), owner: "terminal-test".into(), site_id, viewport: ViewportId::ROOT };
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
    wait(&mut h, "the prompt", |t| text(t).contains("PS "));
    assert_eq!(h.state().tabs[0].phase(), Phase::Running);
    typed(&mut h, "echo (\"zenv-\" + 6*7)");
    wait(&mut h, "the echo", |t| text(t).contains("zenv-42"));
    let cols = h.state().tabs[0].term.lock().size().0;
    typed(&mut h, "echo \"w=$($Host.UI.RawUI.WindowSize.Width)\"");
    wait(&mut h, "the width", |t| text(t).contains(&format!("w={cols}")));
    h.set_size(egui::vec2(1200.0, 500.0));
    h.step();
    let wider = h.state().tabs[0].term.lock().size().0;
    assert!(wider > cols);
    typed(&mut h, "cls; echo \"w=$($Host.UI.RawUI.WindowSize.Width)\"");
    wait(&mut h, "the new width", |t| text(t).contains(&format!("w={wider}")));
    typed(&mut h, "exit");
    wait(&mut h, "the exit line", |t| text(t).contains("[PowerShell ended (exit code 0). Press Enter to start it again.]"));
    assert_eq!(h.state().tabs[0].phase(), Phase::Ended);
    h.key_press(Key::Enter);
    wait(&mut h, "the restart", |t| t.tabs[0].phase() == Phase::Running && text(t).contains("PS "));
    assert!(!text(h.state()).contains("ended"), "the screen starts clean");
    h.state_mut().close_all();
    assert!(h.state().tabs.is_empty());
}

#[test]
fn a_flood_of_terminal_queries_does_not_freeze_the_window() {
    let core = core();
    let host = Host { core: core.clone(), owner: "terminal-flood-test".into(), site_id: site(&core), viewport: ViewportId::ROOT };
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
    wait(&mut h, "the prompt", |t| text(t).contains("PS "));
    typed(&mut h, "[Console]::Write(\"$([char]27)[c$([char]27)[6n\" * 100000); echo flood-$(2*21)");
    let screen = h.state().tabs[0].term.screen.clone();
    let start = Instant::now();
    loop {
        if let Ok(s) = screen.try_lock() {
            if s.text().contains("flood-42") {
                break;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(60), "the screen stayed locked or the shell never finished");
        std::thread::sleep(Duration::from_millis(100));
    }
    h.step();
    h.state_mut().close_all();
}

#[test]
fn claude_code_symbols_have_glyphs() {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    let font = egui::FontId::monospace(13.0);
    for c in "⏵⏸⏺◐✻✽✳⎿⧉❯─╭▛█✓".chars() {
        assert!(ctx.fonts_mut(|f| f.has_glyph(&font, c)), "no glyph for {c} (U+{:04X})", c as u32);
    }
}

#[test]
fn plus_menu_is_compact_and_adds_a_tab() {
    let core = core();
    let host = Host { core: core.clone(), owner: "terminal-menu-test".into(), site_id: site(&core), viewport: ViewportId::ROOT };
    let t = Terminals::new(&egui::Context::default(), host);
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 500.0)).with_theme(egui::Theme::Dark).with_context_setup(theme::install).build_ui_state(
        |ui, t: &mut Terminals| {
            let rect = ui.max_rect();
            let c = Colors::of(ui);
            t.ui(ui, rect, &c);
        },
        t,
    );
    wait(&mut h, "the agents", |t| !t.agents.is_empty() && !t.tabs.is_empty());
    let first = h.state().tabs.len();
    h.get_by_label("New terminal").click();
    wait(&mut h, "the menu", |_| true);
    h.run_steps(2);
    let item = h.get_by_label("PowerShell");
    let width = item.rect().width();
    assert!(width <= 220.0, "menu items stay compact, got {width}");
    item.click();
    h.run_steps(2);
    let names: Vec<String> = h.state().tabs.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names.len(), first + 1, "{names:?}");
    assert!(names.last().unwrap().starts_with("PowerShell"), "{names:?}");
    h.state_mut().close_all();
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test terminal -- --ignored --nocapture"]
fn full_screen_frame_time() {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 940.0));
    let input = || egui::RawInput { screen_rect: Some(rect), ..Default::default() };
    for (name, colour_every) in [("plain", 0usize), ("coloured runs of 5", 5), ("every cell coloured", 1)] {
        let mut term = Terminal::new(egui::Id::unique(name));
        let frame = |term: &mut Terminal| {
            let mut out = ctx.run_ui(input(), |ui| {
                term.ui(ui, ui.max_rect());
            });
            out.textures_delta.clear();
            ctx.tessellate(out.shapes, 1.0);
        };
        frame(&mut term);
        let (cols, rows) = term.lock().size();
        let mut text = String::new();
        for r in 0..rows {
            for c in 0..cols as usize {
                if colour_every > 0 && c % colour_every == 0 {
                    text.push_str(&format!("\x1b[38;5;{}m", 16 + (c + r as usize) % 216));
                }
                text.push((b'a' + (c % 26) as u8) as char);
            }
            if r + 1 < rows {
                text.push_str("\r\n");
            }
        }
        term.lock().feed(text.as_bytes());
        let mut ms = Vec::new();
        for _ in 0..30 {
            let t = Instant::now();
            frame(&mut term);
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        ms.sort_by(f64::total_cmp);
        println!("{cols}x{rows} {name}: median {:.2} ms, max {:.2} ms", ms[15], ms[29]);
    }
}
