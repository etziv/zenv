mod details_dialog;
pub mod docker_picker;
pub mod form;
mod grid;
pub mod hosts_dialog;
mod panel;
mod program;
mod support;
mod tailnet;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, Ui};
use zenv_core::app_process::AppProcessStatus;
use zenv_core::diagnostics::Check;
use zenv_core::docker_apps::DockerContainer;
use zenv_core::editors::Editors;
use zenv_core::local_sites::{HostsStatus, SitePage, SiteQuery};
use zenv_core::log_manager::LogTail;
use zenv_core::runtime_catalog::{Category, ComponentId};
use zenv_core::runtime_manager::{self, Installation};
use zenv_core::service_manager::ServiceStatus;
use zenv_core::tailnet::{TailnetApp, TailnetStatus};
use zenv_core::thumbnails::Thumbnail;
use zenv_core::tls::CaStatus;

use super::{Action, Cx};
use crate::loader::{Loader, Task};
use crate::widgets;

const PAGE_SIZE: u32 = 48;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsTab {
    Diagnose,
    Error,
    Access,
    Process,
}

pub enum Overlay {
    Form(Box<form::FormState>),
    Details { site_id: String, tab: DetailsTab },
    Delete { site_id: String, domain: String },
    Hosts(hosts_dialog::HostsApply),
}

pub struct AppsPage {
    pub sites: Loader<SitePage>,
    pub installations: Loader<Vec<Installation>>,
    pub hosts: Loader<HostsStatus>,
    pub tailnet_status: Loader<TailnetStatus>,
    pub tailnet_apps: Loader<Vec<TailnetApp>>,
    pub editors: Loader<Editors>,
    pub tls: Loader<CaStatus>,
    pub docker_containers: Loader<Vec<DockerContainer>>,

    pub search: String,
    pub committed_search: String,
    pub search_at: Option<Instant>,
    pub page: u32,
    pub selected: Option<String>,
    pub last_selected: Option<String>,

    pub thumbnails: HashMap<String, Loader<Option<Thumbnail>>>,
    pub auto_capture_tried: std::collections::HashSet<String>,

    pub process: Loader<AppProcessStatus>,
    pub process_draft: Option<String>,

    pub diagnose: Loader<Vec<Check>>,
    pub log: Loader<LogTail>,
    pub log_follow: bool,
    pub details_shown: Option<(String, DetailsTab)>,

    pub overlay: Option<Overlay>,
    pub pending: Option<String>,

    pub tailnet_own_name: bool,
    pub tailnet_service: String,
}

impl Default for AppsPage {
    fn default() -> Self {
        Self {
            sites: Loader::default(),
            installations: Loader::default(),
            hosts: Loader::default(),
            tailnet_status: Loader::default(),
            tailnet_apps: Loader::default(),
            editors: Loader::default(),
            tls: Loader::default(),
            docker_containers: Loader::default(),
            search: String::new(),
            committed_search: String::new(),
            search_at: None,
            page: 0,
            selected: None,
            last_selected: None,
            thumbnails: HashMap::new(),
            auto_capture_tried: std::collections::HashSet::new(),
            process: Loader::default(),
            process_draft: None,
            diagnose: Loader::default(),
            log: Loader::default(),
            log_follow: true,
            details_shown: None,
            overlay: None,
            pending: None,
            tailnet_own_name: false,
            tailnet_service: String::new(),
        }
    }
}

impl AppsPage {
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let ctx = ui.ctx().clone();

        if cx.tasks_finished {
            self.sites.invalidate();
            self.hosts.invalidate();
            self.tailnet_status.invalidate();
            self.tailnet_apps.invalidate();
            self.docker_containers.invalidate();
            self.process.invalidate();
            self.diagnose.invalidate();
            self.log.invalidate();
            self.thumbnails.values_mut().for_each(Loader::invalidate);
        }
        if let Some(key) = self.pending.clone() {
            if !cx.busy.contains(&key) {
                self.pending = None;
                let form_failed = cx.shared.errors.contains_key(&key) && matches!(self.overlay, Some(Overlay::Form(_)));
                if !form_failed {
                    self.overlay = None;
                }
            }
        }

