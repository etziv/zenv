pub mod composer;
pub mod conn;
pub mod markdown;
pub mod model;
pub mod options;
pub mod palette;
mod tools;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Layout, Margin, Rect, RichText, Sense, Stroke, Ui};
use serde_json::Value;
use zenv_core::app_core::Core;
use zenv_core::studio_chat::{Attachment, ChatControl, ChatOptions};
use zenv_core::studio_history::{self, SessionSummary};

use super::{panel_icon_button, Colors};
use crate::widgets::{self, Led};
use crate::{lucide, theme};
use composer::{Action, Composer, Env};
use conn::{lock, Event, Op, Shared, Start, Target};
use markdown::Block;
use model::{ChatState, Entry, Item};
use options::{Catalog, Command, Model};

const SHOWN: usize = 200;
const HEADER: f32 = 32.0;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(20);
const DOCS: &str = "https://code.claude.com/docs";

pub enum Request {
    OpenFile(String),
    Terminal(Option<String>),
}

#[derive(Default)]
pub struct Mcp {
    pub servers: Option<Vec<Value>>,
    pub error: Option<String>,
    pub busy: Option<String>,
    waiting: Option<u64>,
}

struct Wait {
    request_id: Option<String>,
    since: Instant,
    reload: bool,
}

type Job<T> = Option<Receiver<T>>;

fn job<T: Send + 'static>(repaint: &Arc<dyn Fn() + Send + Sync>, f: impl FnOnce() -> T + Send + 'static) -> Job<T> {
    let (tx, rx) = channel();
    let repaint = repaint.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
        repaint();
    });
    Some(rx)
}

fn take<T>(job: &mut Job<T>) -> Option<T> {
    let got = job.as_ref()?.try_recv().ok()?;
    *job = None;
    Some(got)
}

pub struct Chat {
    id: Id,
    pub state: ChatState,
    shared: Arc<Mutex<Shared>>,
    ops: Sender<Op>,
    generation: u64,
    target: Target,
    root: PathBuf,
    pub options: ChatOptions,
    pub catalog: Option<Catalog>,
    catalog_stale: bool,
    seen_mode: Option<String>,
    catalog_job: Job<Option<Value>>,
    sessions: Option<Vec<SessionSummary>>,
    sessions_job: Job<Vec<SessionSummary>>,
    history_job: Job<Result<(String, Vec<Value>), String>>,
    instructions_job: Job<Result<(), String>>,
    show_all: bool,
    md: HashMap<u64, (String, Vec<Block>)>,
    toggled: HashSet<u64>,
    pub composer: Composer,
    pub mcp: Mcp,
    waits: HashMap<u64, Wait>,
    next_tag: u64,
    flash: Option<(String, Instant)>,
    pub focus: bool,
    pub requests: Vec<Request>,
}

impl Chat {
    pub fn new(ctx: &egui::Context, id: Id, core: Arc<Core>, owner: String, site_id: String, root: PathBuf, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let target = Target { core, owner, site_id, repaint };
        let (catalog, fresh) = options::read_catalog(ctx);
        Self {
            id,
            state: ChatState::default(),
            ops: conn::spawn(target.clone(), shared.clone()),
            shared,
            generation: 0,
            target,
            root,
            options: options::read_options(ctx),
            catalog,
            catalog_stale: !fresh,
            seen_mode: None,
            catalog_job: None,
            sessions: None,
            sessions_job: None,
            history_job: None,
            instructions_job: None,
            show_all: false,
            md: HashMap::new(),
            toggled: HashSet::new(),
            composer: Composer::default(),
            mcp: Mcp::default(),
            waits: HashMap::new(),
            next_tag: 1,
            flash: None,
            focus: true,
            requests: Vec::new(),
        }
    }

    pub fn applied(&self) -> Option<ChatOptions> {
        lock(&self.shared).live.as_ref().filter(|l| l.generation == self.generation).map(|l| l.options.clone())
    }

