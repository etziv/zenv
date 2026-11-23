use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Painter, Pos2, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, WidgetInfo,
};
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::service_manager::Health;

use crate::lucide;
use crate::theme::{self, panel, palette};

pub const PAGE_MAX_WIDTH: f32 = 1280.0;
pub const PAGE_GAP: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Led {
    On,
    Fail,
    Off,
}

pub fn paint_led(painter: &Painter, c: Pos2, r: f32, state: Led) {
    let glow = |color: Color32| {
        for (extra, alpha) in [(4.0, 0.10), (2.5, 0.18), (1.0, 0.25)] {
            painter.circle_filled(c, r + extra, color.gamma_multiply(alpha));
        }
        painter.circle_filled(c, r, color);
    };
    match state {
        Led::On => glow(panel::LED),
        Led::Fail => glow(panel::LED_FAIL),
        Led::Off => {
            painter.circle_stroke(c, r - 0.5, Stroke::new(1.0, panel::MUTED));
        }
    }
}

pub fn led(ui: &mut Ui, state: Led, diameter: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(diameter, diameter), Sense::hover());
    paint_led(ui.painter(), rect.center(), diameter / 2.0, state);
    response
}

pub fn health_label(health: Health) -> &'static str {
    match health {
        Health::Running => "Running",
        Health::Stopped => "Stopped",
        Health::Failed => "Exited unexpectedly",
    }
}

pub fn pulse(ui: &mut Ui, health: Health) -> Response {
    let p = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, health_label(health)));
    let c = rect.center();
    let painter = ui.painter();
    match health {
        Health::Running => {
            let t = (ui.input(|i| i.time) % 2.4 / 2.4) as f32;
            let wave = (t * std::f32::consts::TAU).cos() * 0.5 + 0.5;
            let scale = 1.0 + 1.4 * (1.0 - wave);
            painter.circle_filled(c, 5.0 * scale, p.primary.gamma_multiply(0.55 * wave));
            painter.circle_filled(c, 5.0, p.primary);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        Health::Stopped => {
            painter.circle_stroke(c, 4.5, Stroke::new(1.0, p.muted.gamma_multiply(0.5)));
        }
        Health::Failed => {
            painter.circle_filled(c, 5.0, p.destructive);
        }
    }
    response
}

pub fn icon(glyph: &str, size: f32) -> RichText {
    RichText::new(glyph).font(FontId::new(size, theme::icons()))
}

pub fn strong(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, theme::strong()))
}

pub fn medium(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, theme::medium()))
}

pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, egui::FontFamily::Monospace))
}

pub fn display(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, theme::display()))
}

