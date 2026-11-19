use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};
use zenv_core::runtime_catalog::{Category, ComponentId, ComponentInfo};
use zenv_core::runtime_installer::AvailableVersion;
use zenv_core::runtime_manager::{InstallSource, Installation};
use zenv_core::service_manager::{Service, ServiceInput, ServiceStatus};

use super::Data;
use crate::loader::Task;
use crate::lucide;
use crate::pages::Action;
use crate::theme::palette;
use crate::widgets::{self, Kind};

pub struct InstallState {
    pub component: ComponentId,
    pub show_all: bool,
}

pub struct RegisterState {
    pub component: ComponentId,
    pub path: String,
}

pub struct ServiceState {
    pub category: Category,
    pub editing: Option<String>,
    pub name: String,
    pub installation_id: String,
    pub listen_address: String,
    pub port: String,
    pub https_port: String,
    pub autostart: bool,
    pub data_dir: String,
}

#[derive(Default)]
pub enum Dialog {
    #[default]
    None,
    Install(InstallState),
    Register(RegisterState),
    Service(ServiceState),
    Confirm(Confirm),
}

pub enum Confirm {
    Uninstall(Installation),
    DeleteService(ServiceStatus),
    InitData(ServiceStatus),
}

fn default_name_port(component: ComponentId) -> (&'static str, u16) {
    match component {
        ComponentId::Apache => ("Apache", 8080),
        ComponentId::Nginx => ("Nginx", 8080),
        ComponentId::Caddy => ("Caddy", 8080),
        ComponentId::Frankenphp => ("FrankenPHP", 8080),
        ComponentId::Mysql => ("MySQL", 3306),
        ComponentId::Postgresql => ("PostgreSQL", 5432),
        ComponentId::Redis => ("Redis", 6379),
        ComponentId::Omniroute => ("OmniRoute", 20128),
        ComponentId::Ninerouter => ("9router", 20128),
        _ => ("Server", 8080),
    }
}

fn needs_data_dir(component: ComponentId) -> bool {
    matches!(component, ComponentId::Mysql | ComponentId::Postgresql | ComponentId::Redis | ComponentId::Omniroute | ComponentId::Ninerouter)
}

impl Dialog {
    pub fn install(component: ComponentId) -> Self {
        Dialog::Install(InstallState { component, show_all: false })
    }

    pub fn register(component: ComponentId) -> Self {
        Dialog::Register(RegisterState { component, path: String::new() })
    }

    pub fn service_for(category: Category, inst: &Installation) -> Self {
        let (name, port) = default_name_port(inst.component);
        Dialog::Service(ServiceState {
            category,
            editing: None,
            name: name.to_owned(),
            installation_id: inst.id.clone(),
            listen_address: "127.0.0.1".to_owned(),
            port: port.to_string(),
            https_port: "443".to_owned(),
            autostart: false,
            data_dir: String::new(),
        })
    }

    pub fn edit_service(category: Category, s: &Service) -> Self {
        Dialog::Service(ServiceState {
            category,
            editing: Some(s.id.clone()),
            name: s.name.clone(),
            installation_id: s.installation_id.clone(),
            listen_address: s.listen_address.clone(),
            port: s.http_port.to_string(),
            https_port: s.https_port.unwrap_or(443).to_string(),
            autostart: s.autostart,
            data_dir: s.data_dir.clone().map(|p| p.display().to_string()).unwrap_or_default(),
        })
    }
}

pub fn show(ctx: &egui::Context, d: &Data, dialog: &mut Dialog, busy: &[String], out: &mut Vec<Action>) {
    let close = match dialog {
        Dialog::None => return,
        Dialog::Install(state) => install_dialog(ctx, state, d, out),
        Dialog::Register(state) => register_dialog(ctx, state, d, busy, out),
        Dialog::Service(state) => service_dialog(ctx, state, d, busy, out),
        Dialog::Confirm(confirm) => confirm_dialog(ctx, confirm, d, out),
    };
    if close {
        *dialog = Dialog::None;
    }
}

