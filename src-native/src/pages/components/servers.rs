use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};
use zenv_core::runtime_catalog::{Category, ComponentId, ComponentInfo};
use zenv_core::runtime_manager::Installation;
use zenv_core::service_manager::{Health, ServiceStatus};
use zenv_core::web_server::DataDirState;

use super::detail::{installation_error, installation_items, mono_strong, path_button, run_inline, service_error, source_badge};
use super::dialogs::{Confirm, Dialog};
use super::rows::{self, Notice};
use super::Data;
use crate::loader::{Note, Task, Tone};
use crate::lucide;
use crate::pages::{dashboard, tools, Action};
use crate::theme::palette;
use crate::widgets::{self, Kind, SwitchState};

pub fn list(ui: &mut Ui, d: &Data, current: &ComponentInfo, versions: &[&Installation], dialog: &mut Dialog, busy: &[String], out: &mut Vec<Action>) {
    for &inst in versions {
        let servers: Vec<&ServiceStatus> = d.services.iter().filter(|s| s.service.installation_id == inst.id).collect();
        if servers.is_empty() {
            version_row(ui, d, inst, current, dialog, out);
        } else {
            for s in &servers {
                server_row(ui, d, s, inst, current, dialog, busy, out);
            }
        }
        ui.add_space(4.0);
    }
}

pub fn address_text(status: &ServiceStatus) -> String {
    let mut t = format!("{}{}", status.service.listen_address, dashboard::port_text(status));
    if status.kind != Category::WebServer {
        if let Some(dir) = &status.service.data_dir {
            t.push_str(&format!(" · {}", dir.display()));
        }
    }
    t
}

fn version_row(ui: &mut Ui, d: &Data, inst: &Installation, current: &ComponentInfo, dialog: &mut Dialog, out: &mut Vec<Action>) {
    let p = palette(ui);
    let frame = egui::Frame::new()
        .fill(p.card.gamma_multiply(0.6))
        .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS + 2))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(mono_strong(&inst.version, 13.5));
                        source_badge(ui, inst);
                    });
                    ui.label(widgets::mono(inst.root_path.display().to_string(), 11.0).color(p.muted));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let trigger = widgets::icon_button(ui, lucide::MORE_HORIZONTAL, &format!("More for {}", inst.version));
                    egui::Popup::menu(&trigger).show(|ui| {
                        if widgets::menu_item(ui, lucide::PLUS, format!("Set up a server on {}", inst.version), inst.executable_exists, false) {
                            *dialog = Dialog::service_for(current.category, inst);
                            ui.close();
                        }
                        ui.separator();
                        installation_items(ui, inst, false, inst.component == ComponentId::Frankenphp, dialog, out);
                    });
                    if widgets::button_sm(ui, Kind::Outline, Some(lucide::PLUS), "Set up server", inst.executable_exists).clicked() {
                        *dialog = Dialog::service_for(current.category, inst);
                    }
                    path_button(ui, d, inst, out);
                });
            });
            if let Some(e) = installation_error(d.errors, &inst.id) {
                ui.add_space(4.0);
                widgets::error_panel(ui, e);
            }
        });
    widgets::dashed_rect(ui.painter(), frame.response.rect.shrink(0.5), p.border);
}

struct DataState {
    needs_init: bool,
    blocked: bool,
    initialized: bool,
}

fn data_state(status: &ServiceStatus) -> DataState {
    let state = status.data_dir_info.as_ref().map(|i| i.state);
    DataState {
        needs_init: status.needs_initialization && matches!(state, Some(DataDirState::Missing | DataDirState::Empty)),
        blocked: matches!(state, Some(DataDirState::Foreign | DataDirState::Incompatible)),
        initialized: state == Some(DataDirState::Initialized),
    }
}

