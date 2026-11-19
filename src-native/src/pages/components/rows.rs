use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, CornerRadius, Margin, RichText, Stroke, Ui};
use zenv_core::app_core::Core;
use zenv_core::log_manager::LogTail;
use zenv_core::service_manager::{LogSource, ServiceEvent};

use crate::loader::Loader;
use crate::theme::{self, palette};
use crate::widgets::{self, Kind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Success(String),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Notices(Arc<Mutex<HashMap<String, Notice>>>);

impl Notices {
    pub fn set(&self, service_id: &str, notice: Notice) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).insert(service_id.to_owned(), notice);
    }

    pub fn clear(&self, service_id: &str) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(service_id);
    }

    pub fn get(&self, service_id: &str) -> Option<Notice> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).get(service_id).cloned()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Logs,
    Activity,
}

pub struct Details {
    pub tab: Tab,
    pub picked: String,
    pub follow: bool,
    sources: Loader<Vec<LogSource>>,
    tail: Loader<LogTail>,
    tail_for: String,
    events: Loader<Vec<ServiceEvent>>,
}

impl Default for Details {
    fn default() -> Self {
        Self {
            tab: Tab::Logs,
            picked: "error".to_owned(),
            follow: true,
            sources: Loader::default(),
            tail: Loader::default(),
            tail_for: String::new(),
            events: Loader::default(),
        }
    }
}

#[derive(Default)]
pub struct Rows {
    pub notices: Notices,
    pub details: HashMap<String, Details>,
}

impl Rows {
    pub fn toggle_details(&mut self, service_id: &str) {
        if self.details.remove(service_id).is_none() {
            self.details.insert(service_id.to_owned(), Details::default());
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context, core: &Arc<Core>) {
        for (id, d) in &mut self.details {
            let sid = id.clone();
            d.sources.refresh_every(Duration::from_secs(10), ctx, core, move |c| c.services.log_sources(&sid));
            if d.tail_for != d.picked {
                d.tail_for = d.picked.clone();
                d.tail = Loader::default();
            }
            let (sid, kind) = (id.clone(), d.picked.clone());
            let every = Duration::from_secs(if d.follow { 4 } else { 30 });
            d.tail.refresh_every(every, ctx, core, move |c| c.services.read_log(&sid, &kind, 300));
            if d.tab == Tab::Activity {
                let sid = id.clone();
                d.events.refresh_every(Duration::from_secs(10), ctx, core, move |c| c.services.events(&sid, 50));
            }
        }
    }
}

pub fn details_panel(ui: &mut Ui, d: &mut Details) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        for (tab, label) in [(Tab::Logs, "Logs"), (Tab::Activity, "Activity")] {
            let kind = if d.tab == tab { Kind::Outline } else { Kind::Ghost };
            if widgets::button_sm(ui, kind, None, label, true).clicked() {
                d.tab = tab;
            }
        }
    });
    ui.add_space(6.0);
    match d.tab {
        Tab::Logs => logs(ui, d),
        Tab::Activity => match d.events.get() {
            None => widgets::loading(ui, "Loading…"),
            Some(Err(e)) => widgets::error_panel(ui, e),
            Some(Ok(events)) if events.is_empty() => {
                ui.label(RichText::new("No activity recorded yet.").small().color(p.muted));
            }
            Some(Ok(events)) => {
                egui::ScrollArea::vertical().id_salt("service-events").max_height(288.0).show(ui, |ui| {
                    for e in events {
                        ui.horizontal_top(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 16.0), egui::Sense::hover());
                            ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, zenv_core::util::relative_to_now(&e.created_at), egui::FontId::proportional(12.0), p.muted);
                            let color = match e.level.as_str() {
                                "error" => p.destructive,
                                "warning" => p.warning,
                                _ => p.foreground,
                            };
                            ui.add(egui::Label::new(RichText::new(&e.message).size(12.0).color(color)).wrap());
                        });
                    }
                });
            }
        },
    }
}

fn logs(ui: &mut Ui, d: &mut Details) {
    let p = palette(ui);
    ui.horizontal_wrapped(|ui| {
        if let Some(sources) = d.sources.ok() {
            for s in sources {
                let kind = if s.id == d.picked { Kind::Outline } else { Kind::Ghost };
                if widgets::button_sm(ui, kind, None, &s.label, true).clicked() {
                    d.picked = s.id.clone();
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.scope(|ui| {
                ui.visuals_mut().selection.bg_fill = p.primary;
                ui.checkbox(&mut d.follow, RichText::new("Follow").small().color(p.muted));
            });
        });
    });
    ui.add_space(6.0);
    match d.tail.get() {
        None => widgets::loading(ui, "Loading…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(tail)) => {
            egui::Frame::new()
                .fill(p.muted_bg)
                .stroke(Stroke::new(1.0, p.input))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("service-log").stick_to_bottom(d.follow).max_height(288.0).show(ui, |ui| {
                        if !tail.exists || tail.lines.is_empty() {
                            ui.label(RichText::new("(empty)").color(p.muted).small());
                        }
                        for line in &tail.lines {
                            ui.label(widgets::mono(line.clone(), 12.0).color(p.foreground));
                        }
                    });
                });
        }
    }
}
