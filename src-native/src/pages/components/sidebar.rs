use std::collections::HashMap;

use eframe::egui::{self, Color32, Role, WidgetInfo};
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::service_manager::Health;

use super::Data;
use crate::loader::Loader;
use crate::pages::Action;
use crate::theme::{self, palette};
use crate::widgets::{self, Led};

pub fn render(ui: &mut egui::Ui, d: &Data, current: ComponentId, picked: &mut ComponentId, _out: &mut [Action]) {
    let p = palette(ui);
    for info in d.components {
        let list: Vec<_> = d.installations.iter().filter(|i| i.component == info.id).collect();
        let running = d.services.iter().filter(|s| s.service.component == info.id && s.health == Health::Running).count();
        let active = info.id == current;
        let id = ui.make_persistent_id(("component-row", info.id));
        let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let (fill, border) = match (active, hovered) {
            (true, _) => (p.primary.gamma_multiply(0.07), p.primary.gamma_multiply(0.5)),
            (false, true) => (p.accent, Color32::TRANSPARENT),
            (false, false) => (Color32::TRANSPARENT, Color32::TRANSPARENT),
        };
        let frame = egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(theme::RADIUS + 2))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    logo(ui, d.icons, info.id, !list.is_empty(), 40.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            let name = if active { widgets::strong(info.name, 15.0) } else { egui::RichText::new(info.name).size(15.0).font(egui::FontId::new(15.0, theme::medium())) };
                            ui.label(name);
                            if running > 0 {
                                widgets::led(ui, Led::On, 6.0);
                            }
                        });
                        if list.is_empty() {
                            ui.label(egui::RichText::new("Not installed").size(12.0).color(p.muted));
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                for v in &list {
                                    widgets::chip(ui, &v.version, v.is_default);
                                }
                            });
                        }
                    });
                });
            });
        let r = ui.interact(frame.response.rect, id, egui::Sense::click());
        r.widget_info(|| WidgetInfo::labeled(Role::Button, true, info.name));
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            *picked = info.id;
        }
        ui.add_space(4.0);
    }
}

pub fn logo(ui: &mut egui::Ui, icons: &HashMap<ComponentId, Loader<Option<String>>>, id: ComponentId, installed: bool, size: f32) -> egui::Response {
    let bytes = icons.get(&id).and_then(Loader::get).and_then(|r| r.as_ref().ok()).and_then(|o| o.as_deref()).and_then(widgets::data_url_bytes);
    widgets::component_logo(ui, id, installed, bytes.as_deref(), size)
}
