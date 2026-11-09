//! An app's Studio in its own Zenv window: a coding agent's terminal next to
//! the app's live preview. One window per app, labelled `studio-<site id>`;
//! it runs Zenv's own page, so its terminals go through Zenv's commands.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use zenv_core::error::{AppError, AppResult};

pub const PREFIX: &str = "studio-";

pub fn is_studio(label: &str) -> bool {
    label.starts_with(PREFIX)
}

/// Shows the app's Studio window (focusing an open one).
pub fn show(app: &AppHandle, site_id: &str, domain: &str) -> AppResult<()> {
    // Window labels: letters, digits and - / : _ only (site ids are UUIDs).
    if !site_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::Validation(format!("bad app id {site_id}")));
    }
    let label = format!("{PREFIX}{site_id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("{domain} · Studio · Zenv"))
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 560.0)
        // Files dropped on the chat reach the page (Tauri would take them otherwise).
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| AppError::Platform(format!("cannot open the Studio window: {e}")))?;
    Ok(())
}
