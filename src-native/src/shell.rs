use eframe::egui::{self, Color32, CornerRadius, FontId, Frame, Margin, Rect, Sense, Stroke, StrokeKind, ThemePreference, Ui};
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::service_manager::{Health, ServiceStatus};

use crate::lucide;
use crate::pages::{Action, Route, NAV};
use crate::theme::{self, panel, palette};
use crate::widgets::{self, Led};

pub const HEADER_HEIGHT: f32 = 48.0;
pub const RAIL_HEIGHT: f32 = 34.0;

pub fn header(ui: &mut Ui, route: Route, version: &str, extra: impl FnOnce(&mut Ui, &mut Vec<Action>), out: &mut Vec<Action>) {
    let p = palette(ui);
    let full = ui.max_rect();
    ui.painter().hline(full.x_range(), full.bottom() - 0.5, Stroke::new(1.0, p.panel_line));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.set_height(HEADER_HEIGHT);
        let mark = ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(widgets::display("zenv", 19.0).color(Color32::WHITE));
            ui.add_space(-2.0);
            ui.label(widgets::mono(version, 10.0).color(panel::MUTED));
        });
        let mark = mark.response.interact(Sense::click());
        mark.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, "Zenv dashboard"));
        if mark.clicked() {
            out.push(Action::Navigate(Route::Dashboard));
        }
        ui.add_space(16.0);
        for (i, (_, routes)) in NAV.iter().enumerate() {
            if i > 0 {
                let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, HEADER_HEIGHT), Sense::hover());
                ui.painter().vline(r.center().x, (r.top() + 16.0)..=(r.bottom() - 16.0), Stroke::new(1.0, p.panel_line));
            }
            for (target, label) in routes.iter() {
                if nav_tab(ui, label, *target == route).clicked() {
                    out.push(Action::Navigate(*target));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if panel_icon(ui, lucide::SETTINGS, "Settings", route == Route::Settings).clicked() {
                out.push(Action::Navigate(Route::Settings));
            }
            let pref = ui.ctx().options(|o| o.theme_preference);
            let (glyph, next, label) = match pref {
                ThemePreference::System => (lucide::MONITOR, ThemePreference::Light, "Theme: follow Windows (click to change)"),
                ThemePreference::Light => (lucide::SUN, ThemePreference::Dark, "Theme: light (click to change)"),
                ThemePreference::Dark => (lucide::MOON, ThemePreference::System, "Theme: dark (click to change)"),
            };
            if panel_icon(ui, glyph, label, false).clicked() {
                ui.ctx().set_theme(next);
            }
            extra(ui, out);
        });
    });
}

fn nav_tab(ui: &mut Ui, label: &str, active: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::new(13.0, theme::strong()), panel::TEXT);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20.0, HEADER_HEIGHT), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, label));
    let color = if active {
        Color32::WHITE
    } else if r.hovered() {
        panel::TEXT
    } else {
        panel::MUTED
    };
    let painter = ui.painter();
    painter.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        let bar = Rect::from_min_max(egui::pos2(rect.left() + 8.0, rect.bottom() - 2.0), egui::pos2(rect.right() - 8.0, rect.bottom()));
        painter.rect_filled(bar, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, panel::LED);
    }
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

fn panel_icon(ui: &mut Ui, glyph: &str, label: &str, active: bool) -> egui::Response {
    let p = palette(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if active || r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), p.panel_raised);
    }
    let color = if active {
        panel::LED
    } else if r.hovered() {
        panel::TEXT
    } else {
        panel::MUTED
    };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::new(16.0, theme::icons()), color);
    r.on_hover_text(label)
}

pub fn rail_entries(services: &[ServiceStatus]) -> Vec<&ServiceStatus> {
    services.iter().filter(|s| s.health != Health::Stopped).collect()
}

pub fn rail_name(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Postgresql => "postgres",
        ComponentId::Ninerouter => "9router",
        other => other.as_str(),
    }
}

pub fn page_for(kind: Category) -> Route {
    match kind {
        Category::WebServer => Route::WebServers,
        Category::Database => Route::Databases,
        Category::Cache => Route::Cache,
        Category::Tool => Route::Tools,
        Category::Language => Route::Languages,
    }
}

pub fn rail_empty_text(services: &[ServiceStatus]) -> &'static str {
    if services.is_empty() {
        "no services"
    } else {
        "all services stopped"
    }
}

pub fn rail(ui: &mut Ui, services: Option<&[ServiceStatus]>, out: &mut Vec<Action>) {
    let edge = ui.clip_rect();
    trace(ui, Rect::from_min_max(egui::pos2(edge.left(), edge.bottom() - 1.0), edge.right_bottom()));
    let Some(services) = services else { return };
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let live = rail_entries(services);
        if live.is_empty() {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::led(ui, Led::Off, 6.0);
            ui.label(widgets::mono(rail_empty_text(services), 11.0).color(panel::MUTED));
            return;
        }
        for s in live {
            if rail_chip(ui, s).clicked() {
                out.push(Action::Navigate(page_for(s.kind)));
            }
        }
    });
}

