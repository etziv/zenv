//! An AI router's dashboard (OmniRoute, 9router) in its own Zenv window, one
//! per router (no browser needed). The page is the router's own (a remote URL
//! on 127.0.0.1): it gets no access to Zenv.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use zenv_core::ai_router::RouterSpec;
use zenv_core::error::{AppError, AppResult};

/// Shows the router's dashboard window at `url` (navigating an open one).
pub fn show(app: &AppHandle, spec: &RouterSpec, url: &str) -> AppResult<()> {
    let parsed: tauri::Url = url.parse().map_err(|e| AppError::Validation(format!("bad dashboard address {url}: {e}")))?;
    // Window labels: letters, digits and - / : _ only.
    let label = spec.component.as_str();
    if let Some(w) = app.get_webview_window(label) {
        w.navigate(parsed).map_err(|e| AppError::Platform(e.to_string()))?;
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title(format!("{} · Zenv", spec.name))
        .inner_size(1280.0, 840.0)
        .min_inner_size(900.0, 600.0)
        .build()
        .map_err(|e| AppError::Platform(format!("cannot open the {} window: {e}", spec.name)))?;
    Ok(())
}