    fn start(&self) -> Start {
        Start { generation: self.generation, options: self.options.clone(), resume: self.state.session_id.clone() }
    }

    fn end(&mut self) {
        self.generation += 1;
        {
            let mut s = lock(&self.shared);
            s.generation = self.generation;
            s.lines.clear();
        }
        self.waits.clear();
        let _ = self.ops.send(Op::End);
    }

    pub fn close(&mut self) {
        self.end();
    }

    pub fn send(&mut self, text: String, attachments: Vec<Attachment>) {
        if text == "/clear" && attachments.is_empty() {
            self.new_chat();
            return;
        }
        let restart = conn::needs_restart(self.applied().as_ref(), &self.options, self.state.busy);
        self.state.sent(&text, attachments.iter().map(|a| a.name.clone()).collect());
        if restart {
            self.end();
        }
        let _ = self.ops.send(Op::Send { start: self.start(), text, attachments });
    }

    pub fn stop(&self) {
        let _ = self.ops.send(Op::Interrupt);
    }

    pub fn answer(&mut self, request_id: &str, allow: bool, always: bool) {
        self.state.answered(request_id, allow, always);
        let _ = self.ops.send(Op::Answer { request_id: request_id.to_owned(), allow, always });
    }

    pub fn new_chat(&mut self) {
        self.end();
        self.show_all = false;
        self.state = ChatState::default();
        self.md.clear();
        self.toggled.clear();
    }

    fn control(&mut self, control: ChatControl, reload: bool) -> u64 {
        let tag = self.next_tag;
        self.next_tag += 1;
        self.waits.insert(tag, Wait { request_id: None, since: Instant::now(), reload });
        let _ = self.ops.send(Op::Control { start: self.start(), tag, control });
        tag
    }

    fn mcp_load(&mut self) {
        self.mcp.waiting = Some(self.control(ChatControl::McpStatus, false));
    }

    fn mcp_act(&mut self, server: String, control: ChatControl) {
        self.mcp.busy = Some(server);
        self.control(control, true);
    }

    fn controlled(&mut self, tag: u64, reload: bool, result: Result<Value, String>) {
        match result {
            Err(e) => {
                self.mcp.error = Some(e);
                self.mcp.busy = None;
            }
            Ok(_) if reload => {
                self.mcp.busy = None;
                self.mcp_load();
            }
            Ok(r) if self.mcp.waiting == Some(tag) => {
                self.mcp.waiting = None;
                self.mcp.servers = Some(r["mcpServers"].as_array().cloned().unwrap_or_default());
            }
            Ok(_) => {}
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let (lines, events, mut responses) = {
            let mut s = lock(&self.shared);
            (std::mem::take(&mut s.lines), std::mem::take(&mut s.events), std::mem::take(&mut s.responses))
        };
        let lines: Vec<Value> = lines.into_iter().filter(|(g, _)| *g == self.generation).map(|(_, v)| v).collect();
        if !lines.is_empty() {
            self.state.apply(&lines);
        }
        for e in events {
            match e {
                Event::Error(msg) => self.state.notice(msg, true),
                Event::ControlSent { tag, request_id } => {
                    if let Some(w) = self.waits.get_mut(&tag) {
                        w.request_id = Some(request_id);
                    }
                }
                Event::ControlFailed { tag, error } => {
                    if let Some(w) = self.waits.remove(&tag) {
                        self.controlled(tag, w.reload, Err(error));
                    }
                }
            }
        }
        let mut done = Vec::new();
        for (tag, w) in &self.waits {
            let answer = w.request_id.as_ref().and_then(|rid| responses.remove(rid));
            match answer {
                Some(r) if r["subtype"] == "success" => done.push((*tag, Ok(r["response"].clone()))),
                Some(r) => done.push((*tag, Err(r["error"].as_str().unwrap_or("Claude Code refused").to_owned()))),
                None if w.since.elapsed() > CONTROL_TIMEOUT => done.push((*tag, Err("Claude Code did not answer".into()))),
                None => {}
            }
        }
        if !responses.is_empty() {
            lock(&self.shared).responses.extend(responses);
        }
        for (tag, result) in done {
            if let Some(w) = self.waits.remove(&tag) {
                self.controlled(tag, w.reload, result);
            }
        }
        if !self.waits.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.follow_mode();
        self.poll_jobs(ctx);
    }

    fn follow_mode(&mut self) {
        if self.state.mode == self.seen_mode {
            return;
        }
        self.seen_mode = self.state.mode.clone();
        let Some(m) = self.state.mode.as_deref().and_then(options::mode_of) else { return };
        let mut s = lock(&self.shared);
        let Some(live) = s.live.as_mut().filter(|l| l.generation == self.generation && l.options.mode != Some(m)) else { return };
        live.options.mode = Some(m);
        self.options.mode = Some(m);
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.composer.wants_catalog) {
            self.want_catalog();
        }
        if let Some(Some(raw)) = take(&mut self.catalog_job) {
            options::save_catalog(ctx, &raw);
            self.catalog = Catalog::parse(&raw).or(self.catalog.take());
        }
        if let Some(s) = take(&mut self.sessions_job) {
            self.sessions = Some(s);
        }
        match take(&mut self.history_job) {
            Some(Ok((id, lines))) => {
                self.state = ChatState::history(&id, &lines);
                self.md.clear();
                self.toggled.clear();
            }
            Some(Err(e)) => self.state.notice(e, true),
            None => {}
        }
        match take(&mut self.instructions_job) {
            Some(Ok(())) => self.requests.push(Request::OpenFile("CLAUDE.md".into())),
            Some(Err(e)) => self.state.notice(format!("Couldn't open CLAUDE.md: {e}"), true),
            None => {}
        }
    }

