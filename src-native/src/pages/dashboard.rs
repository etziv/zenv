use std::cmp::Ordering;
use std::time::Duration;

use eframe::egui::{self, RichText, Ui};
use zenv_core::app_services::{NeededService, StartOutcome};
use zenv_core::diagnostics::{Check, CheckStatus};
use zenv_core::runtime_catalog::{self, Category};
use zenv_core::runtime_manager::{self, Installation};
use zenv_core::service_manager::{Health, ServiceStatus};

use super::{Action, Cx, Route};
use crate::loader::{Loader, Note, Task, Tone};
use crate::lucide;
use crate::theme::palette;
use crate::widgets::{self, Kind, Led, SwitchState};

#[derive(Default)]
pub struct DashboardPage {
    installations: Loader<Vec<Installation>>,
    needed: Loader<Vec<NeededService>>,
    diagnostics: Loader<Vec<Check>>,
}

pub struct Data<'a> {
    pub services: &'a Loader<Vec<ServiceStatus>>,
    pub installations: &'a Loader<Vec<Installation>>,
    pub needed: &'a Loader<Vec<NeededService>>,
    pub diagnostics: &'a Loader<Vec<Check>>,
    pub errors: &'a std::collections::HashMap<String, String>,
}

impl DashboardPage {
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        if cx.tasks_finished {
            self.installations.invalidate();
            self.needed.invalidate();
        }
        let ctx = ui.ctx().clone();
        self.installations.refresh_every(Duration::from_secs(30), &ctx, cx.core, |c| runtime_manager::list(&c.db, None));
        self.needed.refresh_every(Duration::from_secs(5), &ctx, cx.core, |c| c.services_for_apps());
        self.diagnostics.refresh_every(Duration::from_secs(300), &ctx, cx.core, zenv_core::diagnostics::run);
        let data = Data {
            services: &cx.shared.services,
            installations: &self.installations,
            needed: &self.needed,
            diagnostics: &self.diagnostics,
            errors: &cx.shared.errors,
        };
        view(ui, &data, cx.busy, &mut cx.actions);
    }
}

pub const GROUPS: [(Category, &str, Route, &str); 4] = [
    (Category::WebServer, "Web servers", Route::WebServers, "Add Apache or Nginx"),
    (Category::Database, "Databases", Route::Databases, "Add MySQL or PostgreSQL"),
    (Category::Cache, "Cache & queue", Route::Cache, "Add Redis"),
    (Category::Tool, "Tools", Route::Tools, "Add 9router or OmniRoute"),
];

pub fn view(ui: &mut Ui, d: &Data, busy: &[String], out: &mut Vec<Action>) {
    let (services, installations) = match (d.services.get(), d.installations.get()) {
        (Some(Err(e)), _) | (_, Some(Err(e))) => return widgets::error_panel(ui, e),
        (Some(Ok(s)), Some(Ok(i))) => (s, i),
        _ => return widgets::loading(ui, "Loading…"),
    };
    let running = services.iter().filter(|s| s.health == Health::Running).count();
    let meta = if services.is_empty() {
        "No services yet".to_owned()
    } else {
        format!("{running} of {} services running", services.len())
    };
    let needed = d.needed.ok().map(Vec::as_slice).unwrap_or_default();
    widgets::page_header(ui, "Dashboard", &meta, |ui| {
        start_apps_button(ui, needed, busy, out);
        stop_all_button(ui, running, busy, out);
    });

    let window = ui.ctx().content_rect().width();
    for (kind, title, route, empty) in GROUPS {
        let members = sorted(services.iter().filter(|s| s.kind == kind).collect());
        widgets::section(ui, title, |ui| {
            if members.is_empty() {
                if widgets::button(ui, Kind::Dashed, Some(lucide::PLUS), empty, true).clicked() {
                    out.push(Action::Navigate(route));
                }
                return;
            }
            let cols = widgets::grid_columns(window, &[(0.0, 1), (640.0, 2), (1280.0, 3)]);
            grid(ui, cols, &members, |ui, s| service_tile(ui, s, services, busy, d.errors, out));
        });
    }

    widgets::section(ui, "Languages", |ui| languages(ui, installations, window, out));
    widgets::section(ui, "Health", |ui| health(ui, d.diagnostics, out));
}