fn modal<T>(ctx: &egui::Context, id: &str, body: impl FnOnce(&mut Ui) -> T) -> (T, bool) {
    let r = widgets::modal(ctx, id).show(ctx, body);
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (r.inner, r.backdrop_response.clicked() || escape)
}

fn kind_noun(kind: Category) -> &'static str {
    match kind {
        Category::WebServer => "web server",
        Category::Database => "database server",
        Category::Cache => "cache server",
        _ => "service",
    }
}

fn confirm_dialog(ctx: &egui::Context, confirm: &Confirm, d: &Data, out: &mut Vec<Action>) -> bool {
    let muted = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(text).color(palette(ui).muted));
    };
    let path = |ui: &mut Ui, p: &std::path::Path| {
        ui.label(widgets::mono(p.display().to_string(), 12.0).color(palette(ui).foreground));
    };
    let choice = match confirm {
        Confirm::Uninstall(inst) => {
            let managed = inst.source == InstallSource::Managed;
            let label = format!("{} {}", inst.component, inst.version);
            let title = if managed { format!("Uninstall {label}?") } else { format!("Remove {label} from Zenv?") };
            widgets::confirm_modal(ctx, "confirm-uninstall", &title, if managed { "Uninstall" } else { "Remove" }, managed, false, |ui| {
                muted(ui, if managed { "Zenv installed this version and will delete its folder:" } else { "Zenv forgets this installation. Its files stay where they are:" });
                path(ui, &inst.root_path);
            })
        }
        Confirm::DeleteService(status) => {
            let s = &status.service;
            let noun = kind_noun(status.kind);
            let web = status.kind == Category::WebServer;
            widgets::confirm_modal(ctx, "confirm-delete-service", &format!("Delete {}?", s.name), &format!("Delete {noun}"), true, false, |ui| {
                let kept = if web { ", and so is the document root." } else { "." };
                muted(ui, &format!("Zenv removes this {noun}'s record and the configuration it generated. The installation and the logs are kept{kept}"));
                if let (false, Some(dir)) = (web, &s.data_dir) {
                    muted(ui, "The data folder is not deleted:");
                    path(ui, dir);
                }
            })
        }
        Confirm::InitData(status) => {
            let postgres = status.service.component == ComponentId::Postgresql;
            widgets::confirm_modal(ctx, "confirm-init-data", "Initialize the data folder?", "Initialize", false, false, |ui| {
                muted(ui, &format!("Zenv creates a new, empty {} data folder at:", if postgres { "PostgreSQL" } else { "MySQL" }));
                if let Some(dir) = &status.service.data_dir {
                    path(ui, dir);
                }
                let user = if postgres { "The postgres user" } else { "The root user" };
                muted(ui, &format!("{user} can connect from this computer without a password. This only works on an empty folder; existing data is never touched."));
            })
        }
    };
    match choice {
        widgets::ModalChoice::None => false,
        widgets::ModalChoice::Cancel => true,
        widgets::ModalChoice::Confirm => {
            out.push(confirmed_task(confirm, d));
            true
        }
    }
}

fn confirmed_task(confirm: &Confirm, d: &Data) -> Action {
    let task = match confirm {
        Confirm::Uninstall(inst) => {
            let id = inst.id.clone();
            if inst.source == InstallSource::Managed {
                Task::new(format!("installation:uninstall:{id}"), "Uninstall", move |c| c.installer.uninstall(&id))
            } else {
                Task::new(format!("installation:unregister:{id}"), "Remove", move |c| zenv_core::runtime_manager::unregister(&c.db, &id))
            }
        }
        Confirm::DeleteService(status) => {
            let id = status.service.id.clone();
            Task::new(format!("service:delete:{id}"), "Delete server", move |c| c.services.delete(&id).map(|_| ()))
        }
        Confirm::InitData(status) => {
            let id = status.service.id.clone();
            let notices = d.rows.borrow().notices.clone();
            Task::new(format!("service:init:{id}"), "Initialize", move |c| {
                c.services.initialize_data_dir(&id)?;
                notices.set(&id, super::rows::Notice::Success("Data folder initialized.".to_owned()));
                Ok(())
            })
        }
    };
    Action::Run(task.inline())
}

