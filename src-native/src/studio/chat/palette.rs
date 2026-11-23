use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Id, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke, Ui};
use serde_json::Value;
use zenv_core::studio_chat::{ChatOptions, Effort};

use super::composer::{Action, Env};
use super::options::{self, EFFORTS, LEVELS};
use crate::studio::Colors;
use crate::{lucide, theme};

pub enum Out {
    Act(Action),
    Close,
    Attach,
    Mention,
    Commands,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum View {
    #[default]
    Main,
    Models,
    Styles,
    Mcp,
    Account,
}

impl View {
    fn title(self) -> &'static str {
        match self {
            View::Main => "",
            View::Models => "Switch model",
            View::Styles => "Output style",
            View::Mcp => "MCP servers",
            View::Account => "Account & usage",
        }
    }
}

#[derive(Default)]
pub struct Palette {
    pub view: View,
    pub filter: String,
    pick: usize,
    focused: bool,
}

enum Right {
    None,
    Muted(String),
    Switch(bool),
    Dots(Vec<Effort>),
    Term,
}

enum Run {
    Done(Out),
    Go(View),
    Set(ChatOptions),
}

pub struct Row {
    section: &'static str,
    pub label: String,
    detail: Option<String>,
    right: Right,
    run: Run,
}

fn term(section: &'static str, label: &str, screen: Option<&str>) -> Row {
    Row { section, label: label.into(), detail: None, right: Right::Term, run: Run::Done(Out::Act(Action::Terminal(screen.map(str::to_owned)))) }
}

pub fn rows(env: &Env) -> Vec<Row> {
    let o = env.options;
    let set = |f: &dyn Fn(&mut ChatOptions)| {
        let mut n = o.clone();
        f(&mut n);
        Run::Set(n)
    };
    let chosen = env.models.iter().find(|m| m.value == o.model.as_deref().unwrap_or("default"));
    let model_name = match &o.model {
        Some(m) => chosen.map_or(m.clone(), |c| c.display_name.clone()),
        None => options::pretty_model(env.running_model).or_else(|| options::pretty_model(chosen.and_then(|c| c.resolved_model.as_deref()))).unwrap_or_else(|| "Default".into()),
    };
    let levels = chosen.and_then(|m| m.supported_effort_levels.clone()).unwrap_or_else(|| LEVELS.to_vec());
    let next_effort = {
        let i = o.effort.and_then(|e| levels.iter().position(|l| *l == e));
        let next = match i {
            None => levels.first().copied(),
            Some(i) => levels.get(i + 1).copied(),
        };
        set(&|n| n.effort = next)
    };
    let style = o.output_style.clone().or_else(|| env.catalog.and_then(|c| c.output_style.clone())).unwrap_or_else(|| "default".into());
    vec![
        Row { section: "Context", label: "Attach file…".into(), detail: None, right: Right::None, run: Run::Done(Out::Attach) },
        Row { section: "Context", label: "Mention file from this project…".into(), detail: None, right: Right::None, run: Run::Done(Out::Mention) },
        Row { section: "Context", label: "Clear conversation".into(), detail: None, right: Right::None, run: Run::Done(Out::Act(Action::Clear)) },
        Row { section: "Context", label: "Export conversation".into(), detail: Some("copies it as Markdown".into()), right: Right::None, run: Run::Done(Out::Act(Action::Export)) },
        Row { section: "Model", label: "Switch model…".into(), detail: None, right: Right::Muted(model_name), run: Run::Go(View::Models) },
        Row { section: "Model", label: format!("Effort ({})", options::effort_label(o.effort)), detail: None, right: Right::Dots(levels), run: next_effort },
        Row { section: "Model", label: "Thinking".into(), detail: None, right: Right::Switch(o.thinking != Some(false)), run: set(&|n| n.thinking = Some(o.thinking == Some(false))) },
        Row { section: "Model", label: "Switch models when a message is flagged".into(), detail: None, right: Right::Switch(o.switch_on_flag == Some(true)), run: set(&|n| n.switch_on_flag = Some(o.switch_on_flag != Some(true))) },
        Row { section: "Model", label: "Account & usage…".into(), detail: None, right: Right::None, run: Run::Go(View::Account) },
        Row {
            section: "Model",
            label: "Fast mode".into(),
            detail: (chosen.and_then(|m| m.supports_fast_mode) == Some(false)).then(|| "not for this model".into()),
            right: Right::Switch(o.fast == Some(true)),
            run: set(&|n| n.fast = Some(o.fast != Some(true))),
        },
        Row { section: "Customize", label: "Output styles".into(), detail: None, right: Right::Muted(style), run: Run::Go(View::Styles) },
        Row { section: "Customize", label: "MCP servers".into(), detail: None, right: Right::None, run: Run::Go(View::Mcp) },
        term("Customize", "Hooks", Some("/hooks")),
        term("Customize", "Permissions", Some("/permissions")),
        term("Customize", "Status", Some("/status")),
        term("Customize", "Sandbox", Some("/sandbox")),
        Row { section: "Customize", label: "Slash commands".into(), detail: None, right: Right::None, run: Run::Done(Out::Commands) },
        term("Customize", "Memory", Some("/memory")),
        Row { section: "Customize", label: "Instructions".into(), detail: Some("CLAUDE.md in the editor".into()), right: Right::None, run: Run::Done(Out::Act(Action::Instructions)) },
        term("Customize", "Agents", Some("/agents")),
        term("Customize", "Manage plugins", Some("/plugin")),
        term("Customize", "Open Claude in Terminal", None),
        term("Settings", "Switch account", Some("/login")),
        term("Settings", "Sign out", Some("/logout")),
        term("Settings", "General config…", Some("/config")),
        Row { section: "Support", label: "View help docs".into(), detail: None, right: Right::None, run: Run::Done(Out::Act(Action::Docs)) },
        Row { section: "Support", label: "Doctor".into(), detail: Some("checks the installation".into()), right: Right::Term, run: Run::Done(Out::Act(Action::Terminal(Some("/doctor".into())))) },
    ]
}

