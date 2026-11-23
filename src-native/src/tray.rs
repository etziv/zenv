use std::sync::mpsc::Sender;

use eframe::egui::{self, Pos2, Rect, Vec2};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use zenv_core::service_manager::{Health, ServiceStatus};

const ICON_PNG: &[u8] = include_bytes!("../../src-tauri/icons/32x32.png");
const GAP: f32 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    TogglePanel(Rect),
    ShowMain,
    Quit,
    Tooltip(String),
}

pub fn install(ctx: &egui::Context, tx: Sender<Msg>, tooltip: &str) -> Result<TrayIcon, String> {
    let open = MenuItem::with_id("open", "Open Zenv", true, None);
    let quit = MenuItem::with_id("quit", "Quit Zenv (stops all services)", true, None);
    let menu = Menu::with_items(&[&open, &PredefinedMenuItem::separator(), &quit]).map_err(|e| e.to_string())?;
    let image = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).map_err(|e| e.to_string())?.into_rgba8();
    let (w, h) = image.dimensions();
    let icon = Icon::from_rgba(image.into_raw(), w, h).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_id("zenv")
        .with_icon(icon)
        .with_tooltip(tooltip)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;

    let (tray_tx, tray_ctx) = (tx.clone(), ctx.clone());
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let Some(msg) = on_tray(&e) {
            let _ = tray_tx.send(msg);
            tray_ctx.request_repaint();
        }
    }));
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let msg = match e.id().as_ref() {
            "open" => Msg::ShowMain,
            "quit" => Msg::Quit,
            _ => return,
        };
        let _ = tx.send(msg);
        menu_ctx.request_repaint();
    }));
    Ok(tray)
}

pub fn on_tray(e: &TrayIconEvent) -> Option<Msg> {
    match e {
        TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => {
            let min = Pos2::new(rect.position.x as f32, rect.position.y as f32);
            Some(Msg::TogglePanel(Rect::from_min_size(min, Vec2::new(rect.size.width as f32, rect.size.height as f32))))
        }
        TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => Some(Msg::ShowMain),
        _ => None,
    }
}

pub fn tooltip(services: &[ServiceStatus]) -> String {
    let running: Vec<&str> = services.iter().filter(|s| s.health == Health::Running).map(|s| s.service.name.as_str()).collect();
    match running.len() {
        0 => "Zenv · nothing running".into(),
        n if n <= 3 => format!("Zenv · {}", running.join(", ")),
        n => format!("Zenv · {n} services running"),
    }
}

pub fn place(icon: Rect, work: Rect, panel: Vec2, scale: f32) -> Pos2 {
    let gap = GAP * scale;
    let below = icon.top() > work.center().y;
    let mut x = icon.center().x - panel.x / 2.0;
    let mut y = if below { work.bottom() - panel.y - gap } else { work.top() + gap };
    if icon.left() < work.left() || icon.left() > work.right() {
        x = if icon.left() < work.left() { work.left() + gap } else { work.right() - panel.x - gap };
        y = icon.center().y - panel.y / 2.0;
    }
    let x = x.clamp(work.left() + gap, (work.right() - panel.x - gap).max(work.left() + gap));
    let y = y.clamp(work.top() + gap, (work.bottom() - panel.y - gap).max(work.top() + gap));
    Pos2::new(x, y)
}