    fn want_catalog(&mut self) {
        if std::mem::take(&mut self.catalog_stale) {
            let (core, site) = (self.target.core.clone(), self.target.site_id.clone());
            self.catalog_job = job(&self.target.repaint, move || core.studio_chat_catalog(&site).ok());
        }
    }

    fn load_session(&mut self, id: String) {
        self.end();
        self.show_all = false;
        let root = self.root.clone();
        self.history_job = job(&self.target.repaint, move || {
            let lines = studio_history::transcript(&root, &id).map_err(|e| e.to_string())?;
            Ok((id, lines.iter().filter_map(|l| serde_json::from_str(l).ok()).collect()))
        });
    }

    fn instructions(&mut self) {
        let root = self.root.clone();
        self.instructions_job = job(&self.target.repaint, move || {
            use zenv_core::project_files;
            if project_files::stat(&root, &["CLAUDE.md".into()])[0].is_none() {
                project_files::create(&root, "CLAUDE.md", project_files::NewKind::File).map_err(|e| e.to_string())?;
            }
            Ok(())
        });
    }

    pub fn export(&self) -> Option<String> {
        let parts: Vec<String> = self
            .state
            .items
            .iter()
            .filter_map(|e| match &e.item {
                Item::User { text, files } if files.is_empty() => Some(format!("**You:** {text}")),
                Item::User { text, files } => Some(format!("**You:** {text}\n_(attached: {})_", files.join(", "))),
                Item::Text { text, .. } => Some(text.clone()),
                Item::Tool(t) => Some(format!("> {}: {}", t.name, tools::summary(&t.name, &t.input))),
                Item::Notice { text, .. } => Some(format!("_{text}_")),
                Item::Result { .. } => None,
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join("\n\n"))
    }

    pub fn models(&self) -> Vec<Model> {
        self.catalog.as_ref().map_or_else(options::fallback_models, |c| c.models.clone())
    }

    pub fn commands(&self) -> Vec<Command> {
        let mut listed = self.catalog.as_ref().map(|c| c.commands.clone()).filter(|c| !c.is_empty()).unwrap_or_else(options::fallback_commands);
        for name in self.state.commands.iter().flatten() {
            if !listed.iter().any(|c| &c.name == name) {
                listed.push(Command { name: name.clone(), description: None });
            }
        }
        listed
    }

    fn act(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Send(text, files) => {
                self.want_catalog();
                self.send(text, files);
            }
            Action::Stop => self.stop(),
            Action::Options(o) => {
                options::save_options(ctx, &o);
                self.options = o;
            }
            Action::Clear => self.new_chat(),
            Action::Export => {
                self.flash = Some(match self.export() {
                    Some(md) => {
                        ctx.copy_text(md);
                        ("Conversation copied as Markdown".into(), Instant::now())
                    }
                    None => ("Nothing to export yet".into(), Instant::now()),
                });
            }
            Action::Terminal(screen) => self.requests.push(Request::Terminal(screen)),
            Action::Instructions => self.instructions(),
            Action::Docs => ctx.open_url(egui::OpenUrl::new_tab(DOCS)),
            Action::McpLoad => {
                self.mcp = Mcp::default();
                self.mcp_load();
            }
            Action::McpToggle(server, enabled) => self.mcp_act(server.clone(), ChatControl::McpToggle { server, enabled }),
            Action::McpReconnect(server) => self.mcp_act(server.clone(), ChatControl::McpReconnect { server }),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, rect: Rect, c: &Colors, active_file: Option<&str>) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        ui.painter().rect_filled(rect, 0.0, c.panel);
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER));
        self.header(ui, header, c);
        ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left(), header.bottom()), vec2(rect.width(), 1.0)), 0.0, c.line);
        let body = Rect::from_min_max(pos2(rect.left(), header.bottom() + 1.0), rect.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(body);
        let models = self.models();
        let commands = self.commands();
        let env = Env {
            busy: self.state.busy,
            commands: &commands,
            models: &models,
            catalog: self.catalog.as_ref(),
            running_model: self.state.model.as_deref(),
            reported_mode: self.state.mode.as_deref(),
            limits: self.state.limits,
            active_file,
            root: &self.root,
            options: &self.options,
            mcp: &self.mcp,
            focus: std::mem::take(&mut self.focus),
            repaint: &self.target.repaint,
        };
        let actions = egui::Panel::bottom(self.id.with("composer"))
            .frame(egui::Frame::new())
            .resizable(false)
            .show_separator_line(false)
            .show(&mut child, |ui| self.composer.ui(ui, &env, c))
            .inner;
        egui::CentralPanel::default().frame(egui::Frame::new()).show(&mut child, |ui| self.conversation(ui, c));
        for a in actions {
            self.act(&ctx, a);
        }
        if self.state.busy {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect, c: &Colors) {
        let mut h = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        h.spacing_mut().item_spacing.x = 8.0;
        let live = self.applied();
        let (led, _) = h.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
        widgets::paint_led(h.painter(), led.center(), 3.0, if self.state.busy || live.is_some() { Led::On } else { Led::Off });
        if self.state.busy {
            let dim = 0.45 * (1.0 - (h.input(|i| i.time) * 4.0).sin().abs() as f32);
            h.painter().circle_filled(led.center(), 3.5, c.panel.gamma_multiply(dim));
        }
        let name = options::pretty_model(self.state.model.as_deref()).unwrap_or_else(|| "Claude Code".into());
        h.add(egui::Label::new(RichText::new(name).font(FontId::new(11.0, FontFamily::Monospace)).color(c.muted)).truncate());
        let flash = self.flash.as_ref().filter(|(_, at)| at.elapsed() < Duration::from_secs(3)).map(|(t, _)| t.clone());
        if let Some(text) = flash {
            h.label(RichText::new(text).size(11.0).color(c.led));
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        } else if live.is_some_and(|l| l != self.options) {
            h.label(RichText::new("· new settings apply to your next message").size(11.0).color(theme::panel::AMBER));
        }
        let mut load = None;
        h.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if panel_icon_button(ui, lucide::SQUARE_PEN, "New chat", false, 28.0, 15.0).clicked() {
                self.new_chat();
            }
            let history = panel_icon_button(ui, lucide::HISTORY, "History: this project's Claude Code sessions", false, 28.0, 15.0);
            if history.clicked() {
                self.sessions = None;
                let root = self.root.clone();
                self.sessions_job = job(&self.target.repaint, move || studio_history::sessions(&root));
            }
            let current = self.state.session_id.clone();
            egui::Popup::menu(&history).width(320.0).align(egui::RectAlign::BOTTOM_END).show(|ui| {
                ui.set_max_width(320.0);
                let muted = theme::palette(ui).muted;
                match &self.sessions {
                    None => {
                        ui.label(RichText::new("Loading…").size(12.0).color(muted));
                    }
                    Some(list) if list.is_empty() => {
                        ui.label(RichText::new("No Claude Code sessions in this project yet.").size(12.0).color(muted));
                    }
                    Some(list) => {
                        egui::ScrollArea::vertical().max_height(384.0).show(ui, |ui| {
                            let now = options::now_ms();
                            for s in list {
                                if session_row(ui, s, current.as_deref() == Some(s.id.as_str()), now).clicked() {
                                    load = Some(s.id.clone());
                                    ui.close();
                                }
                            }
                        });
                    }
                }
            });
        });
        if let Some(id) = load {
            self.load_session(id);
        }
    }

    fn conversation(&mut self, ui: &mut Ui, c: &Colors) {
        if self.state.items.is_empty() {
            empty(ui, c);
            return;
        }
        let hidden = if self.show_all { 0 } else { self.state.items.len().saturating_sub(SHOWN) };
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().id_salt(self.id.with("items")).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 12.0;
                if hidden > 0 {
                    ui.vertical_centered(|ui| {
                        if ui.add(egui::Label::new(RichText::new(format!("Show {hidden} earlier")).size(11.0).color(c.muted)).sense(Sense::click())).clicked() {
                            self.show_all = true;
                        }
                    });
                }
                let (md, toggled) = (&mut self.md, &mut self.toggled);
                for e in &self.state.items[hidden..] {
                    item_ui(ui, e, md, toggled, c, &mut actions);
                }
                if self.state.working() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let (dot, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                        let a = 0.35 + 0.65 * (ui.input(|i| i.time) * 3.0).sin().abs() as f32;
                        ui.painter().circle_filled(dot.center(), 3.0, c.led.gamma_multiply(a));
                        ui.label(RichText::new("Working…").size(12.0).color(c.muted));
                    });
                }
            });
        });
        for a in actions {
            match a {
                tools::Out::Answer(rid, allow, always) => self.answer(&rid, allow, always),
                tools::Out::Open(path) => {
                    if let Some(rel) = model::relative(&self.root.display().to_string(), &path) {
                        self.requests.push(Request::OpenFile(rel));
                    }
                }
            }
        }
    }
}