pub fn filtered(rows: Vec<Row>, filter: &str) -> Vec<Row> {
    let q = filter.trim().to_lowercase();
    rows.into_iter().filter(|r| q.is_empty() || r.label.to_lowercase().contains(&q) || r.section.to_lowercase().contains(&q)).collect()
}

impl Palette {
    fn go(&mut self, view: View, out: &mut Vec<Out>) {
        self.view = view;
        self.filter.clear();
        self.pick = 0;
        self.focused = false;
        if view == View::Mcp {
            out.push(Out::Act(Action::McpLoad));
        }
    }

    fn run(&mut self, run: Run, out: &mut Vec<Out>) {
        match run {
            Run::Done(o) => {
                out.push(o);
                out.push(Out::Close);
            }
            Run::Go(v) => self.go(v, out),
            Run::Set(o) => out.push(Out::Act(Action::Options(o))),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, id: Id, above: Rect, env: &Env, c: &Colors) -> Vec<Out> {
        let mut out = Vec::new();
        let shown = if self.view == View::Main { filtered(rows(env), &self.filter) } else { Vec::new() };
        let filter_id = id.with("filter");
        let ours = ctx.memory(|m| m.focused().is_none_or(|f| f == filter_id));
        let (esc, down, up, enter) = ctx.input_mut(|i| {
            (
                ours && i.consume_key(Modifiers::NONE, Key::Escape),
                ours && i.consume_key(Modifiers::NONE, Key::ArrowDown),
                ours && i.consume_key(Modifiers::NONE, Key::ArrowUp),
                ours && i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        let mut run = None;
        if esc {
            if self.view == View::Main {
                out.push(Out::Close);
            } else {
                self.go(View::Main, &mut out);
            }
        } else if !shown.is_empty() {
            let n = shown.len();
            if down || up {
                self.pick = (self.pick + if down { 1 } else { n - 1 }) % n;
            }
            if enter {
                run = Some(self.pick.min(n - 1));
            }
        }
        let max_h = ctx.content_rect().height() * 0.6;
        let mut clicked = None;
        let mut back = false;
        let mut dot_pick = None;
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos2(above.left(), above.top() - 4.0)).pivot(Align2::LEFT_BOTTOM).show(ctx, |ui| {
            egui::Frame::new().fill(c.raised).stroke(Stroke::new(1.0, c.line)).corner_radius(CornerRadius::same(theme::RADIUS)).show(ui, |ui| {
                ui.set_width(above.width());
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    if self.view != View::Main && crate::studio::panel_icon_button(ui, lucide::ARROW_LEFT, "Back", false, 24.0, 14.0).clicked() {
                        back = true;
                    }
                    if self.view == View::Main {
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .id(filter_id)
                                .frame(egui::Frame::new())
                                .desired_width(f32::INFINITY)
                                .min_size(vec2(0.0, 36.0))
                                .align(egui::Align2::LEFT_CENTER)
                                .font(FontId::proportional(12.5))
                                .text_color(Color32::WHITE)
                                .hint_text(RichText::new("Filter actions…").color(c.muted)),
                        );
                        if !self.focused {
                            edit.request_focus();
                            self.focused = true;
                        }
                        if edit.changed() {
                            self.pick = 0;
                        }
                    } else {
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                        ui.painter().text(pos2(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, self.view.title(), FontId::proportional(12.5), c.muted);
                    }
                });
                crate::studio::hline(ui, c.line);
                egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                    egui::Frame::new().inner_margin(Margin::same(4)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.view {
                            View::Main => {
                                let mut last = "";
                                for (i, r) in shown.iter().enumerate() {
                                    if r.section != last {
                                        if i > 0 {
                                            ui.add_space(4.0);
                                            crate::studio::hline(ui, c.line);
                                        }
                                        egui::Frame::new().inner_margin(Margin { left: 8, right: 8, top: 8, bottom: 4 }).show(ui, |ui| {
                                            ui.label(RichText::new(r.section).size(11.0).color(c.muted));
                                        });
                                        last = r.section;
                                    }
                                    let (row_clicked, dot) = row_ui(ui, r, i == self.pick, env.options.effort, c);
                                    if row_clicked.hovered {
                                        self.pick = i;
                                    }
                                    if row_clicked.clicked {
                                        clicked = Some(i);
                                    }
                                    if dot.is_some() {
                                        dot_pick = dot;
                                    }
                                }
                                if shown.is_empty() {
                                    ui.label(RichText::new("No action matches.").size(12.5).color(c.muted));
                                }
                            }
                            View::Models => {
                                for m in env.models {
                                    let value = (m.value != "default").then(|| m.value.clone());
                                    if panel_choice(ui, env.options.model == value, &m.display_name, m.description.as_deref(), c) {
                                        out.push(Out::Act(Action::Options(ChatOptions { model: value, ..env.options.clone() })));
                                        out.push(Out::Close);
                                    }
                                }
                            }
                            View::Styles => {
                                let styles = env.catalog.and_then(|c| c.output_styles.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| vec!["default".into()]);
                                let current = env.options.output_style.clone().or_else(|| env.catalog.and_then(|c| c.output_style.clone())).unwrap_or_else(|| "default".into());
                                for s in styles {
                                    if panel_choice(ui, current == s, &s, None, c) {
                                        let style = (s != "default").then_some(s);
                                        out.push(Out::Act(Action::Options(ChatOptions { output_style: style, ..env.options.clone() })));
                                        out.push(Out::Close);
                                    }
                                }
                            }
                            View::Mcp => mcp_ui(ui, env, c, &mut out),
                            View::Account => account_ui(ui, env, c),
                        }
                    });
                });
            });
        });
        if back {
            self.go(View::Main, &mut out);
        }
        if let Some(e) = dot_pick {
            let effort = if env.options.effort == Some(e) { None } else { Some(e) };
            out.push(Out::Act(Action::Options(ChatOptions { effort, ..env.options.clone() })));
        } else if let Some(i) = run.or(clicked) {
            if let Some(r) = shown.into_iter().nth(i) {
                self.run(r.run, &mut out);
            }
        }
        out
    }
}

