use std::time::Duration;

use eframe::egui::{self, vec2, Align, Color32, Layout, RichText, Sense, Ui};
use zenv_core::local_sites::{ConfigStatus, SitePage, SiteView, Strategy};
use zenv_core::service_manager::{Health, ServiceStatus};

use super::{support, AppsPage, DetailsTab, Overlay};
use crate::loader::Task;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind};
use crate::lucide;

pub(super) fn thumb_key(id: &str) -> String {
    format!("thumbnail:{id}")
}

fn capture_task(id: &str, label: &str) -> Task {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let id = id.to_owned();
    Task::new(thumb_key(&id), label, move |c| {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
        c.capture_thumbnail(&id).map(|_| ())
    })
    .inline()
}

fn is_serving(site: &SiteView, web_servers: &[ServiceStatus]) -> bool {
    match &site.site.service_id {
        Some(id) => web_servers.iter().any(|s| &s.service.id == id && s.health == Health::Running),
        None => web_servers.iter().any(|s| s.health == Health::Running),
    }
}

fn grid_cols(width: f32) -> usize {
    const MIN: f32 = 210.0;
    const GAP: f32 = 12.0;
    (((width + GAP) / (MIN + GAP)).floor() as usize).max(1)
}

pub fn list(ui: &mut Ui, page: &mut AppsPage, data: &SitePage, web_servers: &[ServiceStatus], cx: &mut Cx) {
    let cols = grid_cols(ui.available_width());
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
    for row in data.items.chunks(cols) {
        ui.columns(cols, |columns| {
            for (col, site) in columns.iter_mut().zip(row) {
                card(col, page, site, web_servers, cx);
            }
        });
    }
}

pub fn pagination(ui: &mut Ui, page: &mut AppsPage, data: &SitePage, pages: u64) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let current = u64::from(page.page);
            let from = page.page * data.limit + 1;
            let to = ((page.page + 1) * data.limit).min(data.total as u32);
            ui.label(RichText::new(format!("{from}–{to} of {}", data.total)).small().color(palette(ui).muted));
            if widgets::icon_button(ui, lucide::CHEVRON_RIGHT, "Next page").clicked() && current + 1 < pages {
                page.page += 1;
                page.sites.invalidate();
            }
            if widgets::icon_button(ui, lucide::CHEVRON_LEFT, "Previous page").clicked() && page.page > 0 {
                page.page -= 1;
                page.sites.invalidate();
            }
        });
    });
}

fn card(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, web_servers: &[ServiceStatus], cx: &mut Cx) {
    let selected = page.selected.as_deref() == Some(site.site.id.as_str());
    let serving = is_serving(site, web_servers);
    let p = palette(ui);
    let border = if selected { p.primary } else { p.border };
    let frame = widgets::card(ui, border).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        thumbnail(ui, page, site, serving, cx);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 48.0);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add(egui::Label::new(widgets::medium(&site.site.domain, 13.0)).truncate());
                let sub = if site.site.strategy == Strategy::Php {
                    site.php_version.as_deref().map(|v| format!("PHP {v}")).unwrap_or_else(|| support::project_type_label(site.site.project_type).to_owned())
                } else {
                    support::project_type_label(site.site.project_type).to_owned()
                };
                ui.label(RichText::new(sub).size(11.0).color(p.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, Kind::Outline, None, "Open", site.site.enabled).clicked() {
                    cx.actions.push(Action::OpenUrl(site.url.clone()));
                }
            });
        });
    });
    let resp = frame.response.interact(Sense::click());
    if resp.clicked() {
        page.selected = Some(site.site.id.clone());
    }
    resp.context_menu(|ui| {
        page.selected = Some(site.site.id.clone());
        actions_menu(ui, page, site, cx);
    });
}

