use std::sync::{Arc, OnceLock};

use eframe::egui::{self, Key};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::studio_chat::PermissionMode;
use zenv_native::studio::chat::{options, Chat};
use zenv_native::studio::Colors;
use zenv_native::theme;

const AREA_SETTLE: usize = 6;

fn core() -> Arc<Core> {
    static CORE: OnceLock<Arc<Core>> = OnceLock::new();
    CORE.get_or_init(|| Arc::new(Core::initialize(AppPaths::new(std::env::temp_dir().join(format!("zenv-native-chat-qa-{}", std::process::id())))).expect("test core"))).clone()
}

fn catalog() -> Value {
    json!({ "models": [{ "value": "default", "displayName": "Default" }, { "value": "haiku", "displayName": "Haiku" }], "commands": [{ "name": "review" }] })
}

fn harness() -> Harness<'static, Option<Chat>> {
    Harness::builder().with_size(egui::vec2(820.0, 760.0)).with_theme(egui::Theme::Dark).with_context_setup(theme::install).build_ui_state(
        |ui, s: &mut Option<Chat>| {
            let chat = s.get_or_insert_with(|| {
                options::save_catalog(ui.ctx(), &catalog());
                Chat::new(ui.ctx(), egui::Id::unique("chat-qa"), core(), "chat-qa".into(), "no-such-site".into(), std::env::temp_dir().join("zenv-chat-qa-missing"), Arc::new(|| {}))
            });
            let rect = ui.max_rect();
            let c = Colors::of(ui);
            chat.ui(ui, rect, &c, None);
        },
        None,
    )
}

fn chat<'a>(h: &'a mut Harness<'static, Option<Chat>>) -> &'a mut Chat {
    h.state_mut().as_mut().expect("chat")
}

fn open_mode_menu(h: &mut Harness<'static, Option<Chat>>) {
    h.get_by_label_contains("Permission mode:").click();
    h.run_steps(AREA_SETTLE);
}

#[test]
#[ignore = "measurement: cargo test --release -p zenv-native --test chat_qa -- --ignored --nocapture"]
fn qa_pasted_screenshot_cost_on_the_ui_thread() {
    for (w, h) in [(1920usize, 1080usize), (3840, 2160)] {
        let pixels: Vec<egui::Color32> = (0..w * h).map(|i| egui::Color32::from_rgb((i % 251) as u8, (i / w % 241) as u8, (i % 7 * 30) as u8)).collect();
        let img = egui::ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels };
        let t = std::time::Instant::now();
        let a = options::attachment_from_image(&img).expect("png");
        println!("{w}x{h}: {:.0} ms, {} KB base64", t.elapsed().as_secs_f64() * 1000.0, a.data.len() / 1024);
    }
}

#[test]
fn qa_mode_chip_still_opens_after_a_mode_pick_and_a_sent_message() {
    let mut h = harness();
    h.run_steps(2);
    open_mode_menu(&mut h);
    h.get_by_label("Plan").click();
    h.run_steps(2);
    assert_eq!(chat(&mut h).options.mode, Some(PermissionMode::Plan));
    let c = chat(&mut h);
    c.state.sent("Reply with just the word pong.", vec![]);
    c.state.apply(&[
        json!({ "type": "system", "subtype": "init", "session_id": "s1", "model": "claude-haiku-5-5", "permissionMode": "plan" }),
        json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "pong" }] } }),
        json!({ "type": "result", "subtype": "success", "is_error": false, "duration_ms": 2900 }),
    ]);
    c.focus = true;
    h.run_steps(2);
    h.event(egui::Event::Text("ping".into()));
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
    open_mode_menu(&mut h);
    assert!(h.query_by_label("Ask first").is_some(), "the mode menu opens again after a conversation");
    h.get_by_label("Ask first").click();
    h.run_steps(2);
    assert_eq!(chat(&mut h).options.mode, Some(PermissionMode::Manual));
}