struct RowResponse {
    hovered: bool,
    clicked: bool,
}

fn row_ui(ui: &mut Ui, r: &Row, picked: bool, effort: Option<Effort>, c: &Colors) -> (RowResponse, Option<Effort>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MenuItem, true, &r.label));
    if picked {
        ui.painter().rect_filled(rect, CornerRadius::same(4), c.line);
    }
    let y = rect.center().y;
    let p = ui.painter().with_clip_rect(rect);
    let label = p.text(pos2(rect.left() + 8.0, y), Align2::LEFT_CENTER, &r.label, FontId::proportional(12.5), if picked { Color32::WHITE } else { c.text });
    if let Some(d) = &r.detail {
        p.text(pos2(label.right() + 8.0, y), Align2::LEFT_CENTER, d, FontId::proportional(11.0), c.muted);
    }
    let right = rect.right() - 8.0;
    let mut dot = None;
    match &r.right {
        Right::None => {}
        Right::Muted(t) => {
            p.text(pos2(right, y), Align2::RIGHT_CENTER, t, FontId::proportional(11.5), c.muted);
        }
        Right::Term => {
            p.text(pos2(right - 7.0, y), Align2::CENTER_CENTER, lucide::TERMINAL, FontId::new(14.0, theme::icons()), c.muted);
        }
        Right::Switch(on) => paint_switch(&p, Rect::from_min_size(pos2(right - 28.0, y - 8.0), vec2(28.0, 16.0)), *on, c),
        Right::Dots(levels) => {
            let w = levels.len() as f32 * 14.0 + 8.0;
            let pill = Rect::from_min_size(pos2(right - w, y - 9.0), vec2(w, 18.0));
            p.rect_filled(pill, CornerRadius::same(9), c.line.gamma_multiply(0.7));
            for (i, l) in levels.iter().enumerate() {
                let center = pos2(pill.left() + 4.0 + 7.0 + i as f32 * 14.0, y);
                let hit = Rect::from_center_size(center, vec2(14.0, 18.0));
                let hovered = resp.hover_pos().is_some_and(|h| hit.contains(h));
                let on = effort == Some(*l);
                let (radius, color) = if on { (5.0, Color32::WHITE) } else if hovered { (3.0, c.text) } else { (3.0, c.muted) };
                p.circle_filled(center, radius, color);
                if resp.clicked() && resp.interact_pointer_pos().is_some_and(|h| hit.contains(h)) {
                    dot = Some(*l);
                }
                if hovered {
                    let name = EFFORTS.iter().find(|(v, _)| *v == Some(*l)).map_or("", |(_, n)| n);
                    resp.clone().on_hover_text(name);
                }
            }
        }
    }
    (RowResponse { hovered: resp.hovered(), clicked: resp.clicked() && dot.is_none() }, dot)
}

