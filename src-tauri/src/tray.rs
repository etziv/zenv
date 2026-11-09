//! Notification-area icon. Closing the main window keeps Zenv (and its
//! services) running here. A left click opens Zenv's own panel (a small
//! frameless window above the icon, hidden again when it loses focus); a
//! right click shows a short native menu (Open, Quit).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindowBuilder};

use zenv_core::app_core::Core;
use zenv_core::service_manager::Health;

const TRAY_ID: &str = "zenv";
pub const PANEL: &str = "tray";
/// Width, and the height before the panel sizes itself to its content.
const PANEL_SIZE: (f64, f64) = (300.0, 320.0);
const PANEL_HEIGHT: (f64, f64) = (150.0, 460.0);
/// Gap between the panel and the taskbar, in logical pixels.
const GAP: f64 = 10.0;

/// When the panel last hid itself on losing focus: a click on the icon
/// blurs the panel first, and must not reopen it straight away.
static LAST_BLUR: Mutex<Option<Instant>> = Mutex::new(None);
/// The panel sits above a bottom taskbar, so it grows upwards.
static GROWS_UP: AtomicBool = AtomicBool::new(true);

/// Shows and focuses the main window (and hides the panel).
pub fn show_main(app: &AppHandle) {
    hide_panel(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(p) = app.get_webview_window(PANEL) {
        let _ = p.hide();
    }
}

/// The panel lost focus: hide it.
pub fn panel_blurred(app: &AppHandle) {
    *LAST_BLUR.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
    hide_panel(app);
}

/// Places the panel above (or beside) the icon, inside the monitor's work
/// area, so it never covers the taskbar.
fn place(panel: &tauri::WebviewWindow, icon: &Rect) -> tauri::Result<()> {
    let scale = panel.scale_factor()?;
    let pos = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let outer = panel.outer_size()?;
    let (w, h) = (outer.width as f64, outer.height as f64);
    let gap = GAP * scale;
    let Some(monitor) = panel.app_handle().monitor_from_point(pos.x, pos.y)? else {
        return panel.set_position(PhysicalPosition::new(pos.x - w / 2.0, pos.y - h - gap));
    };
    let area = monitor.work_area();
    let (ax, ay) = (area.position.x as f64, area.position.y as f64);
    let (aw, ah) = (area.size.width as f64, area.size.height as f64);
    // Taskbar below (the usual case), above, or at a side.
    let mut x = pos.x + size.width / 2.0 - w / 2.0;
    let below = pos.y > ay + ah / 2.0;
    GROWS_UP.store(below, Ordering::Relaxed);
    let mut y = if below { ay + ah - h - gap } else { ay + gap };
    if pos.x < ax || pos.x > ax + aw {
        x = if pos.x < ax { ax + gap } else { ax + aw - w - gap };
        y = pos.y + size.height / 2.0 - h / 2.0;
    }
    x = x.clamp(ax + gap, ax + aw - w - gap);
    y = y.clamp(ay + gap, ay + ah - h - gap);
    panel.set_position(PhysicalPosition::new(x, y))
}

/// Fits the panel to its content (logical height from the page), keeping
/// the edge next to the taskbar in place.
pub fn resize_panel(app: &AppHandle, height: f64) {
    let Some(panel) = app.get_webview_window(PANEL) else { return };
    let height = height.clamp(PANEL_HEIGHT.0, PANEL_HEIGHT.1);
    let (Ok(before), Ok(pos)) = (panel.outer_size(), panel.outer_position()) else { return };
    let _ = panel.set_size(LogicalSize::new(PANEL_SIZE.0, height));
    if GROWS_UP.load(Ordering::Relaxed) {
        if let Ok(after) = panel.outer_size() {
            let y = pos.y + before.height as i32 - after.height as i32;
            let _ = panel.set_position(PhysicalPosition::new(pos.x, y));
        }
    }
}

fn toggle_panel(app: &AppHandle, icon: &Rect) {
    let Some(panel) = app.get_webview_window(PANEL) else { return };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    let just_blurred = LAST_BLUR.lock().unwrap_or_else(|p| p.into_inner()).is_some_and(|t| t.elapsed() < Duration::from_millis(300));
    if just_blurred {
        return;
    }
    let _ = place(&panel, icon);
    let _ = panel.show();
    let _ = panel.set_focus();
    let _ = app.emit_to(PANEL, "zenv://panel-shown", ());
}

fn tooltip(core: &Core) -> String {
    let running: Vec<String> = core
        .services
        .statuses()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.health == Health::Running)
        .map(|s| s.service.name)
        .collect();
    match running.len() {
        0 => "Zenv · nothing running".into(),
        n if n <= 3 => format!("Zenv · {}", running.join(", ")),
        n => format!("Zenv · {n} services running"),
    }
}

/// Creates the panel window and the icon, and keeps the icon's tooltip
/// current (services also change from the window, by crashing, or autostart).
pub fn install(app: &AppHandle, core: Arc<Core>) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html".into()))
        .title("Zenv")
        .inner_size(PANEL_SIZE.0, PANEL_SIZE.1)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // Windows draws a white frame around shadowed frameless windows;
        // the rounded corners and border come from DWM instead.
        .shadow(false)
        .background_color(tauri::window::Color(8, 9, 10, 255))
        .visible(false)
        .build()?;
    if let Some(panel) = app.get_webview_window(PANEL) {
        #[cfg(windows)]
        if let Ok(hwnd) = panel.hwnd() {
            zenv_core::platform::style_popup(hwnd.0 as isize, 0x2b2f32);
        }
        let _ = &panel;
    }

    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open Zenv", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Zenv (stops all services)", true, None::<&str>)?,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip(&core))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => {
                toggle_panel(tray.app_handle(), &rect)
            }
            TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => show_main(tray.app_handle()),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    let app = app.clone();
    std::thread::Builder::new()
        .name("tray-tooltip".into())
        .spawn(move || {
            let mut last = String::new();
            loop {
                std::thread::sleep(Duration::from_secs(3));
                let text = tooltip(&core);
                if text != last {
                    if let Some(tray) = app.tray_by_id(TRAY_ID) {
                        let _ = tray.set_tooltip(Some(&text));
                    }
                    last = text;
                }
            }
        })
        .ok();
    Ok(())
}