fn component_info<'a>(d: &Data<'a>, id: ComponentId) -> Option<&'a ComponentInfo> {
    d.components.iter().find(|c| c.id == id)
}

fn install_dialog(ctx: &egui::Context, state: &mut InstallState, d: &Data, out: &mut Vec<Action>) -> bool {
    let info = component_info(d, state.component);
    let name = info.map(|i| i.name).unwrap_or("this component");
    let mut close = false;
    let (_, backdrop) = modal(ctx, "install-dialog", |ui| {
        ui.set_width(472.0);
        close |= widgets::dialog_title(ui, "Install a version");
        ui.label(
            RichText::new("Downloaded from the official source and installed into the Zenv folder. Versions already on this computer keep working.")
                .small()
                .color(palette(ui).muted),
        );
        ui.add_space(8.0);

        for job in d.jobs.iter().filter(|j| j.component == state.component && super::detail::shown_job(j)) {
            super::detail::progress_row(ui, job, name);
        }

        match d.available {
            None => widgets::loading(ui, &format!("Fetching {name} releases…")),
            Some(loader) => match loader.get() {
                None => widgets::loading(ui, &format!("Fetching {name} releases…")),
                Some(Err(e)) => widgets::error_panel(ui, e),
                Some(Ok(versions)) => {
                    ui.horizontal_top(|ui| {
                        let muted = palette(ui).muted;
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.label(RichText::new(format!("{name} releases from")).size(12.0).color(muted));
                                ui.label(widgets::mono(versions.first().map_or("", |v| v.source), 11.5).color(muted));
                            });
                            if let Some(by) = versions.first().map(|v| v.verified_by).filter(|b| !b.is_empty()) {
                                ui.label(RichText::new(format!("Checked by: {by}")).size(12.0).color(muted));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.visuals_mut().selection.bg_fill = palette(ui).primary;
                            ui.checkbox(&mut state.show_all, RichText::new("Include pre-releases and older").size(12.0).color(muted));
                        });
                    });
                    let shown: Vec<&AvailableVersion> =
                        versions.iter().filter(|v| state.show_all || v.stable).take(if state.show_all { 200 } else { 25 }).collect();
                    let installing = d.jobs.iter().any(|j| j.component == state.component && super::detail::is_active(j));
                    let p = palette(ui);
                    ui.add_space(4.0);
                    egui::Frame::new().stroke(egui::Stroke::new(1.0, p.border)).corner_radius(egui::CornerRadius::same(crate::theme::RADIUS)).show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(288.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for (i, v) in shown.into_iter().enumerate() {
                                if i > 0 {
                                    let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                                    ui.painter().rect_filled(line, 0.0, p.border);
                                }
                                egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(28.0);
                                        ui.label(super::detail::mono_strong(&v.version, 13.0));
                                        if !v.stable {
                                            widgets::badge(ui, "pre-release", widgets::Badge::Warning);
                                        }
                                        if v.size > 0 {
                                            ui.label(RichText::new(format_bytes(v.size)).size(12.0).color(p.muted));
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if v.installed_id.is_some() {
                                                ui.label(RichText::new("Installed").size(12.0).color(p.muted));
                                                ui.label(widgets::icon(lucide::CHECK, 13.0).color(p.muted));
                                            } else if widgets::button_sm(ui, Kind::Outline, Some(lucide::DOWNLOAD), "Install", !installing).clicked() {
                                                let (component, version) = (state.component, v.version.clone());
                                                out.push(Action::Run(Task::new(
                                                    format!("installer:install:{component}:{version}"),
                                                    format!("Install {name} {version}"),
                                                    move |c| c.installer.start(component, &version).map(|_| ()),
                                                )));
                                            }
                                        });
                                    });
                                });
                            }
                        });
                    });
                }
            },
        }

    });
    close || backdrop
}

