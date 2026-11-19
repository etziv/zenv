use std::time::Duration;

use eframe::egui::{self, Ui};
use zenv_core::local_sites::{SitePage, SiteQuery, SiteView};
use zenv_core::runtime_catalog::Category;
use zenv_core::service_manager::{Health, LogSource, ServiceStatus};

use super::log_panel::{self, ViewerState};
use crate::loader::Loader;
use crate::lucide;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind};

pub struct State {
    pub search: String,
    pub selected: Option<String>,
    pub sites: Loader<SitePage>,
    pub sources: Loader<Vec<LogSource>>,
    pub viewer: ViewerState,
    last_search: String,
    sources_for: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            search: String::new(),
            selected: None,
            sites: Loader::default(),
            sources: Loader::default(),
            viewer: ViewerState::default(),
            last_search: String::new(),
            sources_for: None,
        }
    }
}

fn resolve_active<'a>(page: &'a SitePage, selected: Option<&str>) -> Option<&'a SiteView> {
    selected.and_then(|id| page.items.iter().find(|s| s.site.id == id)).or_else(|| page.items.first())
}

pub fn poll(state: &mut State, ui: &Ui, cx: &Cx) {
    let ctx = ui.ctx().clone();
    if state.search != state.last_search || cx.tasks_finished {
        state.sites.invalidate();
        state.last_search = state.search.clone();
    }
    let query = SiteQuery { search: state.search.clone(), offset: 0, limit: 200 };
    state.sites.refresh_every(Duration::from_secs(30), &ctx, cx.core, move |c| c.sites.list(&query));

    let current_id = state.sites.ok().and_then(|page| resolve_active(page, state.selected.as_deref())).map(|s| s.site.id.clone());
    if current_id != state.sources_for {
        state.sources_for = current_id.clone();
        state.sources = Loader::default();
        state.viewer.reset();
    }
    if let Some(id) = current_id {
        let site_id = id.clone();
        state.sources.refresh_every(Duration::from_secs(10), &ctx, cx.core, move |c| c.sites.log_sources(&site_id));
        if let Some(sources) = state.sources.ok() {
            if let Some(active) = log_panel::active_source(sources, state.viewer.picked.as_deref()) {
                let site_id = id.clone();
                let kind = active.id.clone();
                let interval = Duration::from_secs(if state.viewer.follow { 4 } else { 30 });
                state.viewer.tail.refresh_every(interval, &ctx, cx.core, move |c| c.sites.read_log(&site_id, &kind, 300));
            }
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut State, services: Option<&[ServiceStatus]>, out: &mut Vec<Action>) -> Option<String> {
    let mut show_service = None;
    let page = match state.sites.get() {
        Some(Err(e)) => {
            widgets::error_panel(ui, e);
            return None;
        }
        Some(Ok(p)) => p,
        None => {
            widgets::loading(ui, "Loading…");
            return None;
        }
    };
    if page.total == 0 && state.search.is_empty() {
        widgets::empty_state(ui, "No sites yet. Each site gets its own access, error and PHP log here, plus the app's log (such as Laravel's) when it has one.");
        return None;
    }
    let p = palette(ui);
    let active_id = resolve_active(page, state.selected.as_deref()).map(|s| s.site.id.clone());
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(240.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            widgets::card(ui, p.border).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Find a site").desired_width(200.0));
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("project-log-list").max_height(height - 70.0).show(ui, |ui| {
                    if page.items.is_empty() {
                        ui.label(egui::RichText::new("No site matches.").color(p.muted).small());
                    }
                    for s in &page.items {
                        let active = active_id.as_deref() == Some(s.site.id.as_str());
                        if log_panel::site_row(ui, active, &s.site.domain, s.site.enabled).clicked() {
                            state.selected = Some(s.site.id.clone());
                        }
                    }
                    if (page.total as usize) > page.items.len() {
                        ui.label(egui::RichText::new(format!("Showing {} of {}; search to narrow.", page.items.len(), page.total)).color(p.muted).small());
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.set_height(height);
            if let Some(site) = resolve_active(page, state.selected.as_deref()) {
                server_banner(ui, site, services.unwrap_or_default(), &mut show_service);
                let sources = state.sources.ok().cloned().unwrap_or_default();
                log_panel::show(ui, &site.site.domain, Some(&site.site.project_root.display().to_string()), &sources, &mut state.viewer, "Nothing has been logged here yet.", out);
            }
        });
    });
    show_service
}

fn serving_server<'a>(site: &SiteView, services: &'a [ServiceStatus]) -> Option<&'a ServiceStatus> {
    if let Some(id) = &site.site.service_id {
        return services.iter().find(|s| &s.service.id == id);
    }
    let web: Vec<&ServiceStatus> = services.iter().filter(|s| s.kind == Category::WebServer).collect();
    web.iter().find(|s| s.health == Health::Running).or_else(|| web.iter().find(|s| s.health == Health::Failed)).copied()
}

fn server_banner(ui: &mut Ui, site: &SiteView, services: &[ServiceStatus], show_service: &mut Option<String>) {
    let p = palette(ui);
    match serving_server(site, services) {
        Some(s) if s.health == Health::Failed => {
            widgets::card(ui, p.destructive.gamma_multiply(0.4)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(widgets::icon(lucide::TRIANGLE_ALERT, 14.0).color(p.destructive));
                    ui.label(format!("{} stopped unexpectedly, so this site is not being served.", s.service.name));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, Kind::Outline, None, "View service log", true).clicked() {
                            *show_service = Some(s.service.id.clone());
                        }
                    });
                });
            });
        }
        Some(s) if s.health == Health::Running => {}
        _ => {
            ui.label(egui::RichText::new("No web server is serving this site right now, so nothing new is being logged.").color(p.muted).small());
        }
    }
}