pub fn grid<T>(ui: &mut Ui, cols: usize, items: &[T], mut cell: impl FnMut(&mut Ui, &T)) {
    let gap = 12.0;
    let width = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    for row in items.chunks(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for item in row {
                ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(width);
                    cell(ui, item);
                });
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
}

pub fn service_key(id: &str) -> String {
    format!("service:{id}")
}

pub const START_APPS: &str = "apps:start";
pub const STOP_ALL: &str = "services:stop-all";

pub fn start_apps_task() -> Task {
    Task::reporting(START_APPS, "Couldn't start the apps' services", |c| Ok(start_notes(&c.start_services_for_apps()?)))
}

pub fn stop_all_task() -> Task {
    Task::reporting(STOP_ALL, "Couldn't stop the services", |c| Ok(stop_notes(&c.stop_all_services()?)))
}

pub fn switch_task(id: &str, label: String, running: bool) -> Task {
    let id = id.to_owned();
    if running {
        Task::new(service_key(&id), label, move |c| c.stop_service(&id).map(|_| ()))
    } else {
        Task::new(service_key(&id), label, move |c| c.start_service(&id).map(|_| ()))
    }
}

fn start_apps_button(ui: &mut Ui, needed: &[NeededService], busy: &[String], out: &mut Vec<Action>) {
    let pending = busy.iter().any(|k| k == START_APPS);
    let label = start_apps_label(needed, pending);
    let r = widgets::button(ui, Kind::Primary, Some(lucide::PLAY), &label, !pending && !needed.is_empty());
    if r.clicked() {
        out.push(Action::Run(start_apps_task()));
    }
    if !needed.is_empty() {
        r.on_hover_ui(|ui| {
            ui.label(RichText::new("Used by your apps").small().color(palette(ui).muted));
            for n in needed {
                ui.horizontal(|ui| {
                    widgets::led(ui, if n.running { Led::On } else { Led::Off }, 6.0);
                    ui.label(widgets::strong(&n.name, 12.0));
                    let apps = app_count(&n.used_by);
                    ui.label(widgets::mono(format!("{apps} app{}", if apps == 1 { "" } else { "s" }), 11.0).color(palette(ui).muted));
                });
            }
        });
    }
}

fn failed_notes(outcomes: &[StartOutcome], verb: &str) -> Vec<Note> {
    outcomes
        .iter()
        .filter(|o| o.result == "failed")
        .map(|o| Note::new(Tone::Error, format!("Couldn't {verb} {}", o.name), o.error.clone().unwrap_or_default()))
        .collect()
}

pub fn start_notes(outcomes: &[StartOutcome]) -> Vec<Note> {
    let mut notes = failed_notes(outcomes, "start");
    let started: Vec<&str> = outcomes.iter().filter(|o| o.result == "started").map(|o| o.name.as_str()).collect();
    if !started.is_empty() {
        notes.push(Note::new(Tone::Success, format!("Started {}", started.join(", ")), ""));
    } else if notes.is_empty() {
        let title = if outcomes.is_empty() { "No app needs a service yet" } else { "Everything your apps need is already running" };
        notes.push(Note::new(Tone::Info, title, ""));
    }
    notes
}

pub fn stop_notes(outcomes: &[StartOutcome]) -> Vec<Note> {
    let mut notes = failed_notes(outcomes, "stop");
    let stopped = outcomes.iter().filter(|o| o.result == "stopped").count();
    if stopped > 0 {
        notes.push(Note::new(Tone::Success, format!("Stopped {stopped} service{}", if stopped == 1 { "" } else { "s" }), ""));
    }
    notes
}

pub fn start_apps_label(needed: &[NeededService], pending: bool) -> String {
    let stopped = needed.iter().filter(|n| !n.running).count();
    match (pending, stopped) {
        (true, _) => "Starting…".into(),
        (false, 0) => "Apps running".into(),
        (false, n) => format!("Start apps ({n})"),
    }
}

fn app_count(used_by: &[String]) -> usize {
    let mut apps: Vec<&str> = used_by.iter().map(|u| u.split(' ').next().unwrap_or(u)).collect();
    apps.sort_unstable();
    apps.dedup();
    apps.len()
}

