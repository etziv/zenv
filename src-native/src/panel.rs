use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Label, Layout, Margin, RichText, Sense, Stroke, Ui, UiBuilder};
use zenv_core::service_manager::{Health, ServiceStatus};

use crate::pages::dashboard::{self, service_key, START_APPS, STOP_ALL};
use crate::pages::Action;
use crate::theme::{panel, palette};
use crate::widgets::{self, Kind, Led, Size, SwitchState};
use crate::{lucide, shell};

pub const WIDTH: f32 = 300.0;
pub const HEIGHT: (f32, f32) = (150.0, 460.0);
pub const TITLE: &str = "Zenv · services";
const HEADER: f32 = 41.0;

#[derive(Debug)]
pub enum Out {
    Act(Action),
    OpenMain,
    Quit,
    Dismiss,
}

pub struct View<'a> {
    pub services: Option<&'a [ServiceStatus]>,
    pub version: &'a str,
    pub busy: &'a [String],
    pub error: Option<(&'a str, &'a str)>,
}

pub fn ui(ui: &mut Ui, v: &View) -> (Vec<Out>, f32) {
    let mut out = Vec::new();
    let p = palette(ui);
    let list = v.services.map(|s| dashboard::sorted(s.iter().collect())).unwrap_or_default();
    let total = list.len();
    let (running, others): (Vec<&ServiceStatus>, Vec<&ServiceStatus>) = list.into_iter().partition(|s| s.health == Health::Running);
    let header_frame = Frame::new().fill(p.panel).inner_margin(Margin::symmetric(12, 0));
    egui::Panel::top("tray-header").exact_size(HEADER).frame(header_frame).show(ui, |ui| header(ui, v.version, running.len()));
    let footer = egui::Panel::bottom("tray-footer")
        .frame(Frame::new())
        .show(ui, |ui| footer(ui, v, total, running.len(), &mut out))
        .response
        .rect
        .height();
    let body = egui::CentralPanel::default()
        .frame(Frame::new().fill(p.background))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    Frame::new().inner_margin(Margin::same(4)).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        body(ui, v, &running, &others, &mut out);
                    });
                })
                .content_size
                .y
        })
        .inner;
    (out, HEADER + body + footer)
}

fn header(ui: &mut Ui, version: &str, running: usize) {
    let full = ui.max_rect();
    shell::trace(ui, egui::Rect::from_min_max(egui::pos2(full.left() - 12.0, full.bottom() - 1.0), egui::pos2(full.right() + 12.0, full.bottom())));
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(widgets::display("zenv", 16.0).color(Color32::WHITE));
        ui.label(widgets::mono(version, 10.0).color(panel::MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let text = if running > 0 { format!("{running} running") } else { "all stopped".into() };
            ui.label(widgets::mono(text, 11.0).color(panel::MUTED));
            widgets::led(ui, if running > 0 { Led::On } else { Led::Off }, 6.0);
        });
    });
}

fn body(ui: &mut Ui, v: &View, running: &[&ServiceStatus], others: &[&ServiceStatus], out: &mut Vec<Out>) {
    let muted = palette(ui).muted;
    let note = |ui: &mut Ui, text: &str| {
        Frame::new().inner_margin(Margin::symmetric(8, 12)).show(ui, |ui| ui.label(RichText::new(text).size(12.0).color(muted)));
    };
    if v.services.is_none() {
        note(ui, "Loading services…");
    } else if running.is_empty() && others.is_empty() {
        note(ui, "No services yet. Open Zenv to add a web server or database.");
    } else {
        group(ui, "Running", running, v.busy, out);
        group(ui, if running.is_empty() { "Services" } else { "Stopped" }, others, v.busy, out);
    }
}

fn group(ui: &mut Ui, label: &str, items: &[&ServiceStatus], busy: &[String], out: &mut Vec<Out>) {
    if items.is_empty() {
        return;
    }
    let muted = palette(ui).muted;
    Frame::new().inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 2 }).show(ui, |ui| {
        let text = format!("{label} · {}", items.len()).to_uppercase();
        let heading = ui.label(widgets::mono(widgets::tracked(&text), 9.5).color(muted));
        heading.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Heading, true, &text));
    });
    for s in items {
        row(ui, s, busy, out);
    }
}

