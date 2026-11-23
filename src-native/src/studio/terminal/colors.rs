use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use eframe::egui::Color32;

pub const BACKGROUND: Color32 = Color32::from_rgb(0x0a, 0x0b, 0x0b);
pub const FOREGROUND: Color32 = Color32::from_rgb(0xd9, 0xdd, 0xdc);
pub const CURSOR: Color32 = Color32::from_rgb(0x3e, 0xe8, 0xa0);
pub const SELECTION: Color32 = Color32::from_rgba_unmultiplied_const(0x3e, 0xe8, 0xa0, 0x4d);

const DIM: f32 = 0.66;

const ANSI: [u32; 16] = [
    0x161818, 0xff5a4a, 0x3ee8a0, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf, 0x5c6361, 0xff8a7e, 0x7af2c0, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
];

const CUBE: [u8; 6] = [0, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn indexed(i: u8) -> Color32 {
    match i {
        0..=15 => hex(ANSI[i as usize]),
        16..=231 => {
            let n = i as usize - 16;
            Color32::from_rgb(CUBE[n / 36], CUBE[(n / 6) % 6], CUBE[n % 6])
        }
        _ => {
            let v = 8 + 10 * (i - 232);
            Color32::from_rgb(v, v, v)
        }
    }
}

fn dim(c: Color32) -> Color32 {
    let f = |v: u8| (v as f32 * DIM) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

fn named(n: NamedColor, set: &Colors) -> Color32 {
    let i = n as usize;
    if let Some(c) = set[i] {
        return rgb(c);
    }
    match n {
        NamedColor::Foreground | NamedColor::BrightForeground => FOREGROUND,
        NamedColor::Background => BACKGROUND,
        NamedColor::Cursor => CURSOR,
        NamedColor::DimForeground => dim(FOREGROUND),
        _ if i < 16 => indexed(i as u8),
        _ => dim(named(n.to_bright(), set)),
    }
}

pub fn resolve(color: Color, set: &Colors, bold: bool, faint: bool) -> Color32 {
    let c = match color {
        Color::Spec(c) => rgb(c),
        Color::Named(n) if bold && (n as usize) < 8 => named(n.to_bright(), set),
        Color::Named(n) => named(n, set),
        Color::Indexed(i) => {
            let i = if bold && i < 8 { i + 8 } else { i };
            set[i as usize].map_or_else(|| indexed(i), rgb)
        }
    };
    if faint {
        dim(c)
    } else {
        c
    }
}

pub fn default_rgb(index: usize) -> Rgb {
    let c = match index {
        0..=255 => indexed(index as u8),
        256 => FOREGROUND,
        258 => CURSOR,
        _ => BACKGROUND,
    };
    Rgb { r: c.r(), g: c.g(), b: c.b() }
}