pub fn tracked(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for (i, ch) in text.chars().enumerate() {
        if i > 0 {
            out.push('\u{200A}');
        }
        out.extend(ch.to_uppercase());
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Outline,
    Ghost,
    Dashed,
    Destructive,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Default,
    Small,
}

pub fn button(ui: &mut Ui, kind: Kind, glyph: Option<&str>, label: &str, enabled: bool) -> Response {
    button_sized(ui, kind, Size::Default, glyph, label, enabled)
}

pub fn button_sm(ui: &mut Ui, kind: Kind, glyph: Option<&str>, label: &str, enabled: bool) -> Response {
    button_sized(ui, kind, Size::Small, glyph, label, enabled)
}

pub fn button_sized(ui: &mut Ui, kind: Kind, size: Size, glyph: Option<&str>, label: &str, enabled: bool) -> Response {
    button_wide(ui, kind, size, glyph, label, enabled, 0.0)
}

pub fn button_wide(ui: &mut Ui, kind: Kind, size: Size, glyph: Option<&str>, label: &str, enabled: bool, min_width: f32) -> Response {
    let p = palette(ui);
    let (height, pad, text_size) = match size {
        Size::Default => (32.0, 12.0, 13.5),
        Size::Small => (28.0, 10.0, 12.0),
    };
    let fg = match kind {
        Kind::Primary | Kind::Destructive => p.on_primary,
        Kind::Dashed => p.muted,
        _ => p.foreground,
    };
    let fg = if enabled { fg } else { fg.gamma_multiply(0.5) };
    let text_size = if kind == Kind::Dashed { 12.0 } else { text_size };
    let font = FontId::new(text_size, theme::medium());
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let icon_w = if glyph.is_some() { 16.0 + 6.0 } else { 0.0 };
    let width = (pad * 2.0 + icon_w + text.size().x).max(min_width);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    r.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    let hovered = enabled && r.hovered();
    let (fill, stroke) = match kind {
        Kind::Primary => (if hovered { p.primary.gamma_multiply(0.9) } else { p.primary }, Stroke::NONE),
        Kind::Outline => (if hovered { p.accent } else { p.card }, Stroke::new(1.0, p.input)),
        Kind::Ghost => (if hovered { p.accent } else { Color32::TRANSPARENT }, Stroke::NONE),
        Kind::Dashed => (Color32::TRANSPARENT, Stroke::NONE),
        Kind::Destructive => (if hovered { p.destructive.gamma_multiply(0.9) } else { p.destructive }, Stroke::NONE),
    };
    let fill = if enabled { fill } else { fill.gamma_multiply(0.5) };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(theme::RADIUS), fill, stroke, StrokeKind::Inside);
    if kind == Kind::Dashed {
        let line = if hovered { p.primary.gamma_multiply(0.4) } else { p.border };
        dashed_rect(painter, rect.shrink(0.5), line);
    }
    let mut x = rect.center().x - (icon_w + text.size().x) / 2.0;
    let fg = if kind == Kind::Dashed && hovered { p.foreground } else { fg };
    if let Some(g) = glyph {
        painter.text(egui::pos2(x + 8.0, rect.center().y), egui::Align2::CENTER_CENTER, g, FontId::new(15.0, theme::icons()), fg);
        x += icon_w;
    }
    painter.galley_with_override_text_color(egui::pos2(x, rect.center().y - text.size().y / 2.0), text, fg);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub fn dashed_rect(painter: &Painter, rect: egui::Rect, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    for pair in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(&pair, stroke, 4.0, 3.0));
    }
}

pub fn icon_button(ui: &mut Ui, glyph: &str, label: &str) -> Response {
    let p = palette(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), p.accent);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if r.hovered() { p.foreground } else { p.muted };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::new(14.0, theme::icons()), color);
    r.on_hover_text(label)
}

pub struct SwitchState {
    pub on: bool,
    pub failed: bool,
    pub pending: bool,
}

pub fn switch_text(s: &SwitchState) -> &'static str {
    match (s.on, s.failed) {
        (true, _) => "ON",
        (false, true) => "ERR",
        (false, false) => "OFF",
    }
}