pub fn paint_switch(p: &egui::Painter, rect: Rect, on: bool, c: &Colors) {
    p.rect_filled(rect, CornerRadius::same(8), if on { c.led.gamma_multiply(0.8) } else { c.line });
    let x = if on { rect.right() - 8.0 } else { rect.left() + 8.0 };
    p.circle_filled(pos2(x, rect.center().y), 6.0, Color32::WHITE);
}

fn panel_choice(ui: &mut Ui, on: bool, label: &str, detail: Option<&str>, c: &Colors) -> bool {
    let h = if detail.is_some() { 38.0 } else { 28.0 };
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::MenuItem, true, on, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(4), c.line);
    }
    let top = rect.top() + 14.0;
    if on {
        ui.painter().text(pos2(rect.left() + 8.0 + 7.0, top), Align2::CENTER_CENTER, lucide::CHECK, FontId::new(14.0, theme::icons()), c.led);
    }
    ui.painter().text(pos2(rect.left() + 30.0, top), Align2::LEFT_CENTER, label, FontId::proportional(12.5), if r.hovered() { Color32::WHITE } else { c.text });
    if let Some(d) = detail {
        ui.painter().text(pos2(rect.left() + 30.0, rect.bottom() - 9.0), Align2::LEFT_CENTER, d, FontId::proportional(11.0), c.muted);
    }
    r.clicked()
}

fn tools_count(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_array().map(|a| a.len() as u64)).unwrap_or(0)
}

fn mcp_ui(ui: &mut Ui, env: &Env, c: &Colors, out: &mut Vec<Out>) {
    let m = env.mcp;
    ui.spacing_mut().item_spacing.y = 4.0;
    if m.servers.is_none() && m.error.is_none() {
        ui.label(RichText::new("Asking Claude Code…").size(12.5).color(c.muted));
    }
    if let Some(e) = &m.error {
        ui.add(egui::Label::new(RichText::new(e).size(12.5).color(c.fail)).wrap());
    }
    if m.servers.as_ref().is_some_and(Vec::is_empty) {
        ui.label(RichText::new("No MCP servers are set up.").size(12.5).color(c.muted));
    }
    for s in m.servers.iter().flatten() {
        let name = s["name"].as_str().unwrap_or("?").to_owned();
        let status = s["status"].as_str().unwrap_or("");
        let enabled = status != "disabled";
        let busy = m.busy.as_deref() == Some(name.as_str());
        let mut detail = status.to_owned();
        let tools = tools_count(&s["tools"]);
        if tools > 0 {
            detail.push_str(&format!(" · {tools} tools"));
        }
        for k in ["scope", "error"] {
            if let Some(v) = s[k].as_str().filter(|v| !v.is_empty()) {
                detail.push_str(&format!(" · {v}"));
            }
        }
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
        let dot = match status {
            "connected" => theme::panel::LED,
            "failed" => c.fail,
            "disabled" => c.muted.gamma_multiply(0.5),
            _ => theme::panel::AMBER,
        };
        let p = ui.painter().with_clip_rect(rect);
        p.circle_filled(pos2(rect.left() + 8.0, rect.center().y), 3.0, dot);
        p.text(pos2(rect.left() + 20.0, rect.top() + 11.0), Align2::LEFT_CENTER, &name, FontId::proportional(12.5), c.text);
        p.text(pos2(rect.left() + 20.0, rect.bottom() - 9.0), Align2::LEFT_CENTER, detail, FontId::proportional(11.0), c.muted);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(rect.right() - 64.0, rect.top()), rect.max)).layout(egui::Layout::right_to_left(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 4.0;
        let (sw, sr) = child.allocate_exact_size(vec2(28.0, 16.0), if busy { Sense::hover() } else { Sense::click() });
        sr.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, !busy, enabled, &name));
        paint_switch(child.painter(), sw, enabled, c);
        if sr.on_hover_text(if enabled { "Turn off for this chat" } else { "Turn on" }).clicked() {
            out.push(Out::Act(Action::McpToggle(name.clone(), !enabled)));
        }
        if matches!(status, "failed" | "needs-auth") && !busy && crate::studio::panel_icon_button(&mut child, lucide::REFRESH_CW, "Reconnect", false, 24.0, 13.0).clicked() {
            out.push(Out::Act(Action::McpReconnect(name.clone())));
        }
    }
    ui.add_space(4.0);
    crate::studio::hline(ui, c.line);
    ui.add_space(4.0);
    let link = ui.add(egui::Label::new(RichText::new(format!("{}  Add or configure servers (/mcp in a terminal)", lucide::TERMINAL)).size(12.0).color(c.led)).sense(Sense::click()));
    if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        out.push(Out::Act(Action::Terminal(Some("/mcp".into()))));
        out.push(Out::Close);
    }
}

