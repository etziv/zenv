use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, OpenUrl, RichText, ViewportBuilder, ViewportCommand, ViewportId};
use zenv_core::app_core::Core;
use zenv_core::platform::{self, InstanceEvent};
use zenv_core::runtime_installer::{InstallJob, Phase};
use zenv_core::service_manager::{Health, ServiceStatus};

use crate::loader::{Note, Tasks, Tone};
use crate::pages::{Action, Cx, Pages, Route, Shared};
use crate::panel::{self, Out};
use crate::theme::{self, palette};
use crate::tray::{self, Msg};
use crate::{shell, widgets};

const ROUTE_KEY: &str = "zenv.route";

pub struct ZenvApp {
    core: Arc<Core>,
    route: Route,
    shared: Shared,
    pages: Pages,
    tasks: Tasks,
    toasts: Vec<Toast>,
    version: &'static str,
    last_health: HashMap<String, Health>,
    notes: (Sender<Note>, Receiver<Note>),
    seen_jobs: Option<HashMap<String, Phase>>,
    shown_route: Option<Route>,
    tray: Option<tray_icon::TrayIcon>,
    msgs: (Sender<Msg>, Receiver<Msg>),
    panel: Option<PanelState>,
    panel_closed_at: Option<Instant>,
    quitting: bool,
    studios: crate::studio::Windows,
}

struct PanelState {
    icon: egui::Rect,
    height: f32,
    scale: f32,
    styled: bool,
    feed: Arc<Mutex<Feed>>,
}

#[derive(Default)]
struct Feed {
    services: Option<Vec<ServiceStatus>>,
    version: &'static str,
    busy: Vec<String>,
    error: Option<(String, String)>,
    outs: Vec<Out>,
    height: f32,
    scale: f32,
    was_focused: bool,
    blurred: bool,
    escape: bool,
}

pub struct Toast {
    pub tone: Tone,
    pub title: String,
    pub description: String,
    pub action: Option<(&'static str, Route)>,
    pub at: Instant,
}

pub fn crash_toasts(last: &mut HashMap<String, Health>, services: &[ServiceStatus]) -> Vec<Toast> {
    let mut out = Vec::new();
    for s in services {
        let before = last.insert(s.service.id.clone(), s.health);
        if before == Some(Health::Running) && s.health == Health::Failed {
            out.push(Toast {
                tone: Tone::Error,
                title: format!("{} stopped unexpectedly", s.service.name),
                description: "It exited without Zenv asking it to. Its log usually says why.".into(),
                action: Some(("View logs", Route::Logs)),
                at: Instant::now(),
            });
        }
    }
    out
}

pub fn install_notes(seen: &mut Option<HashMap<String, Phase>>, jobs: &[InstallJob]) -> Vec<Note> {
    let Some(seen) = seen else {
        *seen = Some(jobs.iter().map(|j| (j.id.clone(), j.phase)).collect());
        return Vec::new();
    };
    let mut notes = Vec::new();
    for j in jobs {
        let before = seen.insert(j.id.clone(), j.phase);
        let ended = matches!(j.phase, Phase::Done | Phase::Failed);
        if before != Some(j.phase) && ended {
            let name = format!("{} {}", zenv_core::runtime_catalog::info(j.component).name, j.version);
            notes.push(match j.phase {
                Phase::Done => Note::new(Tone::Success, format!("{name} is installed"), ""),
                _ => Note::new(Tone::Error, format!("{name} was not installed"), j.error.clone().unwrap_or_default()),
            });
        }
    }
    notes
}

impl ZenvApp {
    pub fn new(cc: &eframe::CreationContext, core: Arc<Core>, instance: Option<InstanceEvent>) -> Self {
        theme::install(&cc.egui_ctx);
        let msgs = channel();
        let tray = tray::install(&cc.egui_ctx, msgs.0.clone(), &tray::tooltip(&core.services.statuses().unwrap_or_default()))
            .inspect_err(|e| tracing::warn!(error = %e, "no tray icon: closing the window quits Zenv"))
            .ok();
        watch_tooltip(&cc.egui_ctx, &core, msgs.0.clone());
        if let Some(instance) = instance {
            let (ctx, tx) = (cc.egui_ctx.clone(), msgs.0.clone());
            std::thread::Builder::new()
                .name("second-launch".into())
                .spawn(move || {
                    while instance.wait() {
                        let _ = tx.send(Msg::ShowMain);
                        ctx.request_repaint();
                    }
                })
                .ok();
        }
        let route = cc
            .storage
            .and_then(|s| s.get_string(ROUTE_KEY))
            .and_then(|r| crate::pages::route_from_str(&r))
            .unwrap_or(Route::Dashboard);
        let version = core.info().version;
        Self {
            core,
            route,
            shared: Shared::default(),
            pages: Pages::default(),
            tasks: Tasks::default(),
            toasts: vec![],
            version,
            last_health: HashMap::new(),
            notes: channel(),
            seen_jobs: None,
            shown_route: None,
            tray,
            msgs,
            panel: None,
            panel_closed_at: None,
            quitting: false,
            studios: crate::studio::Windows::default(),
        }
    }

