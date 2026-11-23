use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Rect, RichText, Sense, Ui, ViewportId};
use zenv_core::app_core::Core;
use zenv_core::studio::{Agent, AgentInfo, EventSink, TerminalEvent};

use super::terminal::Terminal;
use super::{panel_icon_button, Colors};
use crate::widgets::{self, Led};
use crate::{lucide, theme};

pub const BAR_HEIGHT: f32 = 36.0;
const MENU_WIDTH: f32 = 220.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Idle,
    Starting,
    Running,
    Ended,
}

struct State {
    phase: Phase,
    id: Option<String>,
    closed: bool,
}

pub struct Tab {
    pub key: u64,
    pub name: String,
    pub agent: Agent,
    screen: Option<String>,
    pub term: Terminal,
    state: Arc<Mutex<State>>,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|p| p.into_inner())
}

impl Tab {
    pub fn phase(&self) -> Phase {
        lock(&self.state).phase
    }
}

#[derive(Clone)]
pub struct Host {
    pub core: Arc<Core>,
    pub owner: String,
    pub site_id: String,
    pub viewport: ViewportId,
}

pub struct Terminals {
    host: Host,
    pub tabs: Vec<Tab>,
    pub active: u64,
    next: u64,
    pub agents: Vec<AgentInfo>,
    agents_rx: Option<Receiver<Vec<AgentInfo>>>,
    seeded: bool,
}

impl Terminals {
    pub fn new(ctx: &egui::Context, host: Host) -> Self {
        let mut t = Self { host, tabs: Vec::new(), active: 0, next: 0, agents: Vec::new(), agents_rx: None, seeded: false };
        t.refresh_agents(ctx);
        t
    }

