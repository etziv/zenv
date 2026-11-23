use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use zenv_core::app_core::Core;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, Strategy};
use zenv_core::paths::AppPaths;
use zenv_native::studio::chat::model::{Answer, Item};
use zenv_native::studio::chat::{options, Chat};
use zenv_native::studio::Colors;
use zenv_native::theme;

const AREA_SETTLE: usize = 6;

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| Arc::new(Core::initialize(AppPaths::new(std::env::temp_dir().join(format!("zenv-native-chat-test-{}", std::process::id())))).expect("test core"))).clone()
}

fn gone_site(core: &Core) -> (String, std::path::PathBuf) {
    static SITE: OnceLock<(String, std::path::PathBuf)> = OnceLock::new();
    SITE.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("zenv-chat-site-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let input = SiteInput {
            name: "chat".into(),
            domain: format!("chat{}.test", std::process::id()),
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
        let id = core.sites.create(&input).expect("site").site.id;
        std::fs::remove_dir_all(&root).unwrap();
        (id, root)
    })
    .clone()
}

fn catalog() -> Value {
    json!({
        "models": [
            { "value": "default", "displayName": "Default", "resolvedModel": "claude-opus-5-5", "supportedEffortLevels": ["low", "medium", "high"] },
            { "value": "haiku", "displayName": "Haiku", "description": "Fast" }
        ],
        "commands": [{ "name": "review", "description": "Review a pull request" }, { "name": "compact" }],
        "outputStyles": ["default", "Explanatory"],
        "version": "2.1.0"
    })
}

fn harness() -> Harness<'static, Option<Chat>> {
    Harness::builder().with_size(egui::vec2(820.0, 760.0)).with_theme(egui::Theme::Dark).with_context_setup(theme::install).build_ui_state(
        |ui, s: &mut Option<Chat>| {
            let chat = s.get_or_insert_with(|| {
                options::save_catalog(ui.ctx(), &catalog());
                let core = core();
                let (site, root) = gone_site(&core);
                Chat::new(ui.ctx(), egui::Id::unique("chat-test"), core, "chat-test".into(), site, root, Arc::new(|| {}))
            });
            let rect = ui.max_rect();
            let c = Colors::of(ui);
            chat.ui(ui, rect, &c, Some("src/main.rs"));
        },
        None,
    )
}

fn chat<'a>(h: &'a mut Harness<'static, Option<Chat>>) -> &'a mut Chat {
    h.state_mut().as_mut().expect("chat made on the first frame")
}

fn wait(h: &mut Harness<'static, Option<Chat>>, what: &str, done: impl Fn(&Chat) -> bool) {
    let start = Instant::now();
    while !done(h.state().as_ref().unwrap()) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        h.step();
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn lines(v: Value) -> Vec<Value> {
    v.as_array().unwrap().clone()
}

fn kinds(c: &Chat) -> Vec<&'static str> {
    c.state
        .items
        .iter()
        .map(|e| match &e.item {
            Item::User { .. } => "user",
            Item::Text { .. } => "text",
            Item::Tool(_) => "tool",
            Item::Result { .. } => "result",
            Item::Notice { .. } => "notice",
        })
        .collect()
}

fn notices(c: &Chat) -> Vec<String> {
    c.state.items.iter().filter_map(|e| if let Item::Notice { text, .. } = &e.item { Some(text.clone()) } else { None }).collect()
}

#[test]
fn empty_chat_explains_itself_and_uses_the_listed_models() {
    let mut h = harness();
    h.run_steps(2);
    h.get_by_label("Ask Claude Code to build, fix or explain something in this project.");
    let c = chat(&mut h);
    assert_eq!(c.models().iter().map(|m| m.value.as_str()).collect::<Vec<_>>(), ["default", "haiku"], "a fresh catalog is used, Claude Code is not asked");
    assert!(c.commands().iter().any(|x| x.name == "review"));
}

#[test]
fn a_conversation_draws_and_a_permission_answer_reaches_the_worker() {
    let mut h = harness();
    h.run_steps(1);
    let c = chat(&mut h);
    c.state.sent("make a file", vec!["notes.txt".into()]);
    c.state.apply(&lines(json!([
        { "type": "system", "subtype": "init", "session_id": "11111111-1111-1111-1111-111111111111", "model": "claude-opus-5-5" },
        { "type": "assistant", "message": { "content": [{ "type": "text", "text": "Sure. Running `ls` first:\n\n- one\n- two" }] } },
        { "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "ls -la" } }] } },
        { "type": "control_request", "request_id": "r1", "request": { "subtype": "can_use_tool", "tool_name": "Bash", "tool_use_id": "t1", "input": { "command": "ls -la" },
          "permission_suggestions": [{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "ls:*" }] }] } }
    ])));
    h.run_steps(2);
    h.get_by_label("Allow Claude to use Bash?");
    h.get_by_label("Always allow Bash ls:*");
    h.get_by_label("Opus 5.5");
    h.get_by_label("Allow").click();
    h.run_steps(1);
    let Item::Tool(t) = &chat(&mut h).state.items[2].item else { panic!("tool card") };
    assert_eq!(t.permission.as_ref().map(|p| p.state), Some(Answer::Allowed));
    wait(&mut h, "the worker's answer", |c| !notices(c).is_empty());
    assert_eq!(notices(chat(&mut h)), ["Claude Code has stopped; send a message to start it again."], "no program: said, not a panic");
}