    fn show_main(&mut self, ctx: &egui::Context) {
        self.close_panel();
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn quit(&mut self, ctx: &egui::Context) {
        if self.studios.ask_quit(ctx) {
            self.close_panel();
            return;
        }
        self.quitting = true;
        self.panel = None;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn close_panel(&mut self) {
        if self.panel.take().is_some() {
            self.panel_closed_at = Some(Instant::now());
        }
    }

    fn toggle_panel(&mut self, ctx: &egui::Context, icon: egui::Rect) {
        if self.panel.is_some() {
            self.close_panel();
            return;
        }
        if self.panel_closed_at.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return;
        }
        let scale = ctx.native_pixels_per_point().unwrap_or(1.0) * ctx.zoom_factor();
        self.panel = Some(PanelState { icon, height: 320.0, scale, styled: false, feed: Arc::default() });
        ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
        ctx.request_repaint();
    }

    fn panel(&mut self, ctx: &egui::Context) {
        let Some(state) = &mut self.panel else { return };
        let (outs, close) = {
            let mut feed = state.feed.lock().unwrap_or_else(|p| p.into_inner());
            feed.services = self.shared.services.ok().cloned();
            feed.version = self.version;
            feed.busy = self.tasks.busy_keys();
            feed.error = self.toasts.iter().rev().find(|t| t.tone == Tone::Error).map(|t| (t.title.clone(), t.description.clone()));
            if feed.height > 0.0 {
                state.height = feed.height.clamp(panel::HEIGHT.0, panel::HEIGHT.1).ceil();
                state.scale = feed.scale;
            }
            let close = std::mem::take(&mut feed.blurred) || std::mem::take(&mut feed.escape);
            (std::mem::take(&mut feed.outs), close)
        };
        if !state.styled {
            if let Some(hwnd) = platform::find_window(panel::TITLE) {
                platform::style_popup(hwnd, 0x2b2f32);
                state.styled = true;
            }
        }
        for out in outs {
            match out {
                Out::Act(action) => self.apply(ctx, vec![action]),
                Out::OpenMain => self.show_main(ctx),
                Out::Quit => self.quit(ctx),
                Out::Dismiss => {
                    if let Some(i) = self.toasts.iter().rposition(|t| t.tone == Tone::Error) {
                        self.toasts.remove(i);
                    }
                }
            }
        }
        if close {
            self.close_panel();
        }
        let Some(state) = &self.panel else { return };
        let size = egui::vec2(panel::WIDTH, state.height) * state.scale;
        let c = state.icon.center();
        let work = platform::work_area(c.x as i32, c.y as i32).map_or(egui::Rect::from_center_size(c, egui::vec2(4096.0, 4096.0)), |[l, t, r, b]| {
            egui::Rect::from_min_max(egui::pos2(l as f32, t as f32), egui::pos2(r as f32, b as f32))
        });
        let pos = tray::place(state.icon, work, size, state.scale);
        let builder = ViewportBuilder::default()
            .with_title(panel::TITLE)
            .with_inner_size([panel::WIDTH, state.height])
            .with_position(pos / state.scale)
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(true);
        let (id, feed) = (ViewportId::from_hash_of("tray-panel"), state.feed.clone());
        ctx.request_repaint_of(id);
        ctx.show_viewport_deferred(id, builder, move |ui, _| {
            let mut f = feed.lock().unwrap_or_else(|p| p.into_inner());
            let (outs, height) = {
                let error = f.error.as_ref().map(|(t, d)| (t.as_str(), d.as_str()));
                panel::ui(ui, &panel::View { services: f.services.as_deref(), version: f.version, busy: &f.busy, error })
            };
            let (focused, escape) = ui.input(|i| (i.viewport().focused, i.key_pressed(egui::Key::Escape)));
            f.was_focused |= focused == Some(true);
            let blurred = f.was_focused && focused == Some(false);
            let wake = !outs.is_empty() || escape || blurred || (height - f.height).abs() > 0.5;
            f.outs.extend(outs);
            f.height = height;
            f.scale = ui.ctx().pixels_per_point();
            f.blurred |= blurred;
            f.escape |= escape;
            if wake {
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
        });
    }

    fn apply(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Navigate(r) => self.route = r,
                Action::OpenUrl(url) => ctx.open_url(OpenUrl::new_tab(url)),
                Action::OpenRouter(id, page) => {
                    let name = self.shared.services.ok().and_then(|s| s.iter().find(|s| s.service.id == id)).map_or_else(|| id.clone(), |s| s.service.name.clone());
                    let (ctx, core, notes) = (ctx.clone(), self.core.clone(), self.notes.0.clone());
                    std::thread::spawn(move || match core.router_url(&id, page.as_deref()) {
                        Ok(url) => ctx.open_url(OpenUrl::new_tab(url)),
                        Err(e) => {
                            let _ = notes.send(Note::new(Tone::Error, format!("Couldn't open {name}"), e.to_string()));
                            ctx.request_repaint();
                        }
                    });
                }
                Action::Toast(note) => self.toast(note),
                Action::OpenStudio(site) => drop(self.studios.open(ctx, &self.core, site)),
                Action::Run(task) => {
                    self.shared.errors.remove(&task.key);
                    self.tasks.start(ctx, &self.core, task)
                }
            }
        }
    }