fn trace(ui: &Ui, rect: Rect) {
    let lit = panel::LED.gamma_multiply(0.45);
    let clear = Color32::TRANSPARENT;
    let mut mesh = egui::Mesh::default();
    let xs = [0.0, 0.3, 0.7, 1.0];
    let colors = [clear, lit, lit, clear];
    for (x, c) in xs.iter().zip(colors) {
        let px = rect.left() + rect.width() * x;
        mesh.colored_vertex(egui::pos2(px, rect.top()), c);
        mesh.colored_vertex(egui::pos2(px, rect.bottom()), c);
    }
    for i in 0..3u32 {
        let b = i * 2;
        mesh.add_triangle(b, b + 1, b + 2);
        mesh.add_triangle(b + 1, b + 2, b + 3);
    }
    ui.painter().add(mesh);
}

pub fn chip_label(status: &ServiceStatus) -> String {
    let s = &status.service;
    let version = status.installation.as_ref().map(|i| format!(" {}", i.version)).unwrap_or_default();
    let failed = status.health == Health::Failed;
    format!("{}{version} · {}:{}{}", s.name, s.listen_address, s.http_port, if failed { " · exited unexpectedly" } else { "" })
}

fn rail_chip(ui: &mut Ui, status: &ServiceStatus) -> egui::Response {
    let p = palette(ui);
    let s = &status.service;
    let failed = status.health == Health::Failed;
    let font = FontId::new(11.0, egui::FontFamily::Monospace);
    let name = ui.painter().layout_no_wrap(rail_name(s.component).to_owned(), font.clone(), panel::MUTED);
    let port_color = if failed { panel::LED_FAIL } else { panel::TEXT };
    let port = ui.painter().layout_no_wrap(format!(":{}", s.http_port), font, port_color);
    let width = 6.0 + 6.0 + 6.0 + name.size().x + 6.0 + port.size().x + 6.0;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 24.0), Sense::click());
    let label = chip_label(status);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    let painter = ui.painter();
    let line = if r.hovered() { panel::MUTED } else { p.panel_line };
    painter.rect(rect, CornerRadius::same(2), p.panel_raised, Stroke::new(1.0, line), StrokeKind::Inside);
    let led_center = egui::pos2(rect.left() + 9.0, rect.center().y);
    widgets::paint_led(painter, led_center, 3.0, if failed { Led::Fail } else { Led::On });
    let x = rect.left() + 18.0;
    let ny = rect.center().y - name.size().y / 2.0;
    let nw = name.size().x;
    painter.galley(egui::pos2(x, ny), name, Color32::PLACEHOLDER);
    painter.galley(egui::pos2(x + nw + 6.0, ny), port, Color32::PLACEHOLDER);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r.on_hover_text(label)
}

pub fn header_frame(ui: &Ui) -> Frame {
    Frame::new().fill(palette(ui).panel).inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 0 })
}

pub fn rail_frame(ui: &Ui) -> Frame {
    Frame::new().fill(palette(ui).panel).inner_margin(Margin::symmetric(16, 0))
}

pub fn frame(ui: &mut Ui, route: Route, version: &str, services: Option<&[ServiceStatus]>, page: impl FnOnce(&mut Ui, &mut Vec<Action>)) -> Vec<Action> {
    frame_with(ui, route, version, services, |_, _| {}, page)
}

pub fn frame_with(
    ui: &mut Ui,
    route: Route,
    version: &str,
    services: Option<&[ServiceStatus]>,
    header_extra: impl FnOnce(&mut Ui, &mut Vec<Action>),
    page: impl FnOnce(&mut Ui, &mut Vec<Action>),
) -> Vec<Action> {
    let mut out = Vec::new();
    egui::Panel::top("header").exact_size(HEADER_HEIGHT).frame(header_frame(ui)).show(ui, |ui| header(ui, route, version, header_extra, &mut out));
    egui::Panel::top("rail").exact_size(RAIL_HEIGHT).frame(rail_frame(ui)).show(ui, |ui| rail(ui, services, &mut out));
    let surface = Frame::new().fill(palette(ui).background);
    egui::CentralPanel::default().frame(surface).show(ui, |ui| {
        widgets::dot_grid(ui, ui.max_rect());
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let avail = ui.available_width();
            let width = avail.min(widgets::PAGE_MAX_WIDTH);
            ui.horizontal_top(|ui| {
                ui.add_space(((avail - width) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(width);
                    Frame::new().inner_margin(Margin { left: 32, right: 32, top: 28, bottom: 16 }).show(ui, |ui| {
                        ui.set_width(width - 64.0);
                        page(ui, &mut out);
                    });
                });
            });
        });
    });
    out
}
