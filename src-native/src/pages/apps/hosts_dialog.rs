use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText, Ui};
use zenv_core::domain_manager::hosts::ApplyMethod;
use zenv_core::error::AppError;
use zenv_core::local_sites::HostsStatus;

use super::{AppsPage, Overlay};
use crate::loader::Task;
use crate::lucide;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind};

const HOSTS_APPLY_KEY: &str = "hosts:apply";

#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    Done { unchanged: bool, backup: Option<PathBuf> },
    Declined,
}

#[derive(Clone, Default)]
pub struct HostsApply(Arc<Mutex<Option<Applied>>>);

impl HostsApply {
    pub fn get(&self) -> Option<Applied> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set(&self, applied: Applied) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(applied);
    }

    fn task(&self) -> Task {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = None;
        let slot = self.clone();
        Task::new(HOSTS_APPLY_KEY, "Update hosts file", move |c| match c.sites.hosts_apply() {
            Ok(o) => {
                slot.set(Applied::Done { unchanged: o.method == ApplyMethod::Unchanged, backup: o.backup });
                Ok(())
            }
            Err(AppError::ElevationDeclined(_)) => {
                slot.set(Applied::Declined);
                Ok(())
            }
            Err(e) => Err(e),
        })
        .inline()
    }
}

pub fn banner(ui: &mut Ui, page: &mut AppsPage, hosts: &HostsStatus) {
    let p = palette(ui);
    let plan = &hosts.plan;
    if plan.add.is_empty() && plan.remove.is_empty() {
        return;
    }
    let text = if !plan.add.is_empty() {
        format!("{} domain{} still need{} a hosts-file entry before {} work{} in your browser.", plan.add.len(), if plan.add.len() == 1 { "" } else { "s" }, if plan.add.len() == 1 { "s" } else { "" }, if plan.add.len() == 1 { "it" } else { "they" }, if plan.add.len() == 1 { "s" } else { "" })
    } else {
        format!("The hosts file still lists {} domain{} of removed or switched-off apps.", plan.remove.len(), if plan.remove.len() == 1 { "" } else { "s" })
    };
    widgets::card(ui, p.warning.gamma_multiply(0.4)).fill(p.warning.gamma_multiply(0.05)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(widgets::icon(lucide::TRIANGLE_ALERT, 16.0).color(p.warning));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button_sm(ui, Kind::Outline, None, "Review & update hosts file", true).clicked() {
                    page.overlay = Some(Overlay::Hosts(HostsApply::default()));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(text).size(14.0)).wrap());
                });
            });
        });
    });
}

fn mixed(ui: &mut Ui, parts: &[(&str, bool)], size: f32, color: Color32) {
    let mut job = egui::text::LayoutJob::default();
    for &(text, mono) in parts {
        let rt = if mono { widgets::mono(text, size - 1.0) } else { RichText::new(text).size(size) };
        rt.color(color).append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn boxed(ui: &mut Ui, fill: Color32, stroke: Color32, margin: i8, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(crate::theme::RADIUS))
        .inner_margin(egui::Margin::same(margin))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

pub fn dialog(ui: &mut Ui, page: &mut AppsPage, hosts: &HostsStatus, apply: &HostsApply, cx: &mut Cx) {
    let pending = cx.busy.iter().any(|k| k == HOSTS_APPLY_KEY);
    let applied = apply.get();
    let done = matches!(applied, Some(Applied::Done { .. }));
    let plan = &hosts.plan;
    let p = palette(ui);
    let resp = widgets::modal(ui.ctx(), "hosts-dialog").show(ui.ctx(), |ui| {
        ui.set_width(536.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        if widgets::dialog_title(ui, "Update the hosts file") && !pending {
            page.overlay = None;
        }
        mixed(ui, &[("Domains like ", false), ("shop.test", true), (" only reach this computer through the Windows hosts file. Zenv changes nothing but its own marked section, and saves a backup first.", false)], 14.0, p.muted);
        ui.add_space(12.0);
        if !plan.add.is_empty() {
            ui.label(widgets::medium("Add", 14.0));
            for e in &plan.add {
                ui.label(widgets::mono(format!("{} → {}", e.ip, e.domain), 12.0));
            }
            ui.add_space(8.0);
        }
        if !plan.remove.is_empty() {
            ui.label(widgets::medium("Remove (sites deleted or disabled)", 14.0));
            for e in &plan.remove {
                ui.label(widgets::mono(format!("{} → {}", e.ip, e.domain), 12.0).color(p.muted).strikethrough());
            }
            ui.add_space(8.0);
        }
        if !plan.conflicts.is_empty() {
            boxed(ui, p.warning.gamma_multiply(0.05), p.warning.gamma_multiply(0.4), 10, |ui| {
                ui.label(widgets::medium("Left alone: your own lines point these domains elsewhere", 12.0).color(p.warning));
                for c in &plan.conflicts {
                    ui.label(widgets::mono(format!("{} → {}", c.domain, c.mapped_to), 12.0));
                }
            });
            ui.add_space(8.0);
        }
        let path = hosts.path.display().to_string();
        mixed(ui, &[("Zenv's section of ", false), (&path, true), (" will read:", false)], 12.0, p.muted);
        boxed(ui, p.muted_bg.gamma_multiply(0.4), p.border, 8, |ui| {
            egui::ScrollArea::vertical().max_height(144.0).show(ui, |ui| {
                let text = if plan.block.is_empty() { "(removed — no domains need it)".to_owned() } else { plan.block.join("\n") };
                ui.label(widgets::mono(text, 11.5));
            });
        });
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            ui.label(widgets::icon(lucide::SHIELD_CHECK, 16.0).color(p.muted));
            ui.add(egui::Label::new(RichText::new("Windows will ask for administrator permission for this one change. Zenv itself keeps running without it.").size(12.0).color(p.muted)).wrap());
        });

        match &applied {
            Some(Applied::Done { unchanged, backup }) => {
                ui.add_space(12.0);
                boxed(ui, p.success.gamma_multiply(0.05), p.success.gamma_multiply(0.3), 10, |ui| {
                    let text = if *unchanged { "The hosts file already had everything; nothing was changed." } else { "Hosts file updated. New domains work immediately." };
                    ui.label(RichText::new(text).size(14.0).color(p.success));
                    if let Some(b) = backup {
                        ui.label(RichText::new(format!("Previous version saved to {}", b.display())).size(12.0).color(p.muted));
                    }
                });
            }
            Some(Applied::Declined) => {
                ui.add_space(12.0);
                boxed(ui, p.warning.gamma_multiply(0.05), p.warning.gamma_multiply(0.4), 12, |ui| {
                    ui.label(widgets::medium("Nothing was changed because permission was declined.", 14.0));
                    ui.add(egui::Label::new(RichText::new("Add the lines above to the hosts file yourself (as administrator), or use .localhost domains, which work in browsers without a hosts entry.").size(14.0).color(p.muted)).wrap());
                });
            }
            None => {
                if let Some(err) = cx.shared.errors.get(HOSTS_APPLY_KEY) {
                    ui.add_space(12.0);
                    widgets::error_panel(ui, err);
                }
            }
        }

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !done && widgets::button(ui, Kind::Primary, None, if pending { "Waiting for permission…" } else { "Update hosts file" }, !pending && plan.changed).clicked() {
                    cx.actions.push(Action::Run(apply.task()));
                }
                if widgets::button(ui, Kind::Outline, None, if done { "Close" } else { "Cancel" }, !pending).clicked() {
                    page.overlay = None;
                }
            });
        });
    });
    if resp.should_close() && !pending {
        page.overlay = None;
    }
}
