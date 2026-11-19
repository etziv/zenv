pub mod apps;
pub mod components;
pub mod dashboard;
pub mod logs;
pub mod settings;
pub mod tools;

use std::sync::Arc;

use eframe::egui;
use zenv_core::app_core::Core;
use zenv_core::service_manager::ServiceStatus;

use crate::loader::{Loader, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    Dashboard,
    Apps,
    WebServers,
    Languages,
    Databases,
    Cache,
    Tools,
    Logs,
    Settings,
}

pub const NAV: &[(&str, &[(Route, &str)])] = &[
    ("Overview", &[(Route::Dashboard, "Dashboard")]),
    ("Serve", &[(Route::Apps, "Apps"), (Route::WebServers, "Web Servers")]),
    (
        "Runtimes",
        &[(Route::Languages, "Languages"), (Route::Databases, "Databases"), (Route::Cache, "Cache"), (Route::Tools, "Tools")],
    ),
    ("System", &[(Route::Logs, "Logs")]),
];

pub const ROUTES: [Route; 9] = [
    Route::Dashboard,
    Route::Apps,
    Route::WebServers,
    Route::Languages,
    Route::Databases,
    Route::Cache,
    Route::Tools,
    Route::Logs,
    Route::Settings,
];

pub fn route_from_str(s: &str) -> Option<Route> {
    ROUTES.into_iter().find(|r| format!("{r:?}") == s)
}

#[derive(Debug)]
pub enum Action {
    Navigate(Route),
    OpenUrl(String),
    OpenRouter(String, Option<String>),
    Toast(crate::loader::Note),
    Run(Task),
}

#[derive(Default)]
pub struct Shared {
    pub services: Loader<Vec<ServiceStatus>>,
    pub errors: std::collections::HashMap<String, String>,
    pub jobs: Loader<Vec<zenv_core::runtime_installer::InstallJob>>,
}

pub struct Cx<'a> {
    pub core: &'a Arc<Core>,
    pub shared: &'a Shared,
    pub busy: &'a [String],
    pub tasks_finished: bool,
    pub actions: Vec<Action>,
}

#[derive(Default)]
pub struct Pages {
    pub dashboard: dashboard::DashboardPage,
    pub apps: apps::AppsPage,
    pub components: components::ComponentsPage,
    pub logs: logs::LogsPage,
    pub settings: settings::SettingsPage,
}

impl Pages {
    pub fn ui(&mut self, route: Route, ui: &mut egui::Ui, cx: &mut Cx) {
        match route {
            Route::Dashboard => self.dashboard.ui(ui, cx),
            Route::Apps => self.apps.ui(ui, cx),
            Route::WebServers | Route::Languages | Route::Databases | Route::Cache | Route::Tools => self.components.ui(route, ui, cx),
            Route::Logs => self.logs.ui(ui, cx),
            Route::Settings => self.settings.ui(ui, cx),
        }
    }
}