        let trimmed = self.search.trim().to_owned();
        if trimmed != self.committed_search {
            match self.search_at {
                Some(t) if t.elapsed() >= SEARCH_DEBOUNCE => {
                    self.committed_search = trimmed;
                    self.page = 0;
                    self.sites.invalidate();
                    self.search_at = None;
                }
                Some(_) => ctx.request_repaint_after(Duration::from_millis(30)),
                None => {
                    self.search_at = Some(Instant::now());
                    ctx.request_repaint_after(SEARCH_DEBOUNCE);
                }
            }
        }

        let query = SiteQuery { search: self.committed_search.clone(), offset: self.page * PAGE_SIZE, limit: PAGE_SIZE };
        self.sites.refresh_every(Duration::from_secs(10), &ctx, cx.core, move |c| c.sites.list(&query));
        self.installations.refresh_every(Duration::from_secs(30), &ctx, cx.core, |c| runtime_manager::list(&c.db, None));
        self.hosts.refresh_every(Duration::from_secs(10), &ctx, cx.core, |c| c.sites.hosts_status());
        self.tailnet_status.refresh_every(Duration::from_secs(10), &ctx, cx.core, |c| Ok(c.tailnet_status()));
        self.tailnet_apps.refresh_every(Duration::from_secs(10), &ctx, cx.core, |c| c.tailnet_apps());
        self.editors.refresh_every(Duration::from_secs(60), &ctx, cx.core, |c| c.editors());
        self.tls.refresh_every(Duration::from_secs(60), &ctx, cx.core, |c| c.tls.status());

        let services = cx.shared.services.ok().cloned().unwrap_or_default();
        let web_servers: Vec<ServiceStatus> = services.iter().filter(|s| s.kind == Category::WebServer).cloned().collect();
        let installations = self.installations.ok().cloned().unwrap_or_default();
        let php_versions: Vec<Installation> =
            installations.iter().filter(|i| i.component == ComponentId::Php && i.executable_exists).cloned().collect();
        let frankens: Vec<Installation> = installations.iter().filter(|i| i.component == ComponentId::Frankenphp).cloned().collect();

        let page_data = match self.sites.get() {
            Some(Err(e)) => {
                widgets::page_header(ui, "Apps", "", |_| {});
                widgets::error_panel(ui, e);
                return;
            }
            None => {
                widgets::page_header(ui, "Apps", "", |_| {});
                widgets::loading(ui, "Loading…");
                return;
            }
            Some(Ok(p)) => p.clone(),
        };
        let total = page_data.total;
        let meta = format!("{total} app{}{}", if total == 1 { "" } else { "s" }, if self.committed_search.is_empty() { "" } else { " found" });
        let web_servers_empty = web_servers.is_empty();
        widgets::page_header(ui, "Apps", &meta, |ui| {
            if widgets::button(ui, widgets::Kind::Primary, Some(crate::lucide::PLUS), "New app", !web_servers_empty).clicked() {
                self.overlay = Some(Overlay::Form(Box::new(form::FormState::create(&php_versions))));
            }
            widgets::text_input(ui, &mut self.search, "Search apps", 256.0, Some(crate::lucide::SEARCH));
        });

        if let Some(hosts) = self.hosts.ok().cloned() {
            if hosts.plan.changed {
                hosts_dialog::banner(ui, self, &hosts);
            }
        }

        if total == 0 && self.committed_search.is_empty() {
            if web_servers.is_empty() {
                widgets::empty_state(ui, "Add a web server first: Apps are served by a web server Zenv manages.");
                if widgets::button(ui, widgets::Kind::Outline, None, "Go to Web Servers", true).clicked() {
                    cx.actions.push(Action::Navigate(super::Route::WebServers));
                }
            } else {
                widgets::empty_state(ui, "No apps yet: pick a project folder to get started.");
                if widgets::button(ui, widgets::Kind::Primary, Some(crate::lucide::PLUS), "New app", true).clicked() {
                    self.overlay = Some(Overlay::Form(Box::new(form::FormState::create(&php_versions))));
                }
            }
            return;
        }
        if total == 0 {
            ui.label(RichText::new(format!("No app matches \"{}\".", self.committed_search)).color(crate::theme::palette(ui).muted));
            return;
        }