fn stop_all_button(ui: &mut Ui, running: usize, busy: &[String], out: &mut Vec<Action>) {
    let pending = busy.iter().any(|k| k == STOP_ALL);
    if running == 0 && !pending {
        return;
    }
    let label = if pending { "Stopping…".to_owned() } else { format!("Stop all ({running})") };
    if widgets::button(ui, Kind::Outline, Some(lucide::SQUARE), &label, !pending).clicked() {
        out.push(Action::Run(stop_all_task()));
    }
}

fn service_tile(
    ui: &mut Ui,
    status: &ServiceStatus,
    all: &[ServiceStatus],
    busy: &[String],
    errors: &std::collections::HashMap<String, String>,
    out: &mut Vec<Action>,
) {
    let p = palette(ui);
    let s = &status.service;
    let running = status.health == Health::Running;
    let failed = status.health == Health::Failed;
    let key = service_key(&s.id);
    let pending = busy.contains(&key);
    let border = if running {
        p.primary.gamma_multiply(0.35)
    } else if failed {
        p.destructive.gamma_multiply(0.4)
    } else {
        p.border
    };
    let frame = widgets::card(ui, border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            widgets::pulse(ui, status.health);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if running { format!("Stop {}", s.name) } else { format!("Start {}", s.name) };
                let mut r = widgets::status_switch(ui, &SwitchState { on: running, failed, pending }, &label);
                let sharing = shares_port_with(status, all);
                if !running && !sharing.is_empty() {
                    r = r.on_hover_text(format!(
                        "Shares :{} with {}: both run, each serving its own apps",
                        s.http_port,
                        sharing.join(", ")
                    ));
                }
                if r.clicked() {
                    out.push(Action::Run(switch_task(&s.id, label, running).inline()));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(widgets::strong(&s.name, 14.0)).truncate());
                });
            });
        });
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            let version = status.installation.as_ref().map(|i| format!(" {}", i.version)).unwrap_or_default();
            ui.label(widgets::mono(format!("{}{version}", s.component), 11.0).color(p.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if running && status.kind == Category::WebServer {
                    if widgets::icon_button(ui, lucide::EXTERNAL_LINK, &format!("Open {}", status.url)).clicked() {
                        out.push(Action::OpenUrl(status.url.clone()));
                    }
                }
                if running && status.kind == Category::Tool {
                    if widgets::icon_button(ui, lucide::EXTERNAL_LINK, &format!("Open the {} dashboard", s.name)).clicked() {
                        out.push(Action::OpenRouter(s.id.clone(), None));
                    }
                }
                port_label(ui, status);
            });
        });
        if let Some(e) = errors.get(&key) {
            widgets::error_panel(ui, e);
        }
    });
    let r = frame.response.rect;
    let stripe = if running { Some(p.primary) } else if failed { Some(p.destructive) } else { None };
    if let Some(c) = stripe {
        let bar = egui::Rect::from_min_max(r.left_top() + egui::vec2(0.5, 0.5), egui::pos2(r.left() + 3.5, r.bottom() - 0.5));
        ui.painter().rect_filled(bar, egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, c);
    }
}

fn port_label(ui: &mut Ui, status: &ServiceStatus) {
    let p = palette(ui);
    let text = port_text(status);
    let r = match text.split_once(" via ") {
        Some((port, front)) => {
            let mut job = egui::text::LayoutJob::default();
            let font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
            job.append(port, 0.0, egui::TextFormat::simple(font.clone(), p.foreground));
            job.append(&format!(" via {front}"), 0.0, egui::TextFormat::simple(font, p.muted));
            ui.label(job)
        }
        None => ui.label(widgets::mono(&text, 11.0).color(p.foreground)),
    };
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
}

pub fn led_of(health: Health) -> Led {
    match health {
        Health::Running => Led::On,
        Health::Failed => Led::Fail,
        Health::Stopped => Led::Off,
    }
}

pub fn port_text(status: &ServiceStatus) -> String {
    let s = &status.service;
    let mut t = format!(":{}", s.http_port);
    match (&status.behind, s.https_port) {
        (Some(front), _) => t.push_str(&format!(" via {front}")),
        (None, Some(https)) if status.kind == Category::WebServer => t.push_str(&format!(" · :{https}")),
        _ => {}
    }
    t
}