    fn toast(&mut self, note: Note) {
        self.toasts.push(Toast { tone: note.tone, title: note.title, description: note.description, action: None, at: Instant::now() });
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(8));
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        let mut dismissed = None;
        let mut go = None;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 74.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(384.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, t) in self.toasts.iter().enumerate() {
                    let p = palette(ui);
                    let frame = egui::Frame::new()
                        .fill(p.popover)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(theme::RADIUS + 2))
                        .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: egui::Color32::from_black_alpha(60) })
                        .inner_margin(egui::Margin { left: 14, right: 12, top: 12, bottom: 12 })
                        .show(ui, |ui| {
                            ui.set_width(384.0 - 26.0);
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 12.0;
                                let (glyph, tint) = tone_style(t.tone, p);
                                ui.label(widgets::icon(glyph, 16.0).color(tint));
                                ui.vertical(|ui| {
                                    ui.set_width(384.0 - 26.0 - 16.0 - 12.0 - 32.0);
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(widgets::medium(&t.title, 14.0));
                                    if !t.description.is_empty() {
                                        ui.label(RichText::new(&t.description).size(12.0).color(p.muted));
                                    }
                                    if let Some((label, route)) = t.action {
                                        ui.add_space(4.0);
                                        let r = ui.add(egui::Label::new(widgets::medium(label, 12.0).color(p.primary)).sense(egui::Sense::click()));
                                        if r.clicked() {
                                            go = Some((i, route));
                                        }
                                    }
                                });
                                if widgets::icon_button(ui, crate::lucide::X, "Dismiss").clicked() {
                                    dismissed = Some(i);
                                }
                            });
                        });
                    let r = frame.response.rect;
                    let bar = egui::Rect::from_min_max(r.left_top(), egui::pos2(r.left() + 3.0, r.bottom()));
                    ui.painter().rect_filled(bar, egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, tone_style(t.tone, p).1);
                }
            });
        if let Some((i, route)) = go {
            self.route = route;
            self.toasts.remove(i);
        } else if let Some(i) = dismissed {
            self.toasts.remove(i);
        }
    }
}

