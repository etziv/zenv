use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use eframe::egui::{self, Ui};
use zenv_core::php_runtime::RuntimeProcess;
use zenv_core::runtime_catalog::{self, Category, ComponentId, ComponentInfo};
use zenv_core::runtime_installer::{AvailableVersion, InstallJob, Phase};
use zenv_core::runtime_manager::{DiscoveredInstallation, Installation};
use zenv_core::path_env::PathStatus;
use zenv_core::service_manager::ServiceStatus;

use super::tools::Routers;
use super::{Action, Cx, Route};
use crate::loader::Loader;
use crate::lucide;
use crate::theme::{self, palette, panel};
use crate::widgets::{self, Kind};

mod detail;
mod dialogs;
mod discovery;
mod rows;
mod servers;
mod sidebar;

pub use dialogs::{Confirm, Dialog};
pub use rows::{Notice, Rows};

#[derive(Default)]
pub struct ComponentsPage {
    installations: Loader<Vec<Installation>>,
    runtimes: Loader<Vec<RuntimeProcess>>,
    jobs: Loader<Vec<InstallJob>>,
    discover: Loader<Vec<DiscoveredInstallation>>,
    scanned: bool,
    icons: HashMap<ComponentId, Loader<Option<String>>>,
    available: HashMap<ComponentId, Loader<Vec<AvailableVersion>>>,
    path_status: HashMap<ComponentId, Loader<PathStatus>>,
    picked: HashMap<Route, ComponentId>,
    dialog: Dialog,
    routers: RefCell<Routers>,
    rows: RefCell<Rows>,
}

pub struct Data<'a> {
    pub category: Category,
    pub title: &'a str,
    pub components: &'a [ComponentInfo],
    pub installations: &'a [Installation],
    pub runtimes: &'a [RuntimeProcess],
    pub services: &'a [ServiceStatus],
    pub icons: &'a HashMap<ComponentId, Loader<Option<String>>>,
    pub path_status: &'a HashMap<ComponentId, Loader<PathStatus>>,
    pub jobs: &'a [InstallJob],
    pub discover: Option<&'a Loader<Vec<DiscoveredInstallation>>>,
    pub available: Option<&'a Loader<Vec<AvailableVersion>>>,
    pub errors: &'a HashMap<String, String>,
    pub routers: &'a RefCell<Routers>,
    pub rows: &'a RefCell<Rows>,
}

pub const SIDEBAR_WIDTH: f32 = 272.0;

pub fn category_of(route: Route) -> Category {
    match route {
        Route::Languages => Category::Language,
        Route::Databases => Category::Database,
        Route::Cache => Category::Cache,
        Route::Tools => Category::Tool,
        _ => Category::WebServer,
    }
}

pub fn title_of(route: Route) -> &'static str {
    match route {
        Route::Languages => "Languages",
        Route::Databases => "Databases",
        Route::Cache => "Cache & Queue",
        Route::Tools => "Tools",
        _ => "Web Servers",
    }
}

impl ComponentsPage {
    pub fn ui(&mut self, route: Route, ui: &mut Ui, cx: &mut Cx) {
        if cx.tasks_finished {
            self.installations.invalidate();
            self.runtimes.invalidate();
            self.jobs.invalidate();
            for loader in self.available.values_mut() {
                loader.invalidate();
            }
            for loader in self.path_status.values_mut() {
                loader.invalidate();
            }
        }
        let ctx = ui.ctx().clone();
        self.installations.refresh_every(Duration::from_secs(10), &ctx, cx.core, |c| zenv_core::runtime_manager::list(&c.db, None));
        self.runtimes.refresh_every(Duration::from_secs(5), &ctx, cx.core, |c| Ok(c.runtime_status()));
        self.jobs.refresh_every(Duration::from_millis(800), &ctx, cx.core, |c| Ok(c.installer.jobs()));
        if self.scanned {
            self.discover.refresh_every(Duration::from_secs(3600), &ctx, cx.core, |c| {
                zenv_core::runtime_manager::discover(&c.db, None, std::slice::from_ref(&c.paths.bin_dir))
            });
        }

        let category = category_of(route);
        if category == Category::Tool {
            self.routers.get_mut().refresh(&ctx, cx);
        }
        self.rows.get_mut().poll(&ctx, cx.core);
        for info in runtime_catalog::catalog().iter().filter(|c| c.category == category) {
            let id = info.id;
            self.icons.entry(id).or_default().refresh_every(Duration::from_secs(3600), &ctx, cx.core, move |c| {
                zenv_core::icons::component_icon(&c.db, &c.paths, id)
            });
            self.path_status.entry(id).or_default().refresh_every(Duration::from_secs(15), &ctx, cx.core, move |c| c.path_status(id));
        }
        if let Dialog::Install(state) = &self.dialog {
            let component = state.component;
            self.available.entry(component).or_default().refresh_every(Duration::from_secs(300), &ctx, cx.core, move |c| {
                c.installer.available(component)
            });
        }

        let (installations, runtimes, jobs) = match (self.installations.get(), self.runtimes.get(), self.jobs.get()) {
            (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => return widgets::error_panel(ui, e),
            (Some(Ok(i)), Some(Ok(r)), Some(Ok(j))) => (i.as_slice(), r.as_slice(), j.as_slice()),
            _ => return widgets::loading(ui, "Loading…"),
        };

        let components: Vec<ComponentInfo> =
            runtime_catalog::catalog().iter().filter(|c| c.category == category).cloned().collect();
        let services = cx.shared.services.ok().map(Vec::as_slice).unwrap_or(&[]);
        let available = match &self.dialog {
            Dialog::Install(state) => self.available.get(&state.component),
            _ => None,
        };

        let data = Data {
            category,
            title: title_of(route),
            components: &components,
            installations,
            runtimes,
            services,
            icons: &self.icons,
            path_status: &self.path_status,
            jobs,
            discover: self.scanned.then_some(&self.discover),
            available,
            errors: &cx.shared.errors,
            routers: &self.routers,
            rows: &self.rows,
        };

        let picked = self.picked.entry(route).or_insert(components.first().map_or(ComponentId::Php, |c| c.id));
        let mut scan_clicked = false;
        view(ui, &data, picked, &mut self.dialog, cx.busy, &mut scan_clicked, &mut cx.actions);
        if scan_clicked {
            self.scanned = true;
            self.discover.invalidate();
        }
    }
}

fn current_component<'a>(components: &'a [ComponentInfo], installations: &[Installation], picked: ComponentId) -> Option<&'a ComponentInfo> {
    components
        .iter()
        .find(|c| c.id == picked)
        .or_else(|| components.iter().find(|c| installations.iter().any(|i| i.component == c.id)))
        .or_else(|| components.first())
}