fn register_dialog(ctx: &egui::Context, state: &mut RegisterState, d: &Data, busy: &[String], out: &mut Vec<Action>) -> bool {
    let info = component_info(d, state.component);
    let name = info.map(|i| i.name).unwrap_or("installation");
    let key = format!("installation:register:{}", state.component);
    let pending = busy.iter().any(|k| k == &key);
    let mut close = false;
    let (_, backdrop) = modal(ctx, "register-dialog", |ui| {
        ui.set_min_width(380.0);
        close |= widgets::dialog_title(ui, "Register an existing installation");
        ui.label(
            RichText::new("Zenv runs the program's own version command to verify it, then records where it lives. The files stay where they are and are never modified.")
                .small()
                .color(palette(ui).muted),
        );
        ui.add_space(8.0);
        ui.label(widgets::strong("Component", 12.0));
        widgets::select(ui, "register-component", name, 300.0, |ui| {
            for c in d.components {
                ui.selectable_value(&mut state.component, c.id, c.name);
            }
        });
        ui.add_space(8.0);
        ui.label(widgets::strong("Installation folder or executable", 12.0));
        ui.horizontal(|ui| {
            widgets::text_input(ui, &mut state.path, r"C:\path\to\installation", 260.0, None);
            if widgets::button(ui, Kind::Outline, Some(lucide::FOLDER_OPEN), "Browse", true).clicked() {
                if let Some(picked) = rfd::FileDialog::new().set_title("Select the installation folder").pick_folder() {
                    state.path = picked.display().to_string();
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, Kind::Outline, None, "Cancel", true).clicked() {
                close = true;
            }
            let path_ok = !state.path.trim().is_empty();
            if widgets::button(ui, Kind::Primary, None, if pending { "Verifying…" } else { "Register" }, path_ok && !pending).clicked() {
                let (component, path) = (state.component, state.path.trim().to_owned());
                out.push(Action::Run(
                    Task::new(key.clone(), "Register installation", move |c| {
                        zenv_core::runtime_manager::register(&c.db, component, &PathBuf::from(path)).map(|_| ())
                    })
                    .inline(),
                ));
            }
        });
        if let Some(e) = d.errors.get(&key) {
            ui.add_space(6.0);
            widgets::error_panel(ui, e);
        }
    });
    close || backdrop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_matches_the_react_helper() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10 MB");
        assert_eq!(format_bytes(23 * 1024 * 1024 + 300_000), "23 MB");
    }

    #[test]
    fn default_name_and_port_match_known_components() {
        assert_eq!(default_name_port(ComponentId::Mysql), ("MySQL", 3306));
        assert_eq!(default_name_port(ComponentId::Apache), ("Apache", 8080));
        assert_eq!(default_name_port(ComponentId::Php), ("Server", 8080));
    }

    #[test]
    fn needs_data_dir_only_for_stateful_components() {
        assert!(needs_data_dir(ComponentId::Mysql));
        assert!(needs_data_dir(ComponentId::Redis));
        assert!(!needs_data_dir(ComponentId::Apache));
        assert!(!needs_data_dir(ComponentId::Php));
    }
}