#[test]
fn a_message_typed_while_busy_waits_its_turn_and_a_failed_start_is_shown() {
    let mut h = harness();
    h.run_steps(2);
    chat(&mut h).state.sent("first", vec![]);
    h.run_steps(1);
    h.event(egui::Event::Text("second".into()));
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(1);
    let c = chat(&mut h);
    assert_eq!(kinds(c), ["user", "user"]);
    assert!(c.composer.text.is_empty(), "the box clears on send");
    assert!(c.state.busy);
    wait(&mut h, "the start error", |c| !notices(c).is_empty());
    let c = chat(&mut h);
    assert!(notices(c)[0].contains("is not an existing folder"), "{:?}", notices(c));
    assert!(!c.state.busy, "an error ends the turn");
}

#[test]
fn enter_in_a_menu_picks_instead_of_sending() {
    let mut h = harness();
    h.run_steps(2);
    h.event(egui::Event::Text("/rev".into()));
    h.run_steps(AREA_SETTLE);
    h.get_by_label("review");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(chat(&mut h).composer.text, "/review ");
    assert!(chat(&mut h).state.items.is_empty(), "nothing was sent");
    chat(&mut h).composer.text = "see ".into();
    h.event(egui::Event::Text("@".into()));
    h.run_steps(AREA_SETTLE);
    h.get_by_label("src/main.rs");
    h.key_press(Key::Tab);
    h.run_steps(2);
    let c = chat(&mut h);
    assert!(c.composer.text.ends_with("@src/main.rs "), "{:?}", c.composer.text);
    assert!(c.state.items.is_empty());
}

