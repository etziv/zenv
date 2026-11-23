use std::collections::HashSet;

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Layout, Margin, Rect, RichText, Sense, Stroke, Ui};
use serde_json::Value;

use super::markdown;
use super::model::{Answer, Tool};
use crate::studio::Colors;
use crate::{lucide, theme};

pub enum Out {
    Answer(String, bool, bool),
    Open(String),
}

fn icon(name: &str) -> &'static str {
    match name {
        "Bash" | "PowerShell" => lucide::TERMINAL,
        "Read" => lucide::FILE_TEXT,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => lucide::FILE_PEN,
        "Glob" | "Grep" => lucide::SEARCH,
        "WebFetch" | "WebSearch" => lucide::GLOBE,
        "Task" | "Agent" => lucide::BOT,
        "TodoWrite" => lucide::LIST_CHECKS,
        "ExitPlanMode" => lucide::LIST_TODO,
        _ => lucide::WRENCH,
    }
}

fn s<'a>(input: &'a Value, key: &str) -> &'a str {
    input[key].as_str().unwrap_or("")
}

pub fn summary(name: &str, input: &Value) -> String {
    match name {
        "Bash" | "PowerShell" => s(input, "command").to_owned(),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some(s(input, "file_path")).filter(|p| !p.is_empty()).unwrap_or(s(input, "notebook_path")).to_owned(),
        "Glob" | "Grep" => s(input, "pattern").to_owned(),
        "WebFetch" => s(input, "url").to_owned(),
        "WebSearch" => s(input, "query").to_owned(),
        "Task" | "Agent" => s(input, "description").to_owned(),
        "TodoWrite" => input["todos"].as_array().map(|t| format!("{} tasks", t.len())).unwrap_or_default(),
        _ => input.as_object().and_then(|o| o.values().find_map(Value::as_str)).unwrap_or("").to_owned(),
    }
}

fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    if parts.len() > 3 { format!("…/{}", parts[parts.len() - 3..].join("/")) } else { path.to_owned() }
}

const SHOWN_LINES: usize = 80;
const SHOWN_CHARS: usize = 4000;

fn first_lines(t: &str) -> (Vec<&str>, usize) {
    let all: Vec<&str> = t.split('\n').collect();
    let more = all.len().saturating_sub(SHOWN_LINES);
    (all.into_iter().take(SHOWN_LINES).collect(), more)
}

fn clip(t: &str) -> String {
    match t.char_indices().nth(SHOWN_CHARS) {
        Some((i, _)) => format!("{}\n… {} more characters not shown", &t[..i], t[i..].chars().count()),
        None => t.to_owned(),
    }
}

fn pre(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(Color32::from_black_alpha(102)).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().id_salt(salt).max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            body(ui);
        });
    });
}

fn mono(text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text).font(FontId::new(11.0, FontFamily::Monospace)).color(color)
}

fn input_view(ui: &mut Ui, key: u64, t: &Tool, c: &Colors) {
    let input = &t.input;
    match t.name.as_str() {
        "Bash" | "PowerShell" => pre(ui, ("tool-in", key), |ui| {
            ui.add(egui::Label::new(mono(s(input, "command"), c.text)).wrap());
        }),
        "Edit" => pre(ui, ("tool-in", key), |ui| {
            for (sign, text, color) in [("-", s(input, "old_string"), c.fail), ("+", s(input, "new_string"), c.led)] {
                let (lines, more) = first_lines(text);
                for l in lines {
                    let r = ui.add(egui::Label::new(mono(format!("{sign} {l}"), color)).wrap());
                    ui.painter().rect_filled(r.rect.expand2(vec2(2.0, 0.0)), 0.0, color.gamma_multiply(0.1));
                }
                if more > 0 {
                    ui.label(mono(format!("{sign} … {more} more lines not shown"), c.muted));
                }
            }
        }),
        "Write" => pre(ui, ("tool-in", key), |ui| {
            ui.add(egui::Label::new(mono(clip(s(input, "content")), c.text)).wrap());
        }),
        "ExitPlanMode" if input["plan"].is_string() => markdown::show(ui, &markdown::parse(s(input, "plan")), c),
        "TodoWrite" if input["todos"].is_array() => {
            ui.spacing_mut().item_spacing.y = 2.0;
            for todo in input["todos"].as_array().into_iter().flatten() {
                let mut text = RichText::new(s(todo, "content")).size(12.0);
                text = match s(todo, "status") {
                    "completed" => text.color(c.muted).strikethrough(),
                    "in_progress" => text.color(c.led),
                    _ => text.color(c.text),
                };
                ui.add(egui::Label::new(text).wrap());
            }
        }
        _ => pre(ui, ("tool-in", key), |ui| {
            ui.add(egui::Label::new(mono(clip(&serde_json::to_string_pretty(input).unwrap_or_default()), c.muted)).wrap());
        }),
    }
}

fn button(ui: &mut Ui, label: &str, fill: Option<Color32>, border: Color32, text: Color32) -> bool {
    let font = FontId::proportional(12.0);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), text).size().x) + 20.0;
    let (rect, r) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let fill = fill.map(|f| if r.hovered() { f.gamma_multiply(0.9) } else { f }).unwrap_or(if r.hovered() { border.gamma_multiply(0.25) } else { Color32::TRANSPARENT });
    ui.painter().rect(rect, CornerRadius::same(4), fill, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, text);
    r.clicked()
}

