use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme, Visuals,
};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: Color32,
    pub foreground: Color32,
    pub card: Color32,
    pub popover: Color32,
    pub primary: Color32,
    pub on_primary: Color32,
    pub secondary: Color32,
    pub muted_bg: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub destructive: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub border: Color32,
    pub input: Color32,
    pub dot: Color32,
    pub panel: Color32,
    pub panel_raised: Color32,
    pub panel_line: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    background: hex(0xf4f5f5),
    foreground: hex(0x111314),
    card: hex(0xffffff),
    popover: hex(0xffffff),
    primary: hex(0x0b8a65),
    on_primary: hex(0xf4fffa),
    secondary: hex(0xeceeee),
    muted_bg: hex(0xeeefef),
    muted: hex(0x60666a),
    accent: hex(0xe8eaea),
    destructive: hex(0xd23f31),
    success: hex(0x12935f),
    warning: hex(0xb7791f),
    border: hex(0xdfe2e3),
    input: hex(0xd3d7d9),
    dot: Color32::from_rgba_unmultiplied_const(17, 19, 20, 18),
    panel: hex(0x0a0b0b),
    panel_raised: hex(0x161818),
    panel_line: hex(0x232626),
};

pub const DARK: Palette = Palette {
    background: hex(0x08090a),
    foreground: hex(0xe8eaea),
    card: hex(0x15181a),
    popover: hex(0x1a1d1f),
    primary: hex(0x3ddc97),
    on_primary: hex(0x04140d),
    secondary: hex(0x1d2022),
    muted_bg: hex(0x1b1e20),
    muted: hex(0x9399a0),
    accent: hex(0x23272a),
    destructive: hex(0xf2685b),
    success: hex(0x4fd99a),
    warning: hex(0xe6ad4c),
    border: hex(0x2b2f32),
    input: hex(0x363b3e),
    dot: Color32::from_rgba_unmultiplied_const(232, 234, 234, 13),
    panel: hex(0x050606),
    panel_raised: hex(0x141617),
    panel_line: hex(0x212426),
};

pub mod panel {
    use eframe::egui::Color32;
    pub const TEXT: Color32 = Color32::from_rgb(0xd9, 0xdd, 0xdc);
    pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x90, 0x8e);
    pub const LED: Color32 = Color32::from_rgb(0x3e, 0xe8, 0xa0);
    pub const LED_FAIL: Color32 = Color32::from_rgb(0xff, 0x5a, 0x4a);
}

pub const RADIUS: u8 = 6;

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode { &DARK } else { &LIGHT }
}

pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn strong() -> FontFamily {
    FontFamily::Name("strong".into())
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn mono_bold() -> FontFamily {
    FontFamily::Name("mono-bold".into())
}

pub fn icons() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    ctx.set_fonts(fonts());
    for theme in [Theme::Light, Theme::Dark] {
        let p = if theme == Theme::Dark { &DARK } else { &LIGHT };
        ctx.set_visuals_of(theme, visuals(theme, p));
        ctx.style_mut_of(theme, |style| {
            style.text_styles = [
                (TextStyle::Heading, FontId::new(26.0, display())),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, medium())),
                (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size.y = 20.0;
        });
    }
}

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let add = |defs: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        defs.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut defs, "plex", include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"));
    add(&mut defs, "plex-semibold", include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"));
    add(&mut defs, "plex-medium", include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"));
    add(&mut defs, "jetbrains", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    add(&mut defs, "jetbrains-bold", include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"));
    add(&mut defs, "chakra", include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf"));
    add(&mut defs, "lucide", include_bytes!("../assets/fonts/lucide.ttf"));
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str| -> Vec<String> {
        let mut v = vec![first.to_owned(), "lucide".to_owned()];
        v.extend(fallback.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, family("plex"));
    defs.families.insert(FontFamily::Monospace, family("jetbrains"));
    defs.families.insert(strong(), family("plex-semibold"));
    defs.families.insert(medium(), family("plex-medium"));
    defs.families.insert(mono_bold(), family("jetbrains-bold"));
    defs.families.insert(display(), family("chakra"));
    defs.families.insert(icons(), vec!["lucide".to_owned()]);
    let root = std::env::var_os("SystemRoot").map_or_else(|| std::path::PathBuf::from(r"C:\Windows"), std::path::PathBuf::from);
    if let Ok(bytes) = std::fs::read(root.join(r"Fonts\seguisym.ttf")) {
        defs.font_data.insert("segoe-symbol".to_owned(), Arc::new(FontData::from_owned(bytes)));
        for f in [FontFamily::Monospace, mono_bold()] {
            defs.families.entry(f).or_default().push("segoe-symbol".to_owned());
        }
    }
    defs
}

fn visuals(theme: Theme, p: &Palette) -> Visuals {
    let mut v = if theme == Theme::Dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(RADIUS);
    v.panel_fill = p.background;
    v.window_fill = p.popover;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(RADIUS + 2);
    v.extreme_bg_color = p.card;
    v.code_bg_color = p.muted_bg;
    v.faint_bg_color = p.background;
    v.override_text_color = Some(p.foreground);
    v.hyperlink_color = p.primary;
    v.selection.bg_fill = p.primary.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.primary);
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    let w = &mut v.widgets;
    for (state, fill) in [
        (&mut w.noninteractive, p.card),
        (&mut w.inactive, p.card),
        (&mut w.hovered, p.accent),
        (&mut w.active, p.accent),
        (&mut w.open, p.accent),
    ] {
        state.corner_radius = r;
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, p.input);
        state.fg_stroke = Stroke::new(1.0, p.foreground);
        state.expansion = 0.0;
    }
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.foreground);
    w.hovered.bg_stroke = Stroke::new(1.0, p.primary.gamma_multiply(0.6));
    w.active.bg_stroke = Stroke::new(1.0, p.primary);
    v
}