fn account_ui(ui: &mut Ui, env: &Env, c: &Colors) {
    egui::Frame::new().inner_margin(Margin::same(6)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let a = env.catalog.and_then(|c| c.account.as_ref());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let email = a.and_then(|a| a.email.clone()).unwrap_or_else(|| "Account unknown until Claude Code answers".into());
            ui.label(RichText::new(email).size(12.0).color(Color32::WHITE));
            let plan: Vec<String> = [a.and_then(|a| a.subscription_type.as_ref().map(|s| format!("Claude {s}"))), a.and_then(|a| a.organization.clone())].into_iter().flatten().collect();
            if !plan.is_empty() {
                ui.label(RichText::new(plan.join(" · ")).size(12.0).color(c.muted));
            }
        });
        let now = options::now_ms();
        for (label, w) in [("5-hour limit", env.limits.five_hour), ("Weekly limit", env.limits.seven_day)] {
            let Some(w) = w else { continue };
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(12.0).color(c.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{}% · resets in {}", (w.utilization * 100.0).round(), options::until_text(w.resets_at, now))).size(12.0).color(c.muted));
                    });
                });
                let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                ui.painter().rect_filled(bar, CornerRadius::same(3), c.line);
                let fill = if w.utilization >= 0.9 { c.fail } else if w.utilization >= 0.75 { theme::panel::AMBER } else { c.led };
                let width = bar.width() * w.utilization.clamp(0.0, 1.0) as f32;
                ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(width, bar.height())), CornerRadius::same(3), fill);
            });
        }
        if env.limits.five_hour.is_none() {
            ui.label(RichText::new("Usage shows after the first message of this chat.").size(11.0).color(c.muted));
        }
        if let Some(v) = env.catalog.and_then(|c| c.version.clone()) {
            ui.label(RichText::new(format!("Claude Code {v}")).size(11.0).color(c.muted));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_matches_labels_and_sections() {
        let labels = |rows: Vec<Row>| rows.into_iter().map(|r| r.label).collect::<Vec<_>>();
        let opts = ChatOptions::default();
        let mcp = super::super::Mcp::default();
        let repaint: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(|| {});
        let models = options::fallback_models();
        let env = Env {
            busy: false,
            commands: &[],
            models: &models,
            catalog: None,
            running_model: None,
            reported_mode: None,
            limits: Default::default(),
            active_file: None,
            root: std::path::Path::new("."),
            options: &opts,
            mcp: &mcp,
            focus: false,
            repaint: &repaint,
        };
        assert_eq!(labels(filtered(rows(&env), "mcp")), ["MCP servers"]);
        assert_eq!(labels(filtered(rows(&env), "settings")), ["Switch account", "Sign out", "General config…"]);
        assert_eq!(filtered(rows(&env), "").len(), 27);
        let effort = rows(&env).into_iter().find(|r| r.label.starts_with("Effort")).unwrap();
        assert!(matches!(effort.run, Run::Set(o) if o.effort == Some(Effort::Low)), "effort steps up from the default");
    }
}