pub fn card(ui: &mut Ui, key: u64, t: &Tool, toggled: &mut HashSet<u64>, c: &Colors, out: &mut Vec<Out>) {
    let pending = t.permission.as_ref().is_some_and(|p| p.state == Answer::Pending);
    let plan = t.name == "ExitPlanMode";
    let open = plan != toggled.contains(&key);
    let expanded = open || pending;
    let text = summary(&t.name, &t.input);
    let file = s(&t.input, "file_path").to_owned();
    let border = if pending { theme::panel::AMBER.gamma_multiply(0.5) } else { c.line };
    egui::Frame::new().fill(c.raised.gamma_multiply(0.4)).stroke(Stroke::new(1.0, border)).corner_radius(CornerRadius::same(theme::RADIUS)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let (row, r) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{} {text}", t.name)));
        if r.clicked() && !pending {
            if !toggled.remove(&key) {
                toggled.insert(key);
            }
        }
        let p = ui.painter().with_clip_rect(row);
        let y = row.center().y;
        let chevron = if expanded { lucide::CHEVRON_DOWN } else { lucide::CHEVRON_RIGHT };
        p.text(pos2(row.left() + 10.0 + 6.0, y), Align2::CENTER_CENTER, chevron, FontId::new(12.0, theme::icons()), c.muted);
        p.text(pos2(row.left() + 28.0 + 7.0, y), Align2::CENTER_CENTER, icon(&t.name), FontId::new(14.0, theme::icons()), c.muted);
        let name = p.text(pos2(row.left() + 50.0, y), Align2::LEFT_CENTER, &t.name, FontId::new(12.0, theme::medium()), Color32::WHITE);
        let shown = if file.is_empty() { text.clone() } else { short_path(&file) };
        let room = (row.right() - 22.0 - (name.right() + 8.0)).max(0.0);
        let galley = ui.painter().layout(shown, FontId::new(11.0, FontFamily::Monospace), c.muted, f32::INFINITY);
        let detail = Rect::from_min_size(pos2(name.right() + 8.0, y - galley.size().y / 2.0), vec2(room, galley.size().y));
        ui.painter().with_clip_rect(detail).galley(detail.min, galley, c.muted);
        let dot = match (&t.result, t.permission.as_ref().map(|p| p.state)) {
            (Some((_, false)), _) => crate::theme::panel::LED,
            (Some((_, true)), _) | (None, Some(Answer::Denied)) => c.fail,
            _ if pending => theme::panel::AMBER,
            _ => c.muted.gamma_multiply(0.4 + 0.6 * (ui.input(|i| i.time) * 3.0).sin().abs() as f32),
        };
        p.circle_filled(pos2(row.right() - 12.0, y), 3.0, dot);
        if !text.is_empty() {
            r.on_hover_text(&text);
        }
        if expanded {
            crate::studio::hline(ui, c.line);
            egui::Frame::new().inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                input_view(ui, key, t, c);
                if !file.is_empty() && ui.add(egui::Label::new(mono("Open in editor", c.led)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    out.push(Out::Open(file.clone()));
                }
                if let Some((result, error)) = &t.result {
                    pre(ui, ("tool-out", key), |ui| {
                        let shown = if result.is_empty() { "(no output)" } else { result };
                        ui.add(egui::Label::new(mono(shown, if *error { c.fail } else { c.muted })).wrap());
                    });
                }
            });
        }
        match &t.permission {
            Some(perm) if pending => {
                crate::studio::hline(ui, c.line);
                egui::Frame::new().inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                        let ask = if plan { "Start with this plan?".to_owned() } else { format!("Allow Claude to use {}?", t.name) };
                        ui.label(RichText::new(ask).size(12.0).color(c.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, if plan { "Keep planning" } else { "Deny" }, None, c.line, c.text) {
                                out.push(Out::Answer(perm.request_id.clone(), false, false));
                            }
                            if let Some(always) = &perm.always {
                                if button(ui, always, None, c.led.gamma_multiply(0.5), c.led) {
                                    out.push(Out::Answer(perm.request_id.clone(), true, true));
                                }
                            }
                            if button(ui, if plan { "Approve" } else { "Allow" }, Some(c.led), c.led, Color32::BLACK) {
                                out.push(Out::Answer(perm.request_id.clone(), true, false));
                            }
                        });
                    });
                });
            }
            Some(perm) if t.result.is_none() => {
                crate::studio::hline(ui, c.line);
                egui::Frame::new().inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                    ui.label(RichText::new(if perm.state == Answer::Allowed { "Allowed" } else { "Denied" }).size(11.0).color(c.muted));
                });
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summaries_name_what_the_tool_works_on() {
        assert_eq!(summary("Bash", &json!({ "command": "npm test" })), "npm test");
        assert_eq!(summary("NotebookEdit", &json!({ "notebook_path": "a.ipynb" })), "a.ipynb");
        assert_eq!(summary("TodoWrite", &json!({ "todos": [{}, {}] })), "2 tasks");
        assert_eq!(summary("mcp__x__y", &json!({ "n": 1, "q": "find me" })), "find me");
        assert_eq!(short_path("C:/a/b/c/d/e.rs"), "…/c/d/e.rs");
        assert!(clip(&"é".repeat(4001)).ends_with("… 1 more characters not shown"));
        let long: String = (0..100).map(|i| format!("l{i}
")).collect();
        let (shown, more) = first_lines(&long);
        assert_eq!((shown.len(), more), (80, 21), "the rest is counted, not silently dropped");
    }
}