pub fn view(
    ui: &mut Ui,
    d: &Data,
    picked: &mut ComponentId,
    dialog: &mut Dialog,
    busy: &[String],
    scan_clicked: &mut bool,
    out: &mut Vec<Action>,
) {
    widgets::page_header(ui, d.title, "", |ui| {
        let pending = d.discover.is_some_and(Loader::loading);
        let label = if pending { "Scanning…" } else { "Scan this computer" };
        if widgets::button(ui, Kind::Outline, Some(lucide::SEARCH), label, !pending).clicked() {
            *scan_clicked = true;
        }
    });

    if let Some(loader) = d.discover {
        discovery::render(ui, loader, d.components, d.installations, out);
    }

    let Some(current) = current_component(d.components, d.installations, *picked) else {
        return;
    };

    ui.add_space(8.0);
    let p = palette(ui);
    let pane = |margin: i8| {
        egui::Frame::new()
            .fill(p.card)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(theme::RADIUS + 2))
            .inner_margin(egui::Margin::same(margin))
    };
    let height = ui.available_height().max(1.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(SIDEBAR_WIDTH);
            ui.set_height(height);
            pane(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("components-sidebar").show(ui, |ui| {
                    sidebar::render(ui, d, current.id, picked, out);
                });
            });
        });
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.set_height(height);
            pane(20).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                let content_height = ui.available_height();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("components-detail").show(ui, |ui| {
                    detail::render(ui, d, current, dialog, busy, out, content_height);
                });
            });
        });
    });

    dialogs::show(ui.ctx(), d, dialog, busy, out);
}

pub fn install_indicator(ui: &mut Ui, jobs: &[InstallJob]) -> Option<ComponentId> {
    let active: Vec<&InstallJob> = jobs.iter().filter(|j| !matches!(j.phase, Phase::Done | Phase::Failed)).collect();
    let first = *active.first()?;
    let name_of = |id: ComponentId| -> String {
        runtime_catalog::catalog().iter().find(|c| c.id == id).map(|c| c.name.to_owned()).unwrap_or_else(|| id.as_str().to_owned())
    };
    let pct = (first.phase == Phase::Downloading && first.total > 0).then(|| (first.downloaded * 100 / first.total.max(1)).min(100));
    let label = if active.len() == 1 {
        match pct {
            Some(pct) => format!("Installing {} {} · {pct}%", name_of(first.component), first.version),
            None => format!("Installing {} {}", name_of(first.component), first.version),
        }
    } else {
        format!("Installing {} versions", active.len())
    };
    let tooltip = active
        .iter()
        .map(|j| format!("{} {}: {}", name_of(j.component), j.version, detail::phase_label(j.phase)))
        .collect::<Vec<_>>()
        .join("\n");

    let p = theme::palette(ui);
    let frame = egui::Frame::new()
        .fill(p.panel_raised)
        .stroke(egui::Stroke::new(1.0, p.panel_line))
        .corner_radius(egui::CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.set_height(28.0);
            ui.set_max_width(288.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(egui::Spinner::new().size(14.0).color(panel::LED));
                ui.add(egui::Label::new(widgets::mono(&label, 11.0).color(panel::TEXT)).truncate());
            });
        });
    let r = frame.response.interact(egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let r = r.on_hover_text(tooltip);
    r.clicked().then_some(first.component)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenv_core::runtime_manager::InstallSource;

    #[test]
    fn category_and_title_follow_route() {
        assert_eq!(category_of(Route::Languages), Category::Language);
        assert_eq!(category_of(Route::Databases), Category::Database);
        assert_eq!(category_of(Route::Cache), Category::Cache);
        assert_eq!(category_of(Route::WebServers), Category::WebServer);
        assert_eq!(title_of(Route::Languages), "Languages");
        assert_eq!(title_of(Route::WebServers), "Web Servers");
    }

    #[test]
    fn current_component_falls_back_to_installed_then_first() {
        let components: Vec<ComponentInfo> =
            runtime_catalog::catalog().iter().filter(|c| c.category == Category::Database).cloned().collect();
        let installed = vec![Installation {
            id: "x".into(),
            component: ComponentId::Postgresql,
            version: "16".into(),
            version_detail: String::new(),
            root_path: Default::default(),
            executable_path: Default::default(),
            source: InstallSource::Managed,
            is_default: true,
            enabled: true,
            executable_exists: true,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }];
        let found = current_component(&components, &installed, ComponentId::Redis);
        assert_eq!(found.map(|c| c.id), Some(ComponentId::Postgresql));
    }
}
