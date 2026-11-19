use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, OpenUrl, RichText};
use zenv_core::app_core::Core;
use zenv_core::runtime_installer::{InstallJob, Phase};
use zenv_core::service_manager::{Health, ServiceStatus};

use crate::loader::{Note, Tasks, Tone};
use crate::pages::{Action, Cx, Pages, Route, Shared};
use crate::theme::{self, palette};
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
    pub fn new(cc: &eframe::CreationContext, core: Arc<Core>) -> Self {
        theme::install(&cc.egui_ctx);
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
        }
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

impl eframe::App for ZenvApp {
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
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(ROUTE_KEY, format!("{:?}", self.route));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.core.shutdown();
    }
}