pub fn status_switch(ui: &mut Ui, s: &SwitchState, label: &str) -> Response {
    let p = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(54.0, 24.0), if s.pending { Sense::hover() } else { Sense::click() });
    response.widget_info(|| WidgetInfo::selected(Role::Switch, !s.pending, s.on, label));
    let (fill, border) = match (s.on, s.failed) {
        (true, _) => (p.primary, p.primary),
        (false, true) => (p.destructive.gamma_multiply(0.15), p.destructive.gamma_multiply(0.6)),
        (false, false) => (p.muted_bg, p.input),
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(12), fill, Stroke::new(1.0, border), StrokeKind::Inside);
    let text_color = match (s.on, s.failed) {
        (true, _) => p.on_primary,
        (false, true) => p.destructive,
        (false, false) => p.muted,
    };
    let font = FontId::new(9.5, theme::mono_bold());
    if s.on {
        painter.text(egui::pos2(rect.left() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, switch_text(s), font, text_color);
    } else {
        painter.text(egui::pos2(rect.right() - 6.0, rect.center().y), egui::Align2::RIGHT_CENTER, switch_text(s), font, text_color);
    }
    let knob_r = 8.8;
    let knob_x = if s.on { rect.right() - 2.0 - knob_r } else { rect.left() + 2.0 + knob_r };
    let knob_c = egui::pos2(knob_x, rect.center().y);
    painter.circle_filled(knob_c + egui::vec2(0.0, 0.6), knob_r, Color32::from_black_alpha(40));
    painter.circle_filled(knob_c, knob_r, p.background);
    if s.pending {
        let t = ui.input(|i| i.time) as f32;
        let arc: Vec<Pos2> = (0..=16)
            .map(|i| {
                let a = t * 6.0 + i as f32 / 16.0 * 4.7;
                knob_c + 5.0 * egui::vec2(a.cos(), a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(arc, Stroke::new(1.5, p.muted)));
        ui.ctx().request_repaint();
    }
    if response.hovered() && !s.pending {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn toggle(ui: &mut Ui, on: bool, enabled: bool, label: &str) -> Response {
    let p = palette(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), if enabled { Sense::click() } else { Sense::hover() });
    r.widget_info(|| WidgetInfo::selected(Role::Switch, enabled, on, label));
    let track = if on { p.primary } else { p.input };
    let track = if enabled { track } else { track.gamma_multiply(0.5) };
    ui.painter().rect_filled(rect, CornerRadius::same(10), track);
    let x = if on { rect.right() - 2.0 - 8.0 } else { rect.left() + 2.0 + 8.0 };
    ui.painter().circle_filled(egui::pos2(x, rect.center().y + 0.5), 8.0, Color32::from_black_alpha(30));
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 8.0, p.background);
    if enabled && r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Default,
    Outline,
    Success,
    Warning,
    Destructive,
    Primary,
}

pub fn badge(ui: &mut Ui, text: &str, kind: Badge) -> Response {
    let p = palette(ui);
    let tinted = |c: Color32, border: f32| (c, c.gamma_multiply(0.10), c.gamma_multiply(border));
    let (fg, fill, border) = match kind {
        Badge::Default => (p.foreground, p.secondary, Color32::TRANSPARENT),
        Badge::Outline => (p.muted, Color32::TRANSPARENT, p.border),
        Badge::Success => tinted(p.success, 0.3),
        Badge::Warning => tinted(p.warning, 0.4),
        Badge::Destructive => tinted(p.destructive, 0.3),
        Badge::Primary => tinted(p.primary, 0.3),
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::new(11.0, theme::medium()), fg);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(galley.size().x + 12.0, 18.0), Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    ui.painter().rect(rect, CornerRadius::same(2), fill, Stroke::new(1.0, border), StrokeKind::Inside);
    ui.painter().galley(egui::pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    r
}

pub fn text_input(ui: &mut Ui, text: &mut String, hint: &str, width: f32, glyph: Option<&str>) -> Response {
    let p = palette(ui);
    let left = if glyph.is_some() { 32 } else { 10 };
    let edit = egui::TextEdit::singleline(text)
        .hint_text(RichText::new(hint).color(p.muted))
        .desired_width(width - left as f32 - 10.0)
        .margin(Margin { left, right: 10, top: 7, bottom: 7 })
        .font(FontId::new(14.0, egui::FontFamily::Proportional));
    let r = ui.add_sized([width, 32.0], edit);
    r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, hint));
    if let Some(g) = glyph {
        let c = egui::pos2(r.rect.left() + 16.0, r.rect.center().y);
        ui.painter().text(c, egui::Align2::CENTER_CENTER, g, FontId::new(15.0, theme::icons()), p.muted);
    }
    r
}

pub fn select<R>(ui: &mut Ui, id: impl egui::AsIdSalt, selected: &str, width: f32, options: impl FnOnce(&mut Ui) -> R) -> Response {
    let p = palette(ui);
    let inner = egui::ComboBox::from_id_salt(id)
        .width(width)
        .height(320.0)
        .selected_text(RichText::new(selected).size(14.0))
        .icon(move |ui, rect, _visuals, _open| {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, lucide::CHEVRON_DOWN, FontId::new(15.0, theme::icons()), p.foreground);
        })
        .show_ui(ui, options);
    inner.response
}

pub fn card(ui: &Ui, border: Color32) -> Frame {
    Frame::new()
        .fill(palette(ui).card)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(theme::RADIUS + 2))
        .inner_margin(Margin::same(14))
}

pub fn section<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    section_with(ui, title, |_| {}, body)
}

pub fn section_with<R>(ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let heading = ui.label(mono(tracked(title), 11.0).color(p.muted));
        heading.widget_info(|| WidgetInfo::labeled(Role::Heading, true, title.to_uppercase()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            actions(ui);
            let rest = ui.available_rect_before_wrap();
            ui.painter().hline(rest.x_range(), rest.center().y, Stroke::new(1.0, p.border));
        });
    });
    ui.add_space(12.0 - ui.spacing().item_spacing.y);
    let out = body(ui);
    ui.add_space(20.0 - ui.spacing().item_spacing.y);
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModalChoice {
    None,
    Cancel,
    Confirm,
}