impl Drop for Chat {
    fn drop(&mut self) {
        let _ = self.ops.send(Op::End);
    }
}

fn session_row(ui: &mut Ui, s: &SessionSummary, current: bool, now: u64) -> egui::Response {
    let p = theme::palette(ui);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &s.title));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), p.accent);
    }
    let title_color = if current { p.primary } else { p.foreground };
    let galley = ui.painter().layout(s.title.clone(), FontId::proportional(12.5), title_color, rect.width() - 16.0);
    let clipped = ui.painter().with_clip_rect(rect.shrink2(vec2(8.0, 0.0)));
    clipped.galley(pos2(rect.left() + 8.0, rect.top() + 5.0), galley, title_color);
    clipped.text(pos2(rect.left() + 8.0, rect.bottom() - 5.0), Align2::LEFT_BOTTOM, options::ago(s.modified, now), FontId::proportional(11.0), p.muted);
    r
}

fn empty(ui: &mut Ui, c: &Colors) {
    let rect = ui.max_rect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(24.0)).layout(Layout::top_down(Align::Center)));
    child.add_space((rect.height() / 2.0 - 60.0).max(0.0));
    child.spacing_mut().item_spacing.y = 8.0;
    child.label(RichText::new(lucide::BOT).font(FontId::new(32.0, theme::icons())).color(c.muted.gamma_multiply(0.4)));
    child.label(RichText::new("Ask Claude Code to build, fix or explain something in this project.").size(14.0).color(c.muted));
    child.label(RichText::new("Enter send · Shift+Enter new line · @ file · / command · paste or drop files").font(FontId::new(11.0, FontFamily::Monospace)).color(c.muted));
    ui.advance_cursor_after_rect(rect);
}

