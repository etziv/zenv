use std::time::Duration;

use eframe::egui::Ui;
use zenv_core::app_core::AppInfo;
use zenv_core::tls::CaStatus;

use super::Cx;
use crate::loader::Loader;
use crate::lucide;
use crate::theme::palette;
use crate::widgets::{self, Kind};

pub mod https;
mod theme_picker;

#[derive(Default)]
pub struct SettingsPage {
    info: Loader<AppInfo>,
    tls: Loader<CaStatus>,
    https: https::HttpsState,
}

impl SettingsPage {
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let ctx = ui.ctx().clone();
        if cx.tasks_finished {
            self.tls.invalidate();
        }
        self.info.refresh_every(Duration::from_secs(3600), &ctx, cx.core, |c| Ok(c.info()));
        self.tls.refresh_every(Duration::from_secs(10), &ctx, cx.core, |c| c.tls.status());
        view(ui, &self.info, &self.tls, &mut self.https, cx.busy, &mut cx.actions);
    }
}

pub fn view(ui: &mut Ui, info: &Loader<AppInfo>, tls: &Loader<CaStatus>, https: &mut https::HttpsState, busy: &[String], out: &mut Vec<super::Action>) {
    let data = match info.get() {
        Some(Err(e)) => return widgets::error_panel(ui, e),
        Some(Ok(d)) => d,
        None => return widgets::loading(ui, "Loading…"),
    };
    widgets::page_header(ui, "Settings", "", |_ui| {});

    widgets::section(ui, "About", |ui| {
        let p = palette(ui);
        widgets::card(ui, p.border).inner_margin(eframe::egui::Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let cols = widgets::columns_for(ui.available_width(), 180.0, 3);
            ui.columns(cols, |columns| {
                about_cell(&mut columns[0], "Version", data.version);
                about_cell(&mut columns[1 % cols], "Database schema", &format!("v{}", data.schema_version));
                about_cell(&mut columns[2 % cols], "Platform", &format!("{} / {}", data.os, data.arch));
            });
        });
    });

    widgets::section(ui, "Appearance", |ui| theme_picker::show(ui));

    widgets::section(ui, "HTTPS", |ui| match tls.get() {
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(ca)) => https::show(ui, ca, https, busy, out),
        None => widgets::loading(ui, "Loading…"),
    });

    widgets::section(ui, "Storage locations", |ui| storage_locations(ui, data, out));
}

fn about_cell(ui: &mut Ui, label: &str, value: &str) {
    let p = palette(ui);
    ui.vertical(|ui| {
        ui.label(eframe::egui::RichText::new(label).color(p.muted).small());
        ui.label(widgets::mono(value, 13.0));
    });
}

fn file_url(path: &std::path::Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

fn storage_locations(ui: &mut Ui, data: &AppInfo, out: &mut Vec<super::Action>) {
    let p = palette(ui);
    let rows: [(&str, &std::path::Path, bool); 8] = [
        ("Zenv home", &data.paths.root, true),
        ("Installed versions (bin)", &data.paths.bin_dir, false),
        ("Database and cache data", &data.paths.data_dir, false),
        ("Generated server configuration (etc)", &data.paths.generated_dir, false),
        ("Logs", &data.paths.logs_dir, true),
        ("Default pages of the web servers (dev)", &data.paths.dev_dir, false),
        ("Backups", &data.paths.backups_dir, false),
        ("Metadata database", &data.paths.db_file, false),
    ];
    widgets::card(ui, p.border).inner_margin(eframe::egui::Margin::symmetric(16, 0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (i, (label, path, openable)) in rows.into_iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(eframe::egui::RichText::new(label).color(p.muted).small());
                    ui.label(widgets::mono(path.display().to_string(), 12.0));
                });
                if openable {
                    ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                        if widgets::button_sm(ui, Kind::Outline, Some(lucide::FOLDER_OPEN), "Open", true).clicked() {
                            out.push(super::Action::OpenUrl(file_url(path)));
                        }
                    });
                }
            });
            ui.add_space(10.0);
        }
    });
}