fn server_row(ui: &mut Ui, d: &Data, status: &ServiceStatus, inst: &Installation, current: &ComponentInfo, dialog: &mut Dialog, busy: &[String], out: &mut Vec<Action>) {
    let p = palette(ui);
    let s = &status.service;
    let web = status.kind == Category::WebServer;
    let running = status.health == Health::Running;
    let failed = status.health == Health::Failed;
    let data = data_state(status);
    let key = dashboard::service_key(&s.id);
    let pending = busy.contains(&key);
    let border = if running { p.primary.gamma_multiply(0.45) } else if failed { p.destructive.gamma_multiply(0.5) } else { p.border };
    widgets::card(ui, border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let label = if running { format!("Stop {}", s.name) } else { format!("Start {}", s.name) };
            let can_switch = running || (inst.executable_exists && !data.needs_init && !data.blocked);
            let r = ui.add_enabled_ui(can_switch, |ui| widgets::status_switch(ui, &SwitchState { on: running, failed, pending }, &label)).inner;
            let r = if data.needs_init { r.on_disabled_hover_text("Initialize the data folder first") } else { r };
            if r.clicked() {
                d.rows.borrow().notices.clear(&s.id);
                let id = s.id.clone();
                let task = if running {
                    Task::new(key.clone(), label, move |c| c.stop_service(&id).map(|_| ()))
                } else {
                    Task::new(key.clone(), label, move |c| c.start_service(&id).map(|_| ()))
                };
                out.push(Action::Run(task.inline()));
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(widgets::strong(&s.name, 13.5)).truncate());
                    ui.label(widgets::mono(&inst.version, 11.0).color(p.muted));
                    source_badge(ui, inst);
                    path_button(ui, d, inst, out);
                });
                ui.label(widgets::mono(address_text(status), 11.0).color(p.muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                service_menu(ui, d, status, inst, current, dialog, out);
                if !running && data.needs_init {
                    let init_key = format!("service:init:{}", s.id);
                    let initializing = busy.contains(&init_key);
                    let text = if initializing { "Initializing…" } else { "Initialize" };
                    if widgets::button_sm(ui, Kind::Primary, Some(lucide::DATABASE), text, !initializing).clicked() {
                        *dialog = Dialog::Confirm(Confirm::InitData(status.clone()));
                    }
                }
                if let (false, true, Some(conn)) = (web, data.initialized, &status.connection) {
                    let mut tip = format!("Copy {}", conn.url);
                    if let Some(user) = &conn.user {
                        let pass = if conn.password.as_deref() == Some("") { "(no password)" } else { "••••" };
                        tip.push_str(&format!("\n{user} / {pass}"));
                    }
                    if widgets::icon_button(ui, lucide::COPY, "Copy connection URL").on_hover_text(tip).clicked() {
                        ui.ctx().copy_text(conn.url.clone());
                        out.push(Action::Toast(Note::new(Tone::Success, "Connection URL copied", conn.url.clone())));
                    }
                }
                if running && web && widgets::icon_button(ui, lucide::EXTERNAL_LINK, "Open in browser").on_hover_text(format!("Open {}", status.url)).clicked() {
                    out.push(Action::OpenUrl(status.url.clone()));
                }
            });
        });
        notices(ui, d, status, inst, &data);
        if let Some(details) = d.rows.borrow_mut().details.get_mut(&s.id) {
            separator(ui);
            rows::details_panel(ui, details);
        }
        if tools::is_router(s.component) {
            separator(ui);
            d.routers.borrow_mut().panel(ui, status, busy, out);
        }
    });
}

fn separator(ui: &mut Ui) {
    let line = ui.available_rect_before_wrap();
    ui.painter().hline((line.left() - 14.0)..=(line.right() + 14.0), line.top() + 2.0, egui::Stroke::new(1.0, palette(ui).border));
    ui.add_space(14.0);
}

fn notice(ui: &mut Ui, color: egui::Color32, glyph: Option<&str>, text: &str) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.05))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if let Some(g) = glyph {
                    ui.label(widgets::icon(g, 12.0).color(color));
                }
                ui.label(RichText::new(text).size(12.0).color(if glyph.is_some() { palette(ui).foreground } else { color }));
            });
        });
}

fn notices(ui: &mut Ui, d: &Data, status: &ServiceStatus, inst: &Installation, data: &DataState) {
    let p = palette(ui);
    let s = &status.service;
    let stored = d.rows.borrow().notices.get(&s.id);
    let error = service_error(d.errors, &s.id).or_else(|| installation_error(d.errors, &inst.id));
    if !(data.needs_init || data.blocked || stored.is_some() || error.is_some()) {
        return;
    }
    ui.add_space(6.0);
    ui.spacing_mut().item_spacing.y = 8.0;
    if data.needs_init {
        notice(ui, p.warning, Some(lucide::TRIANGLE_ALERT), "The data folder is not initialized yet. Initialize it once, then switch the server on.");
    }
    if data.blocked {
        let detail = status.data_dir_info.as_ref().map(|i| i.detail.as_str()).unwrap_or_default();
        notice(ui, p.destructive, Some(lucide::TRIANGLE_ALERT), &format!("Zenv will not use this data folder: {detail}"));
    }
    match stored {
        Some(Notice::Success(text)) => notice(ui, p.success, None, &text),
        Some(Notice::Failed(output)) => widgets::error_panel(ui, &format!("The configuration check failed\n{output}")),
        None => {}
    }
    if let Some(e) = error {
        widgets::error_panel(ui, e);
    }
}