fn tone_style(tone: Tone, p: &theme::Palette) -> (&'static str, egui::Color32) {
    match tone {
        Tone::Error => (crate::lucide::CIRCLE_ALERT, p.destructive),
        Tone::Success => (crate::lucide::CIRCLE_CHECK, p.success),
        Tone::Info => (crate::lucide::INFO, p.primary),
    }
}

fn watch_tooltip(ctx: &egui::Context, core: &Arc<Core>, tx: Sender<Msg>) {
    let (ctx, core) = (ctx.clone(), core.clone());
    std::thread::Builder::new()
        .name("tray-tooltip".into())
        .spawn(move || {
            let mut last = String::new();
            loop {
                std::thread::sleep(Duration::from_secs(3));
                let text = tray::tooltip(&core.services.statuses().unwrap_or_default());
                if text != last {
                    last.clone_from(&text);
                    if tx.send(Msg::Tooltip(text)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        })
        .ok();
}

impl eframe::App for ZenvApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.msgs.1.try_recv() {
            match msg {
                Msg::ShowMain => self.show_main(ctx),
                Msg::Quit => self.quit(ctx),
                Msg::TogglePanel(icon) => self.toggle_panel(ctx, icon),
                Msg::Tooltip(text) => {
                    if let Some(tray) = &self.tray {
                        let _ = tray.set_tooltip(Some(text));
                    }
                }
            }
        }
        if self.studios.take_quit() && !self.quitting {
            self.quit(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.tray.is_some() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            } else if self.studios.ask_quit(ctx) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut finished = false;
        for done in self.tasks.finished() {
            finished = true;
            match done.result {
                Ok(notes) => {
                    self.shared.errors.remove(&done.key);
                    notes.into_iter().for_each(|n| self.toast(n));
                }
                Err(e) if done.inline => {
                    self.shared.errors.insert(done.key, e);
                }
                Err(e) => self.toast(Note::new(Tone::Error, done.label, e)),
            }
        }
        while let Ok(note) = self.notes.1.try_recv() {
            self.toast(note);
        }
        if finished {
            self.shared.services.invalidate();
        }
        self.shared.services.refresh_every(Duration::from_secs(2), &ctx, &self.core, |c| c.services.statuses());
        let installing = self.shared.jobs.ok().is_some_and(|j| j.iter().any(|j| !matches!(j.phase, zenv_core::runtime_installer::Phase::Done | zenv_core::runtime_installer::Phase::Failed)));
        let every = if installing { Duration::from_millis(800) } else { Duration::from_secs(3) };
        self.shared.jobs.refresh_every(every, &ctx, &self.core, |c| Ok(c.installer.jobs()));
        if let Some(jobs) = self.shared.jobs.ok() {
            let notes = install_notes(&mut self.seen_jobs, jobs);
            finished |= !notes.is_empty();
            notes.into_iter().for_each(|n| self.toast(n));
        }
        if let Some(services) = self.shared.services.ok() {
            let crashed = crash_toasts(&mut self.last_health, services);
            self.toasts.extend(crashed);
        }

        let busy = self.tasks.busy_keys();
        let version = self.version;
        let services = self.shared.services.ok().map(Vec::as_slice);
        let entered = self.shown_route.replace(self.route) != Some(self.route);
        let finished = finished || entered;
        let (core, shared, pages, route) = (&self.core, &self.shared, &mut self.pages, self.route);
        let jobs = shared.jobs.ok().map(Vec::as_slice).unwrap_or_default();
        let indicator = |ui: &mut egui::Ui, out: &mut Vec<Action>| {
            if let Some(component) = crate::pages::components::install_indicator(ui, jobs) {
                let category = zenv_core::runtime_catalog::catalog().iter().find(|c| c.id == component).map(|c| c.category);
                out.push(Action::Navigate(category.map_or(Route::Languages, shell::page_for)));
            }
        };
        let actions = shell::frame_with(ui, route, version, services, indicator, |ui, out| {
            let mut cx = Cx { core, shared, busy: &busy, tasks_finished: finished, actions: Vec::new() };
            pages.ui(route, ui, &mut cx);
            out.append(&mut cx.actions);
        });
        self.toasts(&ctx);
        self.apply(&ctx, actions);
        self.panel(&ctx);
        self.studios.show(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(ROUTE_KEY, format!("{:?}", self.route));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.core.shutdown();
    }
}