    pub fn refresh_agents(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let (core, site, ctx, vp) = (self.host.core.clone(), self.host.site_id.clone(), ctx.clone(), self.host.viewport);
        std::thread::spawn(move || {
            let agents = core.studio_agents(&site).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "studio agents");
                Vec::new()
            });
            let _ = tx.send(agents);
            ctx.request_repaint_of(vp);
        });
        self.agents_rx = Some(rx);
    }

    fn poll_agents(&mut self) {
        let Some(Ok(agents)) = self.agents_rx.as_ref().map(Receiver::try_recv) else { return };
        self.agents = agents;
        self.agents_rx = None;
        if !std::mem::replace(&mut self.seeded, true) && self.tabs.is_empty() {
            let installed = |a: Agent| self.agents.iter().find(|x| x.id == a && x.program.is_some()).map(|x| x.id);
            if let Some(agent) = installed(Agent::ClaudeCode).or(installed(Agent::Shell)) {
                self.add(agent, None);
            }
        }
    }

    pub fn add(&mut self, agent: Agent, screen: Option<String>) {
        let base = match &screen {
            Some(s) => format!("Claude {s}"),
            None => agent.name().to_owned(),
        };
        let same = self.tabs.iter().filter(|t| t.name == base || t.name.starts_with(&format!("{base} "))).count();
        let name = if same > 0 { format!("{base} {}", same + 1) } else { base };
        let key = self.next;
        self.next += 1;
        let term = Terminal::new(Id::unique(("studio-terminal", &self.host.owner, key)));
        self.tabs.push(Tab { key, name, agent, screen, term, state: Arc::new(Mutex::new(State { phase: Phase::Idle, id: None, closed: false })) });
        self.active = key;
    }

    pub fn close(&mut self, key: u64) {
        let Some(i) = self.tabs.iter().position(|t| t.key == key) else { return };
        let tab = self.tabs.remove(i);
        let id = {
            let mut st = lock(&tab.state);
            st.closed = true;
            st.id.take()
        };
        tab.term.out.disconnect();
        if let Some(id) = id {
            let _ = self.host.core.studio.close(&self.host.owner, &id);
        }
        if key == self.active {
            if let Some(last) = self.tabs.last_mut() {
                self.active = last.key;
                last.term.focus = true;
            }
        }
    }

    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        self.tabs.iter().any(|t| t.term.has_focus(ctx))
    }

    pub fn focus(&mut self) {
        let active = self.active;
        if let Some(t) = self.tabs.iter_mut().find(|t| t.key == active) {
            t.term.focus = true;
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, rect: Rect, c: &Colors) -> bool {
        self.poll_agents();
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR_HEIGHT));
        let hide = self.bar(ui, bar, c);
        ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left(), bar.bottom()), vec2(rect.width(), 1.0)), 0.0, c.line);
        let body = Rect::from_min_max(pos2(rect.left(), bar.bottom() + 1.0), rect.max);
        let active = self.active;
        match self.tabs.iter_mut().find(|t| t.key == active) {
            Some(tab) => {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
                child.set_clip_rect(body);
                session_ui(&mut child, body, tab, &self.host);
            }
            None => self.empty(ui, body, c),
        }
        hide
    }

    fn empty(&self, ui: &mut Ui, body: Rect, c: &Colors) {
        let claude = self.agents.iter().find(|a| a.id == Agent::ClaudeCode);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(24.0)).layout(egui::Layout::top_down(egui::Align::Center)));
        child.add_space((body.height() / 2.0 - 40.0).max(0.0));
        child.spacing_mut().item_spacing.y = 8.0;
        child.label(RichText::new("Nothing open.").size(14.0).color(c.muted));
        if let Some(a) = claude.filter(|a| a.program.is_none()) {
            child.label(RichText::new(format!("Claude Code isn't installed: {}", a.install_hint)).font(FontId::new(11.0, FontFamily::Monospace)).color(c.muted));
        }
    }

    fn bar(&mut self, ui: &mut Ui, bar: Rect, c: &Colors) -> bool {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(4.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 0.0;
        let mut activate = None;
        let mut close = None;
        for t in &self.tabs {
            let running = t.phase() == Phase::Running;
            let (open, shut) = tab_button(&mut child, t, t.key == self.active, running, c);
            if open {
                activate = Some(t.key);
            }
            if shut {
                close = Some(t.key);
            }
        }
        child.add_space(4.0);
        let plus = panel_icon_button(&mut child, lucide::PLUS, "New terminal", false, 32.0, 16.0);
        if plus.clicked() {
            self.refresh_agents(ui.ctx());
        }
        let mut add = None;
        egui::Popup::menu(&plus).width(MENU_WIDTH).align(egui::RectAlign::BOTTOM_START).show(|ui| {
            ui.set_max_width(MENU_WIDTH);
            ui.label(RichText::new("Terminal").size(11.0).color(theme::palette(ui).muted));
            for a in &self.agents {
                let hint = match &a.program {
                    Some(p) => p.display().to_string(),
                    None => format!("Not installed. {}", a.install_hint),
                };
                let label = if a.program.is_some() { a.name.to_owned() } else { format!("{}  · not installed", a.name) };
                let r = ui.add_enabled_ui(a.program.is_some(), |ui| widgets::menu_item(ui, lucide::TERMINAL, label, true, false));
                if r.inner {
                    add = Some(a.id);
                }
                r.response.on_hover_text(hint);
            }
        });
        if let Some(agent) = add {
            self.add(agent, None);
        }
        let hide = child
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| panel_icon_button(ui, lucide::CHEVRON_RIGHT, "Hide terminal (Ctrl+`)", false, 28.0, 16.0).clicked())
            .inner;
        if let Some(k) = activate {
            self.active = k;
            self.focus();
        }
        if let Some(k) = close {
            self.close(k);
        }
        hide
    }

    pub fn close_all(&mut self) {
        for k in self.tabs.iter().map(|t| t.key).collect::<Vec<_>>() {
            self.close(k);
        }
        self.host.core.studio.close_owned_by(&self.host.owner);
    }
}

fn tab_button(ui: &mut Ui, t: &Tab, active: bool, running: bool, c: &Colors) -> (bool, bool) {
    let font = FontId::proportional(12.0);
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(t.name.clone(), font.clone(), Color32::WHITE).size().x);
    let w = 12.0 + 6.0 + 8.0 + text_w + 8.0 + 20.0 + 4.0;
    let (rect, r) = ui.allocate_exact_size(vec2(w, BAR_HEIGHT), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, &t.name));
    let close_rect = Rect::from_center_size(pos2(rect.right() - 4.0 - 10.0, rect.center().y), vec2(20.0, 20.0));
    let x = ui.interact(close_rect, r.id.with("close"), Sense::click());
    x.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Close {}", t.name)));
    let p = ui.painter();
    let led = pos2(rect.left() + 12.0 + 3.0, rect.center().y);
    widgets::paint_led(p, led, 3.0, if running { Led::On } else { Led::Off });
    let color = match (active, r.hovered()) {
        (true, _) => Color32::WHITE,
        (false, true) => c.text,
        (false, false) => c.muted,
    };
    p.text(pos2(led.x + 3.0 + 8.0, rect.center().y), Align2::LEFT_CENTER, &t.name, font, color);
    if r.hovered() || x.hovered() {
        if x.hovered() {
            p.rect_filled(close_rect, CornerRadius::same(2), c.raised);
        }
        p.text(close_rect.center(), Align2::CENTER_CENTER, lucide::X, FontId::new(12.0, theme::icons()), c.text);
    }
    if active {
        let line = Rect::from_min_max(pos2(rect.left() + 8.0, rect.bottom() - 2.0), pos2(rect.right() - 8.0, rect.bottom()));
        p.rect_filled(line, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, c.led);
    }
    let x = x.on_hover_text("Close (ends the program)");
    (r.clicked() && !x.clicked(), x.clicked() || r.clicked_by(egui::PointerButton::Middle))
}

