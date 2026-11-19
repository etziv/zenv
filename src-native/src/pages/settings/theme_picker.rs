use eframe::egui::{self, CornerRadius, FontId, Margin, Sense, Stroke, StrokeKind, ThemePreference, Ui};

use crate::lucide;
use crate::theme::{self, palette};
use crate::widgets;

const OPTIONS: [(ThemePreference, &str, &str); 3] =
    [(ThemePreference::Dark, lucide::MOON, "Dark"), (ThemePreference::Light, lucide::SUN, "Light"), (ThemePreference::System, lucide::MONITOR, "Follow Windows")];

pub fn show(ui: &mut Ui) {
    let p = palette(ui);
    widgets::card(ui, p.border).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let current = ui.ctx().options(|o| o.theme_preference);
        ui.horizontal(|ui| {
            ui.label("Theme");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| segmented(ui, current));
        });
    });
}

fn segmented(ui: &mut Ui, current: ThemePreference) {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.muted_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (pref, glyph, label) in OPTIONS {
                    if segment(ui, glyph, label, current == pref).clicked() {
                        ui.ctx().set_theme(pref);
                    }
                }
            });
        });
}

fn segment(ui: &mut Ui, glyph: &str, label: &str, active: bool) -> egui::Response {
    let p = palette(ui);
    let font = FontId::new(12.0, theme::medium());
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, p.foreground);
    let width = 20.0 + 16.0 + 6.0 + text.size().x;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, active, label));
    let hovered = r.hovered();
    let fg = if active { p.foreground } else { p.muted };
    if active {
        ui.painter().rect(rect, CornerRadius::same(4), p.card, Stroke::NONE, StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect(rect, CornerRadius::same(4), p.card.gamma_multiply(0.6), Stroke::NONE, StrokeKind::Inside);
    }
    let mut x = rect.left() + 10.0;
    ui.painter().text(egui::pos2(x + 8.0, rect.center().y), egui::Align2::CENTER_CENTER, glyph, FontId::new(13.0, theme::icons()), fg);
    x += 16.0 + 6.0;
    ui.painter().galley_with_override_text_color(egui::pos2(x, rect.center().y - text.size().y / 2.0), text, fg);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}
