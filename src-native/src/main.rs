#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use zenv_core::app_core::{self, Core};
use zenv_core::paths::{self, AppPaths};
use zenv_core::platform;
use zenv_native::app::ZenvApp;

const IDENTIFIER: &str = "dev.zenv.desktop";
const ICON_PNG: &[u8] = include_bytes!("../../src-tauri/icons/128x128.png");

fn main() {
    if let Err(e) = run() {
        platform::show_fatal_error("Zenv failed to start", &e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let app_local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join(IDENTIFIER);
    let paths = AppPaths::new(paths::resolve_home(&app_local));
    paths.ensure().map_err(|e| e.to_string())?;
    let _log = app_core::init_logging(&paths);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "native UI panicked");
        default_hook(info);
    }));
    let instance = match platform::single_instance(&zenv_native::os::instance_name(&paths.root)) {
        Ok(Some(instance)) => Some(instance),
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::warn!(error = %e, "single-instance check failed; starting anyway");
            None
        }
    };
    if let Err(e) = paths::adopt_legacy_database(&app_local, &paths) {
        tracing::warn!(error = %e, "could not copy earlier metadata into the Zenv home");
    }
    let core = Arc::new(Core::initialize(paths).map_err(|e| {
        tracing::error!(error = ?e, "core failed to start");
        e.to_string()
    })?);
    core.spawn_background();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Zenv")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(eframe::icon_data::from_png_bytes(ICON_PNG).map_err(|e| e.to_string())?),
        ..Default::default()
    };
    eframe::run_native("Zenv", options, Box::new(move |cc| Ok(Box::new(ZenvApp::new(cc, core, instance)))))
        .map_err(|e| e.to_string())
}