fn item_ui(ui: &mut Ui, e: &Entry, md: &mut HashMap<u64, (String, Vec<Block>)>, toggled: &mut HashSet<u64>, c: &Colors, out: &mut Vec<tools::Out>) {
    match &e.item {
        Item::User { text, files } => {
            let max = ui.available_width() * 0.9 - 24.0;
            let font = FontId::proportional(markdown::SIZE);
            let text_w = if text.is_empty() { 0.0 } else { ui.painter().layout(text.clone(), font.clone(), Color32::WHITE, max).size().x };
            let chips_w: f32 = files.iter().map(|f| ui.painter().layout_no_wrap(f.clone(), FontId::proportional(11.0), Color32::WHITE).size().x + 37.0).sum();
            let inner = text_w.max(chips_w).min(max).ceil();
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                egui::Frame::new().fill(c.raised).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_width(inner);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        if !files.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                for f in files {
                                    file_chip(ui, f, c);
                                }
                            });
                        }
                        if !text.is_empty() {
                            ui.add(egui::Label::new(RichText::new(text).font(font).color(Color32::WHITE)).wrap());
                        }
                    });
                });
            });
        }
        Item::Text { text, .. } => {
            let entry = md.entry(e.key).or_insert_with(|| (String::new(), Vec::new()));
            if entry.0 != *text {
                *entry = (text.clone(), markdown::parse(text));
            }
            ui.vertical(|ui| markdown::show(ui, &entry.1, c));
        }
        Item::Tool(t) => tools::card(ui, e.key, t, toggled, c, out),
        Item::Result { error: true, text, .. } => {
            egui::Frame::new().stroke(Stroke::new(1.0, c.fail.gamma_multiply(0.4))).corner_radius(CornerRadius::same(theme::RADIUS)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(c.fail)).wrap());
            });
        }
        Item::Result { seconds, .. } => {
            let t = seconds.map_or_else(|| "done".to_owned(), |s| format!("done in {s}s"));
            ui.label(RichText::new(t).font(FontId::new(10.5, FontFamily::Monospace)).color(c.muted));
        }
        Item::Notice { text, error } => {
            ui.add(egui::Label::new(RichText::new(text).font(FontId::new(11.0, FontFamily::Monospace)).color(if *error { c.fail } else { c.muted })).wrap());
        }
    }
}