        let selected_id = self
            .selected
            .clone()
            .filter(|id| page_data.items.iter().any(|s| &s.site.id == id))
            .or_else(|| page_data.items.first().map(|s| s.site.id.clone()));
        if self.last_selected != selected_id {
            self.last_selected = selected_id.clone();
            self.process = Loader::default();
            self.diagnose = Loader::default();
            self.log = Loader::default();
            self.process_draft = None;
            self.tailnet_own_name = false;
            if let Some(sv) = selected_id.as_ref().and_then(|id| page_data.items.iter().find(|s| &s.site.id == id)) {
                self.tailnet_service = support::service_name_for(&sv.site.domain);
            }
        }
        self.selected = selected_id.clone();

        if let Some(id) = selected_id.clone() {
            self.process.refresh_every(Duration::from_secs(2), &ctx, cx.core, move |c| c.app_process_status(&id));
        }

        const PANEL_WIDTH: f32 = 352.0;
        const PANEL_GAP: f32 = 20.0;
        let avail = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = PANEL_GAP;
            ui.vertical(|ui| {
                ui.set_width((avail - PANEL_WIDTH - PANEL_GAP).max(300.0));
                grid::list(ui, self, &page_data, &web_servers, cx);
                if page_data.limit > 0 {
                    let pages = total.div_ceil(u64::from(page_data.limit)).max(1);
                    if pages > 1 {
                        grid::pagination(ui, self, &page_data, pages);
                    }
                }
            });
            ui.vertical(|ui| {
                ui.set_width(PANEL_WIDTH);
                if let Some(sv) = selected_id.as_ref().and_then(|id| page_data.items.iter().find(|s| &s.site.id == id)) {
                    panel::view(ui, self, sv, &web_servers, &php_versions, &frankens, cx);
                }
            });
        });

        if let Some(overlay) = &self.overlay {
            match overlay {
                Overlay::Hosts(apply) => {
                    let apply = apply.clone();
                    if let Some(hosts) = self.hosts.ok().cloned() {
                        hosts_dialog::dialog(ui, self, &hosts, &apply, cx);
                    }
                }
                Overlay::Delete { site_id, domain } => {
                    let (id, domain) = (site_id.clone(), domain.clone());
                    delete_dialog(ui, self, &id, &domain, cx);
                }
                Overlay::Details { site_id, .. } => {
                    if let Some(sv) = page_data.items.iter().find(|s| &s.site.id == site_id).cloned() {
                        details_dialog::dialog(ui, self, &sv, cx);
                    } else {
                        self.overlay = None;
                    }
                }
                Overlay::Form(_) => {
                    form::dialog(ui, self, &web_servers, &php_versions, &frankens, cx);
                }
            }
        }

        if let Some(err) = cx.shared.errors.iter().find(|(k, _)| k.starts_with("site:") && k.ends_with(":delete")).map(|(_, v)| v) {
            widgets::error_panel(ui, err);
        }
    }
}

fn delete_dialog(ui: &mut Ui, page: &mut AppsPage, site_id: &str, domain: &str, cx: &mut Cx) {
    let key = format!("site:{site_id}:delete");
    let pending = cx.busy.contains(&key);
    let resp = widgets::modal(ui.ctx(), "delete-app").show(ui.ctx(), |ui| {
        ui.set_width(380.0);
        if widgets::dialog_title(ui, format!("Remove {domain}?")) && !pending {
            page.overlay = None;
        }
        ui.add_space(6.0);
        ui.label("Zenv stops serving this app and forgets it. The project folder and its files stay exactly as they are.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if pending { "Removing…" } else { "Remove app" };
                if widgets::button(ui, widgets::Kind::Destructive, None, label, !pending).clicked() {
                    let id = site_id.to_owned();
                    page.pending = Some(key.clone());
                    cx.actions.push(Action::Run(
                        Task::new(key.clone(), "Remove app", move |c| c.delete_site(&id))
                        .inline(),
                    ));
                }
                if widgets::button(ui, widgets::Kind::Outline, None, "Cancel", !pending).clicked() {
                    page.overlay = None;
                }
            });
        });
    });
    if resp.should_close() && !pending {
        page.overlay = None;
    }
}
