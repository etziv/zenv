pub mod app;
pub mod loader;
pub mod os;
pub mod panel;
#[rustfmt::skip]
pub mod lucide;
pub mod pages;
pub mod shell;
pub mod studio;
pub mod theme;
pub mod tray;
pub mod widgets;

pub const ICON_PNG: &[u8] = include_bytes!("../../src-tauri/icons/128x128.png");