pub fn shares_port_with(status: &ServiceStatus, all: &[ServiceStatus]) -> Vec<String> {
    if status.kind != Category::WebServer {
        return vec![];
    }
    all.iter()
        .filter(|o| {
            o.kind == Category::WebServer && o.service.id != status.service.id && o.service.http_port == status.service.http_port
        })
        .map(|o| o.service.name.clone())
        .collect()
}

pub fn compare_versions_desc(a: &str, b: &str) -> Ordering {
    natural(b, a)
}

fn natural(a: &str, b: &str) -> Ordering {
    let chunks = |s: &str| -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for ch in s.chars() {
            let digit = ch.is_ascii_digit();
            match out.last_mut() {
                Some((d, buf)) if *d == digit => buf.push(ch),
                _ => out.push((digit, ch.to_string())),
            }
        }
        out
    };
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(&cb) {
        let o = match (x, y) {
            ((true, n), (true, m)) => {
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                n.len().cmp(&m.len()).then_with(|| n.cmp(m))
            }
            ((_, n), (_, m)) => n.cmp(m),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len())
}

pub fn sorted(mut list: Vec<&ServiceStatus>) -> Vec<&ServiceStatus> {
    list.sort_by(|a, b| {
        a.service.component.as_str().cmp(b.service.component.as_str())
            .then_with(|| {
                let v = |s: &ServiceStatus| s.installation.as_ref().map(|i| i.version.clone()).unwrap_or_default();
                compare_versions_desc(&v(a), &v(b))
            })
            .then_with(|| a.service.name.cmp(&b.service.name))
    });
    list
}

fn languages(ui: &mut Ui, installed: &[Installation], window: f32, out: &mut Vec<Action>) {
    let langs: Vec<_> = runtime_catalog::catalog().iter().filter(|c| c.category == Category::Language).collect();
    let cols = widgets::grid_columns(window, &[(0.0, 2), (768.0, 3), (1280.0, 5)]);
    grid(ui, cols, &langs, |ui, lang| {
        let mut versions: Vec<_> = installed.iter().filter(|i| i.component == lang.id).collect();
        versions.sort_by(|a, b| compare_versions_desc(&a.version, &b.version));
        let p = palette(ui);
        let frame = egui::Frame::new()
            .fill(p.card)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS + 2))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    widgets::component_logo(ui, lang.id, !versions.is_empty(), None, 32.0);
                    ui.label(widgets::display(lang.name, 16.0));
                });
                if versions.is_empty() {
                    ui.label(RichText::new("Not installed · add one").size(12.0).color(p.muted));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for v in &versions {
                            widgets::chip(ui, &v.version, v.is_default)
                                .on_hover_text(format!("{}{}", v.root_path.display(), if v.is_default { " (default)" } else { "" }));
                        }
                    });
                }
            });
        let r = frame.response.interact(egui::Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, lang.name));
        if r.hovered() {
            ui.painter().rect_stroke(
                r.rect,
                egui::CornerRadius::same(crate::theme::RADIUS + 2),
                egui::Stroke::new(1.0, p.primary.gamma_multiply(0.4)),
                egui::StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            out.push(Action::Navigate(Route::Languages));
        }
    });
}

fn health(ui: &mut Ui, diagnostics: &Loader<Vec<Check>>, out: &mut Vec<Action>) {
    match diagnostics.get() {
        None => widgets::loading(ui, "Running checks…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(checks)) => {
            let p = palette(ui);
            widgets::card(ui, p.border).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(health_text(checks)).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button_sm(ui, Kind::Outline, None, "View diagnostics", true).clicked() {
                            out.push(Action::Navigate(Route::Logs));
                        }
                    });
                });
            });
        }
    }
}

pub fn health_text(checks: &[Check]) -> String {
    let problems = checks.iter().filter(|c| c.status != CheckStatus::Ok).count();
    if problems == 0 {
        format!("All {} checks passed.", checks.len())
    } else {
        format!("{problems} of {} checks need attention.", checks.len())
    }
}