#[test]
fn shift_enter_is_a_new_line_and_enter_sends() {
    let mut h = harness();
    h.run_steps(2);
    h.event(egui::Event::Text("line one".into()));
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::SHIFT, Key::Enter);
    h.run_steps(1);
    h.event(egui::Event::Text("line two".into()));
    h.run_steps(1);
    assert_eq!(chat(&mut h).composer.text, "line one
line two");
    assert!(chat(&mut h).state.items.is_empty(), "Shift+Enter does not send");
    h.key_press(Key::Enter);
    h.run_steps(1);
    assert!(matches!(&chat(&mut h).state.items[0].item, Item::User { text, .. } if text == "line one
line two"));
}

#[test]
fn the_palette_filters_steps_into_views_and_closes() {
    let mut h = harness();
    h.run_steps(2);
    h.get_by_label("Actions: model, MCP servers, account, settings…").click();
    h.run_steps(2);
    h.get_by_label("Export conversation");
    h.event(egui::Event::Text("output".into()));
    h.run_steps(AREA_SETTLE);
    assert!(h.query_by_label("Export conversation").is_none(), "filtered out");
    h.get_by_label("Output styles").click();
    h.run_steps(AREA_SETTLE);
    h.get_by_label("Explanatory").click();
    h.run_steps(2);
    let c = chat(&mut h);
    assert_eq!(c.options.output_style.as_deref(), Some("Explanatory"));
    assert!(c.composer.palette.is_none(), "choosing closes it");
    h.get_by_label("Actions: model, MCP servers, account, settings…").click();
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(1);
    assert!(chat(&mut h).composer.palette.is_none());
}

#[test]
fn composer_buttons_work_under_a_long_conversation() {
    let mut h = harness();
    h.run_steps(1);
    for i in 0..30 {
        chat(&mut h).state.sent(&format!("question {i}"), vec![]);
        chat(&mut h).state.apply(&lines(json!([{ "type": "assistant", "message": { "content": [{ "type": "text", "text": "an answer

with two paragraphs" }] } }])));
    }
    h.run_steps(3);
    h.get_by_label("Actions: model, MCP servers, account, settings…").click();
    h.run_steps(AREA_SETTLE);
    assert!(chat(&mut h).composer.palette.is_some(), "the palette opens with a conversation above the composer");
}

#[test]
fn new_chat_clears_and_export_needs_something() {
    let mut h = harness();
    h.run_steps(1);
    assert_eq!(chat(&mut h).export(), None);
    chat(&mut h).state.sent("hello", vec![]);
    chat(&mut h).state.apply(&lines(json!([{ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Hi!" }] } }])));
    assert_eq!(chat(&mut h).export().as_deref(), Some("**You:** hello\n\nHi!"));
    h.run_steps(1);
    h.get_by_label("New chat").click();
    h.run_steps(1);
    assert!(chat(&mut h).state.items.is_empty());
    h.get_by_label("Ask Claude Code to build, fix or explain something in this project.");
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test chat -- --ignored --nocapture"]
fn long_conversation_frame_time() {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 900.0));
    let input = || egui::RawInput { screen_rect: Some(rect), ..Default::default() };
    let core = core();
    let (site, root) = gone_site(&core);
    options::save_catalog(&ctx, &catalog());
    let mut chat = Chat::new(&ctx, egui::Id::unique("chat-measure"), core, "chat-measure".into(), site, root, Arc::new(|| {}));
    let para = "Some **bold** text with `code` and a [link](https://x.y).\n\n- item one\n- item two\n\n```rust\nfn main() { println!(\"hi\"); }\n```";
    for i in 0..500 {
        chat.state.sent(&format!("question {i}"), vec![]);
        chat.state.apply(&[json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": para }] } })]);
    }
    let frame = |chat: &mut Chat| {
        let t = Instant::now();
        let mut out = ctx.run_ui(input(), |ui| {
            let c = Colors::of(ui);
            chat.ui(ui, ui.max_rect(), &c, None);
        });
        out.textures_delta.clear();
        ctx.tessellate(out.shapes, 1.0);
        t.elapsed().as_secs_f64() * 1000.0
    };
    frame(&mut chat);
    let mut ms: Vec<f64> = (0..30).map(|_| frame(&mut chat)).collect();
    ms.sort_by(f64::total_cmp);
    println!("{} items (200 shown): median {:.2} ms, max {:.2} ms", chat.state.items.len(), ms[15], ms[29]);
    chat.state.apply(&[json!({ "type": "stream_event", "event": { "type": "content_block_start", "content_block": { "type": "text" } } })]);
    let mut stream = Vec::new();
    for i in 0..300 {
        let deltas: Vec<Value> = (0..17).map(|j| json!({ "type": "stream_event", "event": { "type": "content_block_delta", "delta": { "text": format!("word{i}-{j} ") } } })).collect();
        chat.state.apply(&deltas);
        stream.push(frame(&mut chat));
    }
    stream.sort_by(f64::total_cmp);
    println!("streaming 5,100 deltas over 300 frames: median {:.2} ms, max {:.2} ms", stream[150], stream[299]);
}

#[test]
fn a_focused_chat_keeps_the_editor_shortcuts_away() {
    use zenv_native::studio::terminals::{Host, Terminals};
    let core = core();
    let (site_id, _) = gone_site(&core);
    let host = Host { core, owner: "chat-focus-test".into(), site_id, viewport: egui::ViewportId::ROOT };
    let mut h = Harness::builder().with_size(egui::vec2(820.0, 700.0)).with_context_setup(theme::install).build_ui_state(
        |ui, t: &mut Terminals| {
            if t.tabs.is_empty() {
                t.add_chat(ui.ctx());
            }
            let rect = ui.max_rect();
            let c = Colors::of(ui);
            t.ui(ui, rect, &c);
        },
        Terminals::new(&egui::Context::default(), host),
    );
    h.run_steps(3);
    let ctx = h.ctx.clone();
    assert_eq!(h.state().tabs[0].name, "Claude");
    assert!(h.state().has_focus(&ctx), "typing in the chat must not reach the editor's Ctrl+S / Ctrl+F");
    h.state_mut().close_all();
    assert!(h.state().tabs.is_empty());
}

#[test]
fn the_studio_window_fits_a_1080_screen_and_opens_over_the_main_window() {
    use zenv_native::studio::placement;
    let main = egui::Rect::from_min_size(egui::pos2(300.0, 200.0), egui::vec2(1200.0, 700.0));
    let (size, pos) = placement(Some(egui::vec2(1920.0, 1080.0)), Some(main));
    assert_eq!(size, egui::vec2(1440.0, 864.0));
    let pos = pos.unwrap();
    assert_eq!(pos, egui::pos2(180.0, 118.0), "centred on the main window");
    assert!(pos.y + size.y + 40.0 < 1040.0, "bottom (with the title bar) stays above the taskbar");
    assert_eq!(placement(None, None), (egui::vec2(1440.0, 900.0), None));
    assert_eq!(placement(Some(egui::vec2(1280.0, 720.0)), None).0, egui::vec2(1152.0, 576.0));
}
