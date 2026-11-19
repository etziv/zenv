use std::time::Duration;

use eframe::egui::{self, Ui};
use zenv_core::log_manager;
use zenv_core::php_runtime::RuntimeLogGroup;
use zenv_core::runtime_catalog::Category;
use zenv_core::service_manager::{LogSource, ServiceStatus};

use super::log_panel::{self, ViewerState};
use crate::loader::Loader;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    App,
    Service(String),
    Runtime(String),
}

impl Default for Target {
    fn default() -> Self {
        Target::App
    }
}

pub struct State {
    pub target: Target,
    pub runtimes: Loader<Vec<RuntimeLogGroup>>,
    pub sources: Loader<Vec<LogSource>>,
    pub viewer: ViewerState,
    sources_for: Option<Target>,
}

impl Default for State {
    fn default() -> Self {
        Self { target: Target::default(), runtimes: Loader::default(), sources: Loader::default(), sources_for: None, viewer: ViewerState::default() }
    }
}

const GROUPS: [(Category, &str); 4] = [(Category::WebServer, "Web servers"), (Category::Database, "Databases"), (Category::Cache, "Cache & queue"), (Category::Tool, "Tools")];

pub fn poll(state: &mut State, ui: &Ui, cx: &Cx) {
    let ctx = ui.ctx().clone();
    state.runtimes.refresh_every(Duration::from_secs(30), &ctx, cx.core, |c| c.runtimes.log_groups());

    if state.sources_for.as_ref() != Some(&state.target) {
        state.sources_for = Some(state.target.clone());
        state.sources = Loader::default();
        state.viewer.reset();
    }
    match &state.target {
        Target::App => {
            state.sources.refresh_every(Duration::from_secs(3600), &ctx, cx.core, |c| {
                Ok(vec![LogSource { id: "app".into(), label: "Zenv log".into(), path: c.paths.app_log_file(), exists: true, size_bytes: 0 }])
            });
        }
        Target::Service(id) => {
            let sid = id.clone();
            state.sources.refresh_every(Duration::from_secs(10), &ctx, cx.core, move |c| c.services.log_sources(&sid));
        }
        Target::Runtime(id) => {
            let rid = id.clone();
            state.sources.refresh_every(Duration::from_secs(10), &ctx, cx.core, move |c| {
                let groups = c.runtimes.log_groups()?;
                Ok(groups.into_iter().find(|g| g.installation_id == rid).map(|g| g.sources).unwrap_or_default())
            });
        }
    }

    let Some(sources) = state.sources.ok() else { return };
    let Some(active) = log_panel::active_source(sources, state.viewer.picked.as_deref()) else { return };
    let kind = active.id.clone();
    let interval = Duration::from_secs(if state.viewer.follow { 4 } else { 30 });
    match &state.target {
        Target::App => {
            state.viewer.tail.refresh_every(interval, &ctx, cx.core, move |c| log_manager::tail(&c.paths.app_log_file(), 300));
        }
        Target::Service(id) => {
            let sid = id.clone();
            state.viewer.tail.refresh_every(interval, &ctx, cx.core, move |c| c.services.read_log(&sid, &kind, 300));
        }
        Target::Runtime(id) => {
            let rid = id.clone();
            state.viewer.tail.refresh_every(interval, &ctx, cx.core, move |c| c.runtimes.read_log(&rid, &kind, 300));
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut State, services: Option<&[ServiceStatus]>, out: &mut Vec<Action>) {
    let p = palette(ui);
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(240.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            widgets::card(ui, p.border).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().id_salt("service-log-list").max_height(height - 32.0).show(ui, |ui| {
                    log_panel::group_heading(ui, "Zenv");
                    if log_panel::text_row(ui, state.target == Target::App, "Zenv log").clicked() {
                        state.target = Target::App;
                    }
                    let list = services.unwrap_or_default();
                    for (kind, label) in GROUPS {
                        let members: Vec<&ServiceStatus> = list.iter().filter(|s| s.kind == kind).collect();
                        if members.is_empty() {
                            continue;
                        }
                        log_panel::group_heading(ui, label);
                        for s in members {
                            let active = state.target == Target::Service(s.service.id.clone());
                            if log_panel::service_row(ui, active, s.health, &s.service.name, s.service.http_port).clicked() {
                                state.target = Target::Service(s.service.id.clone());
                            }
                        }
                    }
                    if let Some(groups) = state.runtimes.ok().filter(|g| !g.is_empty()) {
                        log_panel::group_heading(ui, "PHP runtimes");
                        for g in groups {
                            let active = state.target == Target::Runtime(g.installation_id.clone());
                            if log_panel::text_row(ui, active, &g.label).clicked() {
                                state.target = Target::Runtime(g.installation_id.clone());
                            }
                        }
                    }
                });
            });
        });

        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.set_height(height);
            let (title, subtitle): (String, Option<String>) = match &state.target {
                Target::App => ("Zenv".to_owned(), Some("What Zenv itself did: starts, stops, configuration changes, errors.".to_owned())),
                Target::Service(id) => match services.and_then(|l| l.iter().find(|s| &s.service.id == id)) {
                    Some(s) => {
                        let version = s.installation.as_ref().map(|i| format!("{} {} · ", s.service.component, i.version)).unwrap_or_default();
                        (s.service.name.clone(), Some(format!("{version}{}:{}", s.service.listen_address, s.service.http_port)))
                    }
                    None => (String::new(), None),
                },
                Target::Runtime(id) => {
                    let label = state.runtimes.ok().and_then(|g| g.iter().find(|g| &g.installation_id == id)).map(|g| g.label.clone()).unwrap_or_default();
                    (label, Some("Shared by every site on this version; each project's own PHP errors are under Projects.".to_owned()))
                }
            };
            if !title.is_empty() {
                let sources = state.sources.ok().cloned().unwrap_or_default();
                log_panel::show(ui, &title, subtitle.as_deref(), &sources, &mut state.viewer, "This service has not written any logs yet.", out);
            }
        });
    });
}