fn reveal(service_id: &str, path: PathBuf, select: bool) -> Action {
    Action::Run(Task::new(format!("service:reveal:{service_id}"), "Open folder", move |_| crate::os::reveal(&path, select)).inline())
}

fn service_menu(ui: &mut Ui, d: &Data, status: &ServiceStatus, inst: &Installation, current: &ComponentInfo, dialog: &mut Dialog, out: &mut Vec<Action>) {
    let running = status.health == Health::Running;
    let web = status.kind == Category::WebServer;
    let blocked = data_state(status).blocked;
    let s = status.service.clone();
    let notices = d.rows.borrow().notices.clone();
    let expanded = d.rows.borrow().details.contains_key(&s.id);
    let trigger = widgets::icon_button(ui, lucide::MORE_HORIZONTAL, &format!("More for {}", s.name));
    egui::Popup::menu(&trigger).show(|ui| {
        if running && web && widgets::menu_item(ui, lucide::REFRESH_CW, "Reload configuration", true, false) {
            notices.clear(&s.id);
            let id = s.id.clone();
            out.push(run_inline(format!("service:reload:{id}"), "Reload", move |c| c.services.reload(&id).map(|_| ())));
            ui.close();
        }
        if running && widgets::menu_item(ui, lucide::ROTATE_CCW, "Restart", true, false) {
            notices.clear(&s.id);
            let id = s.id.clone();
            out.push(run_inline(format!("service:restart:{id}"), "Restart", move |c| c.restart_service(&id).map(|_| ())));
            ui.close();
        }
        if running && !web && widgets::menu_item(ui, lucide::ACTIVITY, "Test connection", true, false) {
            let (id, notices) = (s.id.clone(), notices.clone());
            out.push(run_inline(format!("service:probe:{id}"), "Test connection", move |c| {
                let message = c.services.probe(&id)?;
                notices.set(&id, Notice::Success(message));
                Ok(())
            }));
            ui.close();
        }
        if !blocked && widgets::menu_item(ui, lucide::SHIELD_CHECK, "Check configuration", true, false) {
            let (id, notices) = (s.id.clone(), notices.clone());
            out.push(run_inline(format!("service:validate:{id}"), "Check configuration", move |c| {
                let report = c.services.validate(&id)?;
                notices.set(&id, if report.ok { Notice::Success("Configuration is valid.".to_owned()) } else { Notice::Failed(report.output) });
                Ok(())
            }));
            ui.close();
        }
        if widgets::menu_item(ui, lucide::CHEVRON_DOWN, if expanded { "Hide logs & activity" } else { "Logs & activity" }, true, false) {
            d.rows.borrow_mut().toggle_details(&s.id);
            ui.close();
        }
        ui.separator();
        if widgets::menu_item(ui, lucide::FILE_COG, "Configuration file", true, false) {
            out.push(reveal(&s.id, status.config_path.clone(), true));
            ui.close();
        }
        if web {
            if widgets::menu_item(ui, lucide::FOLDER_OPEN, "Document root", true, false) {
                out.push(reveal(&s.id, status.document_root.clone(), false));
                ui.close();
            }
        } else if let Some(dir) = &s.data_dir {
            if widgets::menu_item(ui, lucide::FOLDER_OPEN, "Data folder", true, false) {
                out.push(reveal(&s.id, dir.clone(), false));
                ui.close();
            }
        }
        ui.separator();
        let stop_first = if running { " (stop it first)" } else { "" };
        if widgets::menu_item(ui, lucide::PENCIL, format!("Edit{stop_first}"), !running, false) {
            *dialog = Dialog::edit_service(current.category, &s);
            ui.close();
        }
        if widgets::menu_item(ui, lucide::TRASH_2, format!("Delete server{stop_first}"), !running, true) {
            *dialog = Dialog::Confirm(Confirm::DeleteService(status.clone()));
            ui.close();
        }
        ui.separator();
        if widgets::menu_item(ui, lucide::PLUS, format!("Another server on {}", inst.version), true, false) {
            *dialog = Dialog::service_for(current.category, inst);
            ui.close();
        }
        installation_items(ui, inst, false, inst.component == ComponentId::Frankenphp, dialog, out);
    });
}
