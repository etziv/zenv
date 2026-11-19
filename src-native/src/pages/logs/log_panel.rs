use std::path::Path;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui};
use zenv_core::log_manager::LogTail;
use zenv_core::service_manager::{Health, LogSource};

use crate::loader::Loader;
use crate::lucide;
use crate::pages::Action;
use crate::theme::{self, palette};
use crate::widgets::{self, Kind};

pub struct ViewerState {
    pub picked: Option<String>,
    pub filter: String,
    pub follow: bool,
    pub tail: Loader<LogTail>,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self { picked: None, filter: String::new(), follow: true, tail: Loader::default() }
    }
}

impl ViewerState {
    pub fn reset(&mut self) {
        self.picked = None;
        self.tail = Loader::default();
    }
}

pub fn active_source<'a>(sources: &'a [LogSource], picked: Option<&str>) -> Option<&'a LogSource> {
    picked
        .and_then(|id| sources.iter().find(|s| s.id == id))
        .or_else(|| sources.iter().find(|s| s.exists && s.size_bytes > 0))
        .or_else(|| sources.first())
}

pub fn list_row(ui: &mut Ui, active: bool, label_for_a11y: &str, draw: impl FnOnce(&Ui, Rect)) -> Response {
    let p = palette(ui);
    let width = ui.available_width();
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, label_for_a11y));
    let hovered = r.hovered();
    if active {
        ui.painter().rect_filled(rect, 0, p.accent);
        ui.painter().rect_filled(Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 2.0, rect.bottom())), 0, p.primary);
    } else if hovered {
        ui.painter().rect_filled(rect, 0, p.accent.gamma_multiply(0.6));
    }
    draw(ui, rect);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub fn text_row(ui: &mut Ui, active: bool, label: &str) -> Response {
    list_row(ui, active, label, |ui, rect| {
        let p = palette(ui);
        ui.painter().text(egui::pos2(rect.left() + 12.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::new(13.0, egui::FontFamily::Proportional), p.foreground);
    })
}

pub fn site_row(ui: &mut Ui, active: bool, domain: &str, enabled: bool) -> Response {
    list_row(ui, active, domain, |ui, rect| {
        let p = palette(ui);
        let color = if enabled { p.primary } else { p.muted.gamma_multiply(0.5) };
        ui.painter().circle_filled(egui::pos2(rect.left() + 14.0, rect.center().y), 2.5, color);
        ui.painter().text(egui::pos2(rect.left() + 24.0, rect.center().y), Align2::LEFT_CENTER, domain, FontId::new(12.0, egui::FontFamily::Monospace), p.foreground);
    })
}

fn paint_health_dot(ui: &Ui, center: Pos2, health: Health) {
    let p = palette(ui);
    let painter = ui.painter();
    match health {
        Health::Running => {
            let t = (ui.input(|i| i.time) % 2.4 / 2.4) as f32;
            let wave = (t * std::f32::consts::TAU).cos() * 0.5 + 0.5;
            let scale = 1.0 + 1.4 * (1.0 - wave);
            painter.circle_filled(center, 5.0 * scale, p.primary.gamma_multiply(0.55 * wave));
            painter.circle_filled(center, 5.0, p.primary);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        Health::Stopped => {
            painter.circle_stroke(center, 4.5, Stroke::new(1.0, p.muted.gamma_multiply(0.5)));
        }
        Health::Failed => {
            painter.circle_filled(center, 5.0, p.destructive);
        }
    }
}

pub fn service_row(ui: &mut Ui, active: bool, health: Health, name: &str, port: u16) -> Response {
    let label = format!("{name} :{port}");
    list_row(ui, active, &label, |ui, rect| {
        let p = palette(ui);
        paint_health_dot(ui, egui::pos2(rect.left() + 18.0, rect.center().y), health);
        let port_text = format!(":{port}");
        let galley = ui.painter().layout_no_wrap(port_text, FontId::new(10.5, egui::FontFamily::Monospace), p.muted);
        let name_right = rect.right() - 10.0 - galley.size().x - 6.0;
        ui.painter().galley(egui::pos2(rect.right() - 10.0 - galley.size().x, rect.center().y - galley.size().y / 2.0), galley, p.muted);
        let name_galley = ui.painter().layout(name.to_owned(), FontId::new(13.0, egui::FontFamily::Proportional), p.foreground, (name_right - (rect.left() + 30.0)).max(10.0));
        ui.painter().galley(egui::pos2(rect.left() + 30.0, rect.center().y - name_galley.size().y / 2.0), name_galley, Color32::PLACEHOLDER);
    })
}

pub fn group_heading(ui: &mut Ui, label: &str) {
    let p = palette(ui);
    ui.add_space(6.0);
    ui.label(widgets::mono(widgets::tracked(&label.to_uppercase()), 10.0).color(p.muted));
}

pub fn show(ui: &mut Ui, title: &str, subtitle: Option<&str>, sources: &[LogSource], viewer: &mut ViewerState, empty_hint: &str, out: &mut Vec<Action>) {
    let p = palette(ui);
    let active = active_source(sources, viewer.picked.as_deref());
    widgets::card(ui, p.border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_height(ui.available_height());
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(widgets::display(title, 18.0));
                    if let Some(s) = subtitle.filter(|s| !s.is_empty()) {
                        ui.label(widgets::mono(s, 11.0).color(p.muted));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(a) = active {
                        if widgets::button_sm(ui, Kind::Outline, Some(lucide::EXTERNAL_LINK), "Open file", a.exists).clicked() {
                            out.push(Action::OpenUrl(file_url(&a.path)));
                        }
                    }
                });
            });
            if sources.is_empty() {
                widgets::empty_state(ui, empty_hint);
                return;
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                source_tabs(ui, sources, active, viewer);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.scope(|ui| {
                        ui.visuals_mut().selection.bg_fill = p.primary;
                        ui.checkbox(&mut viewer.follow, "Follow");
                    });
                    widgets::text_input(ui, &mut viewer.filter, "Filter lines", 192.0, Some(lucide::SEARCH));
                });
            });
            ui.add_space(8.0);
            match viewer.tail.get() {
                None => widgets::loading(ui, "Loading…"),
                Some(Err(e)) => widgets::error_panel(ui, e),
                Some(Ok(tail)) if !tail.exists => widgets::empty_state(ui, "Nothing written to this log yet."),
                Some(Ok(tail)) => {
                    let filter = viewer.filter.to_lowercase();
                    let lines: Vec<&String> = tail.lines.iter().filter(|l| filter.is_empty() || l.to_lowercase().contains(&filter)).collect();
                    let box_height = ui.available_height().max(256.0) - 22.0;
                    egui::Frame::new()
                        .fill(p.muted_bg)
                        .stroke(Stroke::new(1.0, p.input))
                        .corner_radius(CornerRadius::same(theme::RADIUS))
                        .inner_margin(Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::ScrollArea::vertical()
                                .stick_to_bottom(viewer.follow)
                                .max_height(box_height)
                                .min_scrolled_height(box_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_min_height(box_height - 12.0);
                                    if lines.is_empty() {
                                        ui.label(egui::RichText::new(if filter.is_empty() { "(empty)" } else { "No lines match the filter." }).color(p.muted).small());
                                    }
                                    for line in lines {
                                        ui.label(widgets::mono(line.clone(), 12.0).color(p.foreground));
                                    }
                                });
                        });
                    let mut footer = format!("{} · {}", tail.path.display(), format_bytes(tail.size_bytes));
                    if tail.truncated {
                        footer.push_str(&format!(" · last {} lines", tail.lines.len()));
                    }
                    ui.label(widgets::mono(footer, 10.5).color(p.muted));
                }
            }
        });
    });
}