fn thumbnail(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, serving: bool, cx: &mut Cx) {
    let id = site.site.id.clone();
    let ctx = ui.ctx().clone();
    let loader = page.thumbnails.entry(id.clone()).or_default();
    let fetch_id = id.clone();
    loader.refresh_every(Duration::from_secs(3600), &ctx, cx.core, move |c| c.thumbnails.get(&fetch_id));
    let data = loader.ok().cloned().flatten();
    let tried = loader.get().is_some();
    let capturing = cx.busy.iter().any(|k| k == &thumb_key(&id));

    if data.is_none() && tried && serving && site.site.enabled && !capturing && !page.auto_capture_tried.contains(&id) {
        page.auto_capture_tried.insert(id.clone());
        cx.actions.push(Action::Run(capture_task(&id, "Capture preview")));
    }

    let width = ui.available_width();
    let height = width / 1.6;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.ctx().request_repaint();
    }
    let p = palette(ui);
    ui.painter().rect_filled(rect, 6.0, p.background);
    if let Some(thumb) = &data {
        if let Some(bytes) = widgets::data_url_bytes(&thumb.data_url) {
            let uri = format!("bytes://thumb-{id}-{}", thumb.captured_at);
            ui.put(rect, egui::Image::from_bytes(uri, bytes).fit_to_exact_size(rect.size()).corner_radius(6));
        }
    } else {
        let note = if !site.site.enabled {
            "Switched off"
        } else if capturing {
            "Taking a preview…"
        } else if serving {
            if tried { "No preview" } else { support::project_type_label(site.site.project_type) }
        } else {
            "Start a web server to preview"
        };
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(height / 2.0 - 16.0);
                ui.label(widgets::display(site.site.name.chars().next().unwrap_or('?').to_uppercase().to_string(), 22.0).color(p.muted));
                ui.label(RichText::new(note).small().color(p.muted));
            });
        });
    }
    if !site.site.enabled {
        badge(ui, rect.left_top() + vec2(5.0, 5.0), "Off", p.muted, p.card);
    } else if site.site.config_status == ConfigStatus::Failed {
        badge(ui, rect.left_top() + vec2(5.0, 5.0), "Failed", p.on_primary, p.destructive);
    }
    if capturing {
        ui.put(egui::Rect::from_min_size(rect.right_top() + vec2(-24.0, 4.0), vec2(18.0, 18.0)), egui::Spinner::new().size(14.0));
    } else if serving && site.site.enabled && hovered {
        let btn_rect = egui::Rect::from_min_size(rect.right_top() + vec2(-24.0, 4.0), vec2(20.0, 20.0));
        let r = ui.put(btn_rect, egui::Button::new(widgets::icon(lucide::REFRESH_CW, 12.0)).frame(false));
        if r.on_hover_text("Refresh preview").clicked() {
            cx.actions.push(Action::Run(capture_task(&id, "Refresh preview")));
        }
    }
    ui.advance_cursor_after_rect(rect);
}

fn badge(ui: &mut Ui, pos: egui::Pos2, text: &str, fg: Color32, bg: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(10.0), fg);
    let pad = vec2(5.0, 2.0);
    let rect = egui::Rect::from_min_size(pos, galley.size() + pad * 2.0);
    ui.painter().rect_filled(rect, 3.0, bg);
    ui.painter().galley(rect.min + pad, galley, fg);
}

pub fn actions_menu(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, cx: &mut Cx) {
    let preferred = page.editors.ok().and_then(|e| e.preferred);
    let editor_label = page
        .editors
        .ok()
        .and_then(|e| e.editors.iter().find(|ed| Some(ed.id) == e.preferred).map(|ed| ed.name.to_owned()))
        .map(|n| format!("Open in {n}"))
        .unwrap_or_else(|| "Open in editor".to_owned());
    if item(ui, lucide::CODE, &editor_label, preferred.is_some()) {
        let id = site.site.id.clone();
        cx.actions.push(Action::Run(Task::new(format!("editor:{id}"), "Open in editor", move |c| c.open_in_editor(&id, None).map(|_| ()))));
    }
    if item(ui, lucide::SPARKLES, "Open in Studio", true) {
        cx.actions.push(Action::OpenStudio(crate::studio::Site::from(site)));
    }
    if item(ui, lucide::PENCIL, "Edit all settings", true) {
        page.overlay = Some(Overlay::Form(Box::new(super::form::FormState::edit(site))));
    }
    if item(ui, lucide::ACTIVITY, "Diagnose & logs", true) {
        page.overlay = Some(Overlay::Details { site_id: site.site.id.clone(), tab: DetailsTab::Diagnose });
    }
    if item(ui, lucide::IMAGE, "Refresh preview", site.site.enabled) {
        cx.actions.push(Action::Run(capture_task(&site.site.id, "Refresh preview")));
    }
    if item(ui, lucide::COPY, "Duplicate", true) {
        let id = site.site.id.clone();
        cx.actions.push(Action::Run(Task::new(format!("site:{id}:duplicate"), "Duplicate", move |c| c.sites.duplicate(&id).map(|_| ()))));
    }
    if item(ui, lucide::FILE_COG, "Show configuration file", site.site.enabled) {
        let id = site.site.id.clone();
        cx.actions.push(Action::Run(Task::new(format!("reveal-config:{id}"), "Show configuration", move |c| {
            let path = c.sites.config_file(&id)?;
            crate::os::reveal(&path, true)
        })));
    }
    ui.separator();
    if destructive_item(ui, lucide::TRASH, "Remove app") {
        page.overlay = Some(Overlay::Delete { site_id: site.site.id.clone(), domain: site.site.domain.clone() });
    }
}

fn item(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> bool {
    widgets::menu_item(ui, icon, label, enabled, false)
}

fn destructive_item(ui: &mut Ui, icon: &str, label: &str) -> bool {
    widgets::menu_item(ui, icon, label, true, true)
}