pub fn modal(ctx: &egui::Context, id: &str) -> egui::Modal {
    egui::Modal::new(egui::Id::unique(id)).frame(Frame::popup(&ctx.global_style()).inner_margin(Margin::same(20)))
}

pub fn menu_item(ui: &mut Ui, glyph: &str, label: impl Into<String>, enabled: bool, destructive: bool) -> bool {
    let p = palette(ui);
    let (text, tint) = if destructive { (p.destructive, p.destructive) } else { (p.foreground, p.muted) };
    let label = label.into();
    let content = (icon(glyph, 16.0).color(tint), RichText::new(&label).size(14.0).color(text));
    let button = egui::Button::new(content).frame_when_inactive(false).min_size(egui::vec2(ui.available_width().max(190.0), 32.0));
    let r = ui.add_enabled(enabled, button);
    r.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &label));
    let clicked = r.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

pub fn dialog_title(ui: &mut Ui, title: impl Into<String>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(display(title, 16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = icon_button(ui, lucide::X, "Close").clicked();
        });
    });
    close
}

pub fn confirm_modal(ctx: &egui::Context, id: &str, title: &str, confirm_label: &str, destructive: bool, pending: bool, body: impl FnOnce(&mut Ui)) -> ModalChoice {
    let mut choice = ModalChoice::None;
    let resp = modal(ctx, id).show(ctx, |ui| {
        ui.set_width(472.0);
        if dialog_title(ui, title) && !pending {
            choice = ModalChoice::Cancel;
        }
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        body(ui);
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let kind = if destructive { Kind::Destructive } else { Kind::Primary };
            if button(ui, kind, None, confirm_label, !pending).clicked() {
                choice = ModalChoice::Confirm;
            }
            if button(ui, Kind::Outline, None, "Cancel", !pending).clicked() {
                choice = ModalChoice::Cancel;
            }
        });
    });
    if choice == ModalChoice::None && resp.should_close() && !pending {
        choice = ModalChoice::Cancel;
    }
    choice
}

pub fn page_header(ui: &mut Ui, title: &str, meta: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(display(title, 26.0));
            if !meta.is_empty() {
                ui.label(mono(meta, 11.0).color(palette(ui).muted));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            actions(ui);
        });
    });
    ui.add_space(PAGE_GAP - ui.spacing().item_spacing.y);
}

pub fn empty_state(ui: &mut Ui, title: &str) {
    empty_card(ui, title, None, |_| {});
}

pub fn empty_card(ui: &mut Ui, title: &str, body: Option<&str>, action: impl FnOnce(&mut Ui)) {
    let p = palette(ui);
    let frame = Frame::new()
        .fill(p.card.gamma_multiply(0.6))
        .corner_radius(CornerRadius::same(theme::RADIUS + 2))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(strong(title, 14.0));
            if let Some(b) = body {
                ui.label(RichText::new(b).size(14.0).color(p.muted));
            }
            action(ui);
        });
    dashed_rect(ui.painter(), frame.response.rect.shrink(0.5), p.border);
}

pub fn loading(ui: &mut Ui, text: &str) {
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(16.0).color(palette(ui).muted));
        ui.label(RichText::new(text).size(14.0).color(palette(ui).muted));
    });
    ui.add_space(24.0);
}

pub fn error_panel(ui: &mut Ui, message: &str) {
    let p = palette(ui);
    Frame::new()
        .fill(p.destructive.gamma_multiply(0.05))
        .stroke(Stroke::new(1.0, p.destructive.gamma_multiply(0.3)))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(icon(lucide::CIRCLE_ALERT, 16.0).color(p.destructive));
                ui.label(strong(message, 14.0));
            });
        });
}

pub fn monogram(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Php => "php",
        ComponentId::Go => "go",
        ComponentId::Rust => "rs",
        ComponentId::Ruby => "rb",
        ComponentId::Python => "py",
        ComponentId::Node => "js",
        ComponentId::Apache => "ap",
        ComponentId::Nginx => "ng",
        ComponentId::Caddy => "cd",
        ComponentId::Frankenphp => "fp",
        ComponentId::Mysql => "my",
        ComponentId::Postgresql => "pg",
        ComponentId::Redis => "rd",
        ComponentId::Omniroute => "or",
        ComponentId::Ninerouter => "9r",
    }
}

