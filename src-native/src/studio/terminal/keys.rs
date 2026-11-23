use alacritty_terminal::term::TermMode;
use eframe::egui::{Key, Modifiers};

fn param(m: Modifiers) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8
}

fn cursor(letter: char, m: Modifiers, mode: TermMode) -> String {
    match param(m) {
        1 if mode.contains(TermMode::APP_CURSOR) => format!("\x1bO{letter}"),
        1 => format!("\x1b[{letter}"),
        p => format!("\x1b[1;{p}{letter}"),
    }
}

fn tilde(code: u8, m: Modifiers) -> String {
    match param(m) {
        1 => format!("\x1b[{code}~"),
        p => format!("\x1b[{code};{p}~"),
    }
}

fn function(n: u8, m: Modifiers) -> String {
    const TILDE: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
    match (n, param(m)) {
        (1..=4, 1) => format!("\x1bO{}", (b'P' + n - 1) as char),
        (1..=4, p) => format!("\x1b[1;{p}{}", (b'P' + n - 1) as char),
        _ => tilde(TILDE[n as usize - 5], m),
    }
}

fn control(key: Key) -> Option<u8> {
    let name = key.name();
    match key {
        Key::Space | Key::Num2 => Some(0),
        Key::OpenBracket => Some(0x1b),
        Key::Backslash => Some(0x1c),
        Key::CloseBracket => Some(0x1d),
        Key::Slash | Key::Minus => Some(0x1f),
        _ if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => Some(name.as_bytes()[0] - b'A' + 1),
        _ => None,
    }
}

pub fn encode(key: Key, m: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let esc = |s: &str| if m.alt { format!("\x1b{s}") } else { s.to_owned() };
    let s = match key {
        Key::Enter if m.shift || m.alt => "\x1b\r".to_owned(),
        Key::Enter => "\r".to_owned(),
        Key::Backspace if m.ctrl => "\x08".to_owned(),
        Key::Backspace => esc("\x7f"),
        Key::Tab if m.shift => "\x1b[Z".to_owned(),
        Key::Tab => "\t".to_owned(),
        Key::Escape => "\x1b".to_owned(),
        Key::ArrowUp => cursor('A', m, mode),
        Key::ArrowDown => cursor('B', m, mode),
        Key::ArrowRight => cursor('C', m, mode),
        Key::ArrowLeft => cursor('D', m, mode),
        Key::Home => cursor('H', m, mode),
        Key::End => cursor('F', m, mode),
        Key::Insert => tilde(2, m),
        Key::Delete => tilde(3, m),
        Key::PageUp => tilde(5, m),
        Key::PageDown => tilde(6, m),
        Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12 => {
            function(key.name()[1..].parse().ok()?, m)
        }
        _ if m.ctrl => {
            let c = control(key)?;
            return Some(if m.alt { vec![0x1b, c] } else { vec![c] });
        }
        _ => return None,
    };
    Some(s.into_bytes())
}

pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{}\x1b[201~", text.replace(['\x1b', '\x03'], "")).into_bytes()
    } else {
        text.into_bytes()
    }
}

pub mod button {
    pub const LEFT: u8 = 0;
    pub const MIDDLE: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const RELEASE: u8 = 3;
    pub const WHEEL_UP: u8 = 64;
    pub const WHEEL_DOWN: u8 = 65;
    pub const MOTION: u8 = 32;
}

pub fn mouse(code: u8, col: usize, line: usize, pressed: bool, m: Modifiers, mode: TermMode) -> Vec<u8> {
    let code = code + 4 * m.shift as u8 + 8 * m.alt as u8 + 16 * m.ctrl as u8;
    if mode.contains(TermMode::SGR_MOUSE) {
        return format!("\x1b[<{code};{};{}{}", col + 1, line + 1, if pressed { 'M' } else { 'm' }).into_bytes();
    }
    let code = if pressed { code } else { button::RELEASE + (code & !3) };
    let at = |v: usize| (32 + 1 + v.min(222)) as u8;
    vec![0x1b, b'[', b'M', 32 + code, at(col), at(line)]
}