pub fn file_chip(ui: &mut Ui, name: &str, c: &Colors) {
    egui::Frame::new().stroke(Stroke::new(1.0, c.line)).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new(lucide::FILE).font(FontId::new(11.0, theme::icons())).color(c.muted));
            ui.label(RichText::new(name).size(11.0).color(c.muted));
        });
    });
}

pub fn chip_rect(ui: &Ui, rect: Rect, hovered: bool, on: bool, c: &Colors) {
    if hovered || on {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), if on { c.line } else { c.line.gamma_multiply(0.6) });
    }
}

#[cfg(test)]
mod tests_support {
    use super::*;
    use zenv_core::paths::AppPaths;

    pub fn chat(ctx: &egui::Context, name: &str) -> Chat {
        let dir = std::env::temp_dir().join(format!("zenv-chat-unit-{name}-{}", std::process::id()));
        let core = Arc::new(Core::initialize(AppPaths::new(dir)).expect("core"));
        let mut c = Chat::new(ctx, Id::unique(name), core, name.into(), "no-such-site".into(), std::env::temp_dir(), Arc::new(|| {}));
        c.catalog_stale = false;
        c
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::chat;
    use super::*;
    use serde_json::json;

    fn text(t: &str) -> Value {
        json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": t }] } })
    }

    #[test]
    fn lines_of_an_ended_program_are_ignored() {
        let ctx = egui::Context::default();
        let mut c = chat(&ctx, "generation");
        lock(&c.shared).lines.push((0, text("old")));
        c.poll(&ctx);
        assert_eq!(c.state.items.len(), 1);
        c.new_chat();
        lock(&c.shared).lines.push((0, text("late line of the old program")));
        c.poll(&ctx);
        assert!(c.state.items.is_empty());
        lock(&c.shared).lines.push((1, text("new")));
        c.poll(&ctx);
        assert_eq!(c.state.items.len(), 1);
    }

    #[test]
    fn a_mode_picked_between_turns_survives_the_restart() {
        use zenv_core::studio_chat::PermissionMode;
        let ctx = egui::Context::default();
        let mut c = chat(&ctx, "mode");
        let plan = ChatOptions { mode: Some(PermissionMode::Plan), ..ChatOptions::default() };
        c.options = plan.clone();
        lock(&c.shared).live = Some(conn::Live::running(0, plan));
        lock(&c.shared).lines.push((0, json!({ "type": "system", "subtype": "init", "permissionMode": "plan" })));
        c.poll(&ctx);
        c.options.mode = Some(PermissionMode::Manual);
        c.end();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lock(&c.shared).live.is_some() {
            assert!(std::time::Instant::now() < deadline, "the chat worker never ended the old program");
            std::thread::yield_now();
        }
        lock(&c.shared).live = Some(conn::Live::running(c.generation, c.options.clone()));
        c.poll(&ctx);
        assert_eq!(c.options.mode, Some(PermissionMode::Manual), "the old program's report must not undo the new choice");
        assert_eq!(c.applied().and_then(|o| o.mode), Some(PermissionMode::Manual));
        lock(&c.shared).lines.push((c.generation, json!({ "type": "system", "subtype": "init", "permissionMode": "default" })));
        c.poll(&ctx);
        assert_eq!(c.options.mode, Some(PermissionMode::Manual));
        lock(&c.shared).lines.push((c.generation, json!({ "type": "system", "subtype": "status", "permissionMode": "acceptEdits" })));
        c.poll(&ctx);
        assert_eq!(c.options.mode, Some(PermissionMode::AcceptEdits), "a real change Claude Code reports is still followed");
    }

    #[test]
    fn mcp_status_that_cannot_start_shows_the_error() {
        let ctx = egui::Context::default();
        let mut c = chat(&ctx, "mcp");
        c.act(&ctx, Action::McpLoad);
        let start = Instant::now();
        while c.mcp.error.is_none() {
            assert!(start.elapsed() < Duration::from_secs(10), "no answer from the worker");
            std::thread::sleep(Duration::from_millis(20));
            c.poll(&ctx);
        }
        assert!(c.mcp.servers.is_none() && c.waits.is_empty());
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::tests_support::chat;

    #[test]
    fn opening_studio_starts_no_claude_code_until_the_user_acts() {
        let ctx = eframe::egui::Context::default();
        let mut c = chat(&ctx, "catalog");
        c.catalog_stale = true;
        for _ in 0..5 {
            c.poll(&ctx);
        }
        assert!(c.catalog_job.is_none(), "an idle chat runs nothing in the project folder");
        c.composer.wants_catalog = true;
        c.poll(&ctx);
        assert!(c.catalog_job.is_some(), "opening the model menu or palette asks Claude Code for its list");
        assert!(!c.catalog_stale, "asked once");
    }
}