fn session_ui(ui: &mut Ui, body: Rect, tab: &mut Tab, host: &Host) {
    if tab.phase() == Phase::Idle {
        let (cols, rows) = tab.term.fit(ui, body);
        tab.term.lock().resize(cols, rows);
        start(tab, host, (cols, rows), ui.ctx());
    }
    let frame = tab.term.ui(ui, body);
    let (phase, id) = {
        let st = lock(&tab.state);
        (st.phase, st.id.clone())
    };
    if let (Some((cols, rows)), Phase::Running, Some(id)) = (frame.resized, phase, &id) {
        let _ = host.core.studio.resize(&host.owner, id, cols, rows);
    }
    if frame.input.is_empty() {
        return;
    }
    match phase {
        Phase::Ended if frame.input.contains(&b'\r') => {
            tab.term.lock().reset();
            let size = tab.term.lock().size();
            start(tab, host, size, ui.ctx());
        }
        Phase::Ended | Phase::Idle => {}
        Phase::Starting | Phase::Running => tab.term.out.send(&frame.input),
    }
}

fn start(tab: &mut Tab, host: &Host, size: (u16, u16), ctx: &egui::Context) {
    lock(&tab.state).phase = Phase::Starting;
    let (screen, out, state, host, ctx) = (tab.term.screen.clone(), tab.term.out.clone(), tab.state.clone(), host.clone(), ctx.clone());
    let (agent, first, name) = (tab.agent, tab.screen.take(), tab.name.clone());
    std::thread::spawn(move || {
        let repaint = {
            let (ctx, vp) = (ctx.clone(), host.viewport);
            move || ctx.request_repaint_of(vp)
        };
        let sink: EventSink = {
            let (screen, out, state, repaint) = (screen.clone(), out.clone(), state.clone(), repaint.clone());
            Arc::new(move |event| {
                let lines = match event {
                    TerminalEvent::Output(bytes) => bytes,
                    TerminalEvent::Exit(code) => {
                        out.disconnect();
                        {
                            let mut st = lock(&state);
                            st.phase = Phase::Ended;
                            st.id = None;
                        }
                        let code = code.map(|c| format!(" (exit code {c})")).unwrap_or_default();
                        format!("\r\n\x1b[90m[{name} ended{code}. Press Enter to start it again.]\x1b[0m\r\n").into_bytes()
                    }
                };
                screen.lock().unwrap_or_else(|p| p.into_inner()).feed(&lines);
                repaint();
            })
        };
        match host.core.studio_open_terminal(&host.owner, &host.site_id, agent, first.as_deref(), size, sink) {
            Ok(id) => {
                let mut st = lock(&state);
                if st.closed {
                    drop(st);
                    let _ = host.core.studio.close(&host.owner, &id);
                } else if st.phase == Phase::Starting {
                    st.phase = Phase::Running;
                    st.id = Some(id.clone());
                    drop(st);
                    let (core, owner, pty) = (host.core.clone(), host.owner.clone(), id.clone());
                    out.connect(Box::new(move |bytes| {
                        let _ = core.studio.write(&owner, &pty, bytes);
                    }));
                    let mut sent = size;
                    loop {
                        let now = screen.lock().unwrap_or_else(|p| p.into_inner()).size();
                        if now == sent {
                            break;
                        }
                        let _ = host.core.studio.resize(&host.owner, &id, now.0, now.1);
                        sent = now;
                    }
                }
            }
            Err(e) => {
                lock(&state).phase = Phase::Ended;
                out.disconnect();
                let text = format!("\x1b[31m{e}\x1b[0m\r\n\x1b[90mPress Enter to try again.\x1b[0m\r\n");
                screen.lock().unwrap_or_else(|p| p.into_inner()).feed(text.as_bytes());
            }
        }
        repaint();
    });
}