fn service_dialog(ctx: &egui::Context, state: &mut ServiceState, d: &Data, busy: &[String], out: &mut Vec<Action>) -> bool {
    let candidates: Vec<&Installation> = d.installations.iter().filter(|i| d.components.iter().any(|c| c.id == i.component)).collect();
    let is_web = state.category == Category::WebServer;
    let needs_data = d
        .installations
        .iter()
        .find(|i| i.id == state.installation_id)
        .is_some_and(|i| needs_data_dir(i.component));
    let editing = state.editing.clone();
    let key = editing.as_ref().map(|id| format!("service:update:{id}")).unwrap_or_else(|| format!("service:create:{}", state.installation_id));
    let pending = busy.iter().any(|k| k == &key);
    let port_number: Option<u16> = state.port.parse().ok();
    let https_number: Option<u16> = state.https_port.parse().ok();
    let valid = port_number.is_some() && (!is_web || (https_number.is_some() && https_number != port_number));
    let mut close = false;

    let (_, backdrop) = modal(ctx, "service-dialog", |ui| {
        ui.set_min_width(420.0);
        let title = if editing.is_some() { format!("Edit {}", state.name) } else { "Add a server".to_owned() };
        close |= widgets::dialog_title(ui, title);
        ui.add_space(6.0);

        ui.label(widgets::strong("Name", 12.0));
        widgets::text_input(ui, &mut state.name, "", 300.0, None);

        ui.add_space(6.0);
        ui.label(widgets::strong("Installation", 12.0));
        let installation_label = candidates.iter().find(|i| i.id == state.installation_id).map(|i| format!("{} {}", i.component, i.version)).unwrap_or_default();
        widgets::select(ui, "service-installation", &installation_label, 300.0, |ui| {
            for i in &candidates {
                ui.selectable_value(&mut state.installation_id, i.id.clone(), format!("{} {} — {}", i.component, i.version, i.root_path.display()));
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(widgets::strong("Listen on", 12.0));
                let addr = state.listen_address.clone();
                widgets::select(ui, "service-address", &addr, 140.0, |ui| {
                    for addr in ["127.0.0.1", "0.0.0.0", "::1"] {
                        ui.selectable_value(&mut state.listen_address, addr.to_owned(), addr);
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label(widgets::strong("HTTP port", 12.0));
                widgets::text_input(ui, &mut state.port, "", 80.0, None);
            });
            if is_web {
                ui.vertical(|ui| {
                    ui.label(widgets::strong("HTTPS port", 12.0));
                    widgets::text_input(ui, &mut state.https_port, "", 80.0, None);
                });
            }
        });

        if needs_data {
            ui.add_space(6.0);
            ui.label(widgets::strong("Data folder", 12.0));
            ui.horizontal(|ui| {
                widgets::text_input(ui, &mut state.data_dir, "Leave empty to keep it inside Zenv's data folder", 260.0, None);
                if widgets::button(ui, Kind::Outline, Some(lucide::FOLDER_OPEN), "Browse", true).clicked() {
                    if let Some(picked) = rfd::FileDialog::new().set_title("Select the data folder").pick_folder() {
                        state.data_dir = picked.display().to_string();
                    }
                }
            });
        }

        ui.add_space(6.0);
        ui.checkbox(&mut state.autostart, "Start automatically when Zenv opens");

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, Kind::Outline, None, "Cancel", true).clicked() {
                close = true;
            }
            let label = if editing.is_some() { "Save changes" } else { "Add server" };
            if widgets::button(ui, Kind::Primary, None, label, valid && !pending && !state.installation_id.is_empty()).clicked() {
                let input = ServiceInput {
                    name: state.name.clone(),
                    installation_id: state.installation_id.clone(),
                    listen_address: state.listen_address.clone(),
                    http_port: port_number.unwrap_or(0),
                    autostart: state.autostart,
                    data_dir: needs_data.then(|| state.data_dir.trim().to_owned()).filter(|s| !s.is_empty()),
                    https_port: is_web.then(|| https_number.unwrap_or(443)),
                };
                let job_key = key.clone();
                out.push(Action::Run(match editing.clone() {
                    Some(id) => Task::new(job_key, "Save server", move |c| c.update_service(&id, &input).map(|_| ())),
                    None => Task::new(job_key, "Add server", move |c| c.services.create(&input).map(|_| ())),
                }));
                close = true;
            }
        });
    });
    close || backdrop
}

pub(super) fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < 2 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if value < 10.0 { 1 } else { 0 };
    format!("{value:.digits$} {}", ["KB", "MB", "GB"][unit])
}