fn row(ui: &mut Ui, status: &ServiceStatus, busy: &[String], out: &mut Vec<Out>) {
    let p = palette(ui);
    let s = &status.service;
    let running = status.health == Health::Running;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::hover());
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.muted_bg.gamma_multiply(0.6));
    }
    let inner = rect.shrink2(egui::vec2(8.0, 0.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    child.spacing_mut().item_spacing.x = 8.0;
    widgets::pulse(&mut child, status.health);
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let label = if running { format!("Stop {}", s.name) } else { format!("Start {}", s.name) };
        let pending = busy.contains(&service_key(&s.id));
        let state = SwitchState { on: running, failed: status.health == Health::Failed, pending };
        if widgets::status_switch(ui, &state, &label).clicked() {
            out.push(Out::Act(Action::Run(dashboard::switch_task(&s.id, label, running))));
        }
        let port = status.behind.as_ref().map_or_else(|| format!(":{}", s.http_port), |b| format!("via {b}"));
        ui.allocate_ui_with_layout(egui::vec2(56.0, 32.0), Layout::right_to_left(Align::Center), |ui| {
            ui.add(Label::new(widgets::mono(port, 10.5).color(p.muted)).truncate());
        });
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let name = widgets::medium(&s.name, 12.5).color(if running { p.foreground } else { p.muted });
            ui.add(Label::new(name).truncate());
        });
    });
    let version = status.installation.as_ref().map(|i| format!(" {}", i.version)).unwrap_or_default();
    let via = status.behind.as_ref().map(|b| format!(" via {b}")).unwrap_or_default();
    response.on_hover_text(format!("{}{version} · :{}{via}", s.name, s.http_port));
}

fn footer(ui: &mut Ui, v: &View, total: usize, running: usize, out: &mut Vec<Out>) {
    let p = palette(ui);
    if let Some((title, description)) = v.error {
        error_box(ui, title, description, out);
    }
    let full = ui.available_rect_before_wrap();
    ui.painter().hline(full.x_range(), full.top() + 0.5, Stroke::new(1.0, p.border));
    Frame::new().fill(p.card).inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let starting = v.busy.iter().any(|k| k == START_APPS);
        let stopping = v.busy.iter().any(|k| k == STOP_ALL);
        let half = (ui.available_width() - 6.0) / 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (glyph, label) = if starting { (lucide::LOADER_CIRCLE, "Starting…") } else { (lucide::PLAY, "Start apps") };
            if widgets::button_wide(ui, Kind::Primary, Size::Small, Some(glyph), label, !starting && total > 0, half).clicked() {
                out.push(Out::Act(Action::Run(dashboard::start_apps_task())));
            }
            let (glyph, label) = if stopping { (lucide::LOADER_CIRCLE, "Stopping…") } else { (lucide::SQUARE, "Stop all") };
            if widgets::button_wide(ui, Kind::Outline, Size::Small, Some(glyph), label, !stopping && running > 0, half).clicked() {
                out.push(Out::Act(Action::Run(dashboard::stop_all_task())));
            }
        });
        ui.horizontal(|ui| {
            if widgets::button_sm(ui, Kind::Ghost, Some(lucide::APP_WINDOW), "Open Zenv", true).clicked() {
                out.push(Out::OpenMain);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let quit = widgets::button_sm(ui, Kind::Ghost, Some(lucide::POWER), "Quit", true);
                if quit.on_hover_text("Stops every service and closes Zenv").clicked() {
                    out.push(Out::Quit);
                }
            });
        });
    });
}

fn error_box(ui: &mut Ui, title: &str, description: &str, out: &mut Vec<Out>) {
    let p = palette(ui);
    Frame::new().inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 8 }).show(ui, |ui| {
        Frame::new()
            .fill(p.destructive.gamma_multiply(0.1))
            .stroke(Stroke::new(1.0, p.destructive.gamma_multiply(0.4)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 24.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(widgets::medium(title, 12.0).color(p.destructive));
                        if !description.is_empty() {
                            let mut job = egui::text::LayoutJob::simple(description.to_owned(), egui::FontId::proportional(12.0), p.muted, ui.available_width());
                            job.wrap.max_rows = 3;
                            ui.label(job);
                        }
                    });
                    if widgets::icon_button(ui, lucide::X, "Dismiss").clicked() {
                        out.push(Out::Dismiss);
                    }
                });
            });
    });
}
