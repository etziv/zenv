mod commands;
mod router_window;
mod studio_window;
mod tray;

use std::sync::Arc;

use tauri::Manager;

use zenv_core::app_core::{self, Core};
use zenv_core::paths::AppPaths;
use zenv_core::platform;

struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

pub fn run() {
    let app = tauri::Builder::default()
        // A second launch (Start menu while Zenv sits in the tray) shows the
        // running window instead of starting another Zenv.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_local = app.path().app_local_data_dir()?;
            let paths = AppPaths::new(zenv_core::paths::resolve_home(&app_local));
            paths.ensure()?;
            app.manage(LogGuard(app_core::init_logging(&paths)));
            if let Err(e) = zenv_core::paths::adopt_legacy_database(&app_local, &paths) {
                tracing::warn!(error = %e, "could not copy earlier metadata into the Zenv home");
            }
            let core = Arc::new(Core::initialize(paths).inspect_err(|e| {
                tracing::error!(error = %e, "initialization failed");
            })?);
            app.manage(core.clone());
            tray::install(app.handle(), core.clone())?;
            let updater = core.clone();
            let tailnet = Arc::downgrade(&core);
            std::thread::spawn(move || core.start_autostart_services());
            // A re-rolled tailnet name or renamed PC is served without opening the tailnet panel.
            std::thread::Builder::new()
                .name("tailnet-names".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let Some(core) = tailnet.upgrade() else { return };
                    if let Err(e) = core.follow_tailnet_names() {
                        tracing::warn!(error = %e, "tailnet names not followed");
                    }
                })
                .ok();
            // AI router auto-update (OmniRoute, 9router): shortly after start,
            // then every six hours.
            std::thread::Builder::new()
                .name("router-update".into())
                .spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(90));
                    loop {
                        for component in updater.routers_due_for_update() {
                            if let Err(e) = updater.router_update(component, false) {
                                tracing::warn!(%component, error = %e, "auto-update failed");
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
                    }
                })
                .ok();
            Ok(())
        })
        // Closing the window keeps Zenv and its services running in the tray;
        // the tray panel hides when it loses focus.
        .on_window_event(|window, event| match event {
            // A Studio window really closes, ending its terminals.
            tauri::WindowEvent::Destroyed if studio_window::is_studio(window.label()) => {
                if let Some(core) = window.app_handle().try_state::<Arc<Core>>() {
                    core.studio.close_owned_by(window.label());
                    core.chats.close_owned_by(window.label());
                }
            }
            tauri::WindowEvent::CloseRequested { .. } if studio_window::is_studio(window.label()) => {}
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(false) if window.label() == tray::PANEL => tray::panel_blurred(window.app_handle()),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::catalog_list,
            commands::installations_list,
            commands::installations_discover,
            commands::installation_register,
            commands::installation_unregister,
            commands::installation_set_default,
            commands::installation_refresh,
            commands::installation_set_enabled,
            commands::runtimes_status,
            commands::installer_available,
            commands::installer_install,
            commands::installer_jobs,
            commands::installation_uninstall,
            commands::open_terminal,
            commands::services_list,
            commands::service_create,
            commands::service_update,
            commands::service_delete,
            commands::service_validate,
            commands::service_start,
            commands::service_stop,
            commands::service_restart,
            commands::service_reload,
            commands::service_events,
            commands::service_initialize_data,
            commands::service_probe,
            commands::service_log_sources,
            commands::service_log_read,
            commands::app_log_read,
            commands::port_inspect,
            commands::diagnostics_run,
            commands::reveal,
            commands::open_service_url,
            commands::sites_list,
            commands::site_get,
            commands::site_detect,
            commands::site_create,
            commands::site_update,
            commands::site_set_enabled,
            commands::site_duplicate,
            commands::site_delete,
            commands::site_diagnose,
            commands::site_log_read,
            commands::site_log_sources,
            commands::runtime_log_groups,
            commands::runtime_log_read,
            commands::sites_reapply,
            commands::open_site_url,
            commands::hosts_status,
            commands::hosts_apply,
            commands::tls_status,
            commands::tls_create_ca,
            commands::tls_trust,
            commands::tls_untrust,
            commands::site_thumbnail,
            commands::component_icon,
            commands::services_for_apps,
            commands::start_services_for_apps,
            commands::site_thumbnail_capture,
            commands::router_status,
            commands::router_update,
            commands::router_set_auto_update,
            commands::router_open,
            commands::router_models,
            commands::docker_containers,
            commands::docker_engine_start,
            commands::docker_app_create,
            commands::codex_set_model,
            commands::codex_status,
            commands::codex_setup,
            commands::codex_set_default,
            commands::codex_snippet,
            commands::path_status,
            commands::path_add,
            commands::path_remove,
            commands::tailnet_status,
            commands::tailnet_apps,
            commands::tailnet_open,
            commands::tailnet_close,
            commands::tailnet_open_url,
            commands::stop_all_services,
            commands::panel_open_main,
            commands::panel_hide,
            commands::panel_resize,
            commands::app_quit,
            commands::app_process_status,
            commands::app_process_start,
            commands::app_process_stop,
            commands::app_command_set,
            commands::app_port_suggest,
            commands::studio_open,
            commands::editors_list,
            commands::project_list,
            commands::project_read,
            commands::project_write,
            commands::project_create,
            commands::project_stat,
            commands::project_search,
            commands::project_format,
            commands::formatter_install,
            commands::site_open_in_editor,
            commands::studio_agents,
            commands::terminal_open,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
            commands::chat_open,
            commands::chat_send,
            commands::chat_answer,
            commands::chat_interrupt,
            commands::chat_close,
            commands::chat_control,
            commands::open_claude_docs,
            commands::chat_catalog,
            commands::chat_sessions,
            commands::chat_history,
        ])
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(e) => {
            platform::show_fatal_error("Zenv failed to start", &e.to_string());
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(core) = handle.try_state::<Arc<Core>>() {
                core.shutdown();
            }
        }
    });
}
