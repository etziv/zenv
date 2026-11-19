use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText, Ui};
use zenv_core::diagnostics::CheckStatus;
use zenv_core::local_sites::{SiteView, Strategy};

use super::{AppsPage, DetailsTab, Overlay};
use crate::loader::Loader;
use crate::pages::Cx;
use crate::theme::palette;
use crate::widgets::{self, Kind};
use crate::lucide;

pub fn dialog(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, cx: &mut Cx) {
    let tab = match &page.overlay {
        Some(Overlay::Details { tab, .. }) => *tab,
        _ => return,
    };
    let key = (site.site.id.clone(), tab);
    if page.details_shown.as_ref() != Some(&key) {
        page.details_shown = Some(key);
        page.diagnose = Loader::default();
        page.log = Loader::default();
        page.log_follow = true;
    }

    let ctx = ui.ctx().clone();
    let resp = widgets::modal(&ctx, "details-dialog").show(&ctx, |ui| {
        ui.set_width(560.0);
        if widgets::dialog_title(ui, site.site.name.as_str()) {
            page.overlay = None;
        }
        ui.label(RichText::new(format!("{} · served by {}", site.url, site.service_name)).small().color(palette(ui).muted));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            tab_button(ui, page, site, tab, DetailsTab::Diagnose, "Diagnose");
            tab_button(ui, page, site, tab, DetailsTab::Error, "Error log");
            tab_button(ui, page, site, tab, DetailsTab::Access, "Access log");
            if site.site.strategy == Strategy::ReverseProxy {
                tab_button(ui, page, site, tab, DetailsTab::Process, "Output");
            }
        });
        ui.separator();
        match tab {
            DetailsTab::Diagnose => diagnose_tab(ui, page, site, cx),
            DetailsTab::Error => log_tab(ui, page, site, "error", cx),
            DetailsTab::Access => log_tab(ui, page, site, "access", cx),
            DetailsTab::Process => log_tab(ui, page, site, "process", cx),
        }
        ui.add_space(8.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, Kind::Outline, None, "Close", true).clicked() {
                page.overlay = None;
            }
        });
    });
    if resp.should_close() {
        page.overlay = None;
    }
}

fn tab_button(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, current: DetailsTab, tab: DetailsTab, label: &str) {
    let kind = if current == tab { Kind::Primary } else { Kind::Outline };
    if widgets::button(ui, kind, None, label, true).clicked() {
        page.overlay = Some(Overlay::Details { site_id: site.site.id.clone(), tab });
    }
}

fn diagnose_tab(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let id = site.site.id.clone();
    page.diagnose.refresh_every(Duration::from_secs(300), &ctx, cx.core, move |c| c.sites.diagnose(&id));
    if widgets::button(ui, Kind::Outline, Some(lucide::REFRESH_CW), "Check again", true).clicked() {
        page.diagnose.invalidate();
    }
    match page.diagnose.get() {
        None => widgets::loading(ui, "Checking…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(checks)) => {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for c in checks {
                    ui.horizontal(|ui| {
                        let (icon, color) = match c.status {
                            CheckStatus::Ok => (lucide::CIRCLE_CHECK, palette(ui).success),
                            CheckStatus::Warning => (lucide::TRIANGLE_ALERT, palette(ui).warning),
                            CheckStatus::Error => (lucide::CIRCLE_ALERT, palette(ui).destructive),
                        };
                        ui.label(widgets::icon(icon, 14.0).color(color));
                        ui.vertical(|ui| {
                            ui.label(widgets::strong(c.label.as_str(), 13.0));
                            ui.label(RichText::new(c.detail.as_str()).small().color(palette(ui).muted));
                        });
                    });
                    ui.separator();
                }
            });
        }
    }
}

fn log_tab(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, kind: &str, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let id = site.site.id.clone();
    let k = kind.to_owned();
    if kind == "process" {
        let id2 = site.site.id.clone();
        page.log.refresh_every(if page.log_follow { Duration::from_secs(2) } else { Duration::from_secs(3600) }, &ctx, cx.core, move |c| {
            let _ = c.refresh_docker_log(&id2);
            c.sites.read_log(&id2, &k, 300)
        });
    } else {
        page.log.refresh_every(if page.log_follow { Duration::from_secs(2) } else { Duration::from_secs(3600) }, &ctx, cx.core, move |c| c.sites.read_log(&id, &k, 300));
    }
    ui.horizontal(|ui| {
        let mut follow = page.log_follow;
        if ui.checkbox(&mut follow, "Follow").changed() {
            page.log_follow = follow;
            page.log.invalidate();
        }
        if widgets::icon_button(ui, lucide::REFRESH_CW, "Refresh").clicked() {
            page.log.invalidate();
        }
    });
    match page.log.get() {
        None => widgets::loading(ui, "Loading…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(tail)) => {
            egui::ScrollArea::vertical().max_height(300.0).stick_to_bottom(page.log_follow).show(ui, |ui| {
                if !tail.exists {
                    ui.label(RichText::new("No log yet.").color(palette(ui).muted));
                }
                for line in &tail.lines {
                    ui.label(widgets::mono(line.as_str(), 11.0));
                }
            });
        }
    }
}