fn source_tabs(ui: &mut Ui, sources: &[LogSource], active: Option<&LogSource>, viewer: &mut ViewerState) {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.muted_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for s in sources {
                    let is_active = active.is_some_and(|a| a.id == s.id);
                    let size = s.exists.then(|| format_bytes(s.size_bytes));
                    if source_tab(ui, &s.label, size.as_deref(), is_active).clicked() {
                        viewer.picked = Some(s.id.clone());
                        viewer.tail = Loader::default();
                    }
                }
            });
        });
}

fn source_tab(ui: &mut Ui, label: &str, size: Option<&str>, active: bool) -> egui::Response {
    let p = palette(ui);
    let fg = if active { p.foreground } else { p.muted };
    let text = ui.painter().layout_no_wrap(label.to_owned(), FontId::new(12.0, theme::medium()), fg);
    let size_text = size.map(|s| ui.painter().layout_no_wrap(s.to_owned(), FontId::new(10.0, egui::FontFamily::Monospace), p.muted));
    let gap = if size_text.is_some() { 6.0 } else { 0.0 };
    let content_w = text.size().x + gap + size_text.as_ref().map_or(0.0, |g| g.size().x);
    let full_label = match size { Some(s) => format!("{label} {s}"), None => label.to_owned() };
    let (rect, r) = ui.allocate_exact_size(egui::vec2(content_w + 16.0, 10.0f32.max(text.size().y) + 10.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, &full_label));
    let hovered = r.hovered();
    if active {
        ui.painter().rect(rect, CornerRadius::same(4), p.card, Stroke::NONE, StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect(rect, CornerRadius::same(4), p.card.gamma_multiply(0.6), Stroke::NONE, StrokeKind::Inside);
    }
    let mut x = rect.center().x - content_w / 2.0;
    let y = rect.center().y - text.size().y / 2.0;
    ui.painter().galley_with_override_text_color(egui::pos2(x, y), text.clone(), fg);
    x += text.size().x + gap;
    if let Some(g) = size_text {
        ui.painter().galley_with_override_text_color(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, p.muted);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if n == 0 {
        return "0 B".to_owned();
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{n} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

pub fn file_url(path: &Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}
