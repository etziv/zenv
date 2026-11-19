use eframe::egui::{self, RichText, Ui};
use zenv_core::runtime_catalog::ComponentInfo;
use zenv_core::runtime_manager::{DiscoveredInstallation, Installation};

use crate::loader::{Loader, Task};
use crate::pages::Action;
use crate::theme::palette;
use crate::widgets::{self, Kind};

pub fn render(
    ui: &mut Ui,
    loader: &Loader<Vec<DiscoveredInstallation>>,
    components: &[ComponentInfo],
    installations: &[Installation],
    out: &mut Vec<Action>,
) {
    let p = palette(ui);
    match loader.get() {
        None => {
            ui.add_space(8.0);
            widgets::loading(ui, "Scanning this computer…");
        }
        Some(Err(e)) => {
            ui.add_space(8.0);
            widgets::error_panel(ui, e);
        }
        Some(Ok(results)) => {
            let relevant: Vec<&DiscoveredInstallation> = results.iter().filter(|r| components.iter().any(|c| c.id == r.component)).collect();
            ui.add_space(8.0);
            widgets::card(ui, p.primary.gamma_multiply(0.4)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(widgets::strong(format!("Found {} installation{} on this computer", relevant.len(), if relevant.len() == 1 { "" } else { "s" }), 13.0));
                if relevant.is_empty() {
                    ui.label(RichText::new("Nothing was found on PATH or in common install locations. Use \"Add from folder\" to point Zenv at a folder.").small().color(p.muted));
                    return;
                }
                for r in relevant {
                    let name = components.iter().find(|c| c.id == r.component).map(|c| c.name).unwrap_or("unknown");
                    let registered = installations.iter().any(|i| {
                        i.component == r.component && i.executable_path.to_string_lossy().eq_ignore_ascii_case(&r.executable_path.to_string_lossy())
                    });
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(widgets::strong(name, 12.5));
                                match &r.version {
                                    Some(v) => {
                                        widgets::badge(ui, v, widgets::Badge::Outline);
                                    }
                                    None => {
                                        widgets::badge(ui, "unverified", widgets::Badge::Warning);
                                    }
                                }
                            });
                            ui.label(widgets::mono(r.executable_path.display().to_string(), 10.5).color(p.muted));
                            if let Some(err) = &r.error {
                                ui.label(RichText::new(err).small().color(p.warning));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if registered {
                                ui.label(RichText::new("Registered").small().color(p.muted));
                            } else if widgets::button(ui, Kind::Outline, None, "Register", r.version.is_some()).clicked() {
                                let (component, path) = (r.component, r.executable_path.clone());
                                out.push(Action::Run(Task::new(
                                    format!("installation:register:{component}"),
                                    format!("Register {name}"),
                                    move |c| zenv_core::runtime_manager::register(&c.db, component, &path).map(|_| ()),
                                )));
                            }
                        });
                    });
                    ui.separator();
                }
            });
        }
    }
}