pub fn component_logo(ui: &mut Ui, component: ComponentId, installed: bool, icon_png: Option<&[u8]>, size: f32) -> Response {
    let p = palette(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let (fill, border, text) = match (icon_png.is_some(), installed) {
        (true, _) => (p.card, p.border, p.foreground),
        (false, true) => (p.primary.gamma_multiply(0.10), p.primary.gamma_multiply(0.4), p.primary),
        (false, false) => (p.muted_bg, p.border, p.muted),
    };
    ui.painter().rect(rect, CornerRadius::same(theme::RADIUS), fill, Stroke::new(1.0, border), StrokeKind::Inside);
    match icon_png {
        Some(bytes) => {
            let image = egui::Image::from_bytes(format!("bytes://logo-{}", component.as_str()), bytes.to_vec());
            image.paint_at(ui, egui::Rect::from_center_size(rect.center(), rect.size() * 0.7));
        }
        None => {
            let font = FontId::new((size * 0.33).round(), theme::mono_bold());
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, monogram(component), font, text);
        }
    }
    r
}

pub fn chip(ui: &mut Ui, text: &str, highlighted: bool) -> Response {
    let p = palette(ui);
    let (fg, fill, border) = if highlighted {
        (p.primary, p.primary.gamma_multiply(0.10), p.primary.gamma_multiply(0.4))
    } else {
        (p.muted, Color32::TRANSPARENT, p.border)
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::new(11.0, egui::FontFamily::Monospace), fg);
    let (rect, r) = ui.allocate_exact_size(galley.size() + egui::vec2(12.0, 2.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(2), fill, Stroke::new(1.0, border), StrokeKind::Inside);
    ui.painter().galley(rect.min + egui::vec2(6.0, 1.0), galley, fg);
    r
}

pub fn dot_grid(ui: &Ui, rect: egui::Rect) {
    let color = palette(ui).dot;
    let painter = ui.painter_at(rect);
    let step = 18.0;
    let mut y = rect.top();
    while y <= rect.bottom() {
        let mut x = rect.left();
        while x <= rect.right() {
            painter.circle_filled(egui::pos2(x, y), 1.0, color);
            x += step;
        }
        y += step;
    }
}

pub fn data_url_bytes(url: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let (_, data) = url.split_once(";base64,")?;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

pub fn grid_columns(window_width: f32, steps: &[(f32, usize)]) -> usize {
    steps.iter().rev().find(|(min, _)| window_width >= *min).map_or(1, |(_, n)| *n)
}

pub fn columns_for(width: f32, min: f32, max_cols: usize) -> usize {
    ((width / min).floor() as usize).clamp(1, max_cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_url_decodes_base64_only() {
        assert_eq!(data_url_bytes("data:image/png;base64,aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(data_url_bytes("data:image/png,hi"), None);
        assert_eq!(data_url_bytes("data:image/png;base64,***"), None);
    }

    #[test]
    fn columns_follow_width() {
        assert_eq!(columns_for(200.0, 280.0, 3), 1);
        assert_eq!(columns_for(600.0, 280.0, 3), 2);
        assert_eq!(columns_for(2000.0, 280.0, 3), 3);
    }

    #[test]
    fn grid_columns_follow_tailwind_breakpoints() {
        let steps = [(0.0, 1), (640.0, 2), (1280.0, 3)];
        assert_eq!(grid_columns(500.0, &steps), 1);
        assert_eq!(grid_columns(1000.0, &steps), 2);
        assert_eq!(grid_columns(1280.0, &steps), 3);
    }

    #[test]
    fn switch_text_follows_state() {
        assert_eq!(switch_text(&SwitchState { on: true, failed: true, pending: false }), "ON");
        assert_eq!(switch_text(&SwitchState { on: false, failed: true, pending: false }), "ERR");
        assert_eq!(switch_text(&SwitchState { on: false, failed: false, pending: true }), "OFF");
    }

    #[test]
    fn tracked_spaces_capitals() {
        assert_eq!(tracked("Web"), "W\u{200A}E\u{200A}B");
    }
}
