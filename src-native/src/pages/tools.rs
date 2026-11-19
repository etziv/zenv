use std::time::{Duration, Instant};

use eframe::egui::{self, Ui};
use zenv_core::ai_router::{self, RouterModel, RouterPage, RouterSpec, RouterStatus};
use zenv_core::codex::{self, CodexStatus};
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::service_manager::{Health, ServiceStatus};

use super::{Action, Cx};
use crate::loader::{Loader, Note, Task, Tone};
use crate::lucide;
use crate::theme::palette;
use crate::widgets::{self, Kind};

mod codex_row;

const COMPONENTS: [ComponentId; 2] = [ComponentId::Omniroute, ComponentId::Ninerouter];

#[derive(Default)]
pub struct Picker {
    pub open: bool,
    pub model: String,
    pub context: String,
    pub output: String,
    pub models: Loader<Vec<RouterModel>>,
}

#[derive(Default)]
pub struct RouterState {
    pub status: Loader<RouterStatus>,
    pub codex: Loader<CodexStatus>,
    pub picker: Picker,
    pub show_text: bool,
    pub copied_url: Option<Instant>,
    pub copied_snippet: Option<Instant>,
}

pub struct Routers(pub [RouterState; 2]);

impl Default for Routers {
    fn default() -> Self {
        Self([RouterState::default(), RouterState::default()])
    }
}

impl Routers {
    pub fn refresh(&mut self, ctx: &egui::Context, cx: &Cx) {
        let services = cx.shared.services.ok().map(Vec::as_slice).unwrap_or_default();
        for (i, component) in COMPONENTS.into_iter().enumerate() {
            let state = &mut self.0[i];
            if cx.tasks_finished {
                state.status.invalidate();
                state.codex.invalidate();
            }
            state.status.refresh_every(Duration::from_secs(30), ctx, cx.core, move |c| c.router_status(component));
            state.codex.refresh_every(Duration::from_secs(30), ctx, cx.core, move |_c| {
                let spec = ai_router::spec(component).expect("known router component");
                codex::status(&spec.codex)
            });
            let running = services.iter().find(|s| s.service.component == component && s.health == Health::Running);
            if let (true, Some(s)) = (state.picker.open, running) {
                let id = s.service.id.clone();
                state.picker.models.refresh_every(Duration::from_secs(60), ctx, cx.core, move |c| c.router_models(&id));
            }
        }
    }

    pub fn panel(&mut self, ui: &mut Ui, status: &ServiceStatus, busy: &[String], out: &mut Vec<Action>) {
        let Some(i) = COMPONENTS.iter().position(|c| *c == status.service.component) else { return };
        let spec = ai_router::spec(COMPONENTS[i]).expect("known router component");
        let running = status.health == Health::Running;
        router_panel(ui, spec, status, running, &mut self.0[i], busy, out);
    }
}

pub fn is_router(component: ComponentId) -> bool {
    COMPONENTS.contains(&component)
}

pub fn base_url(status: &ServiceStatus) -> String {
    let listen = status.service.listen_address.as_str();
    let host = if listen == "0.0.0.0" {
        "127.0.0.1".to_owned()
    } else if listen == "::" {
        "[::1]".to_owned()
    } else if listen.contains(':') {
        format!("[{listen}]")
    } else {
        listen.to_owned()
    };
    format!("http://{host}:{}/v1", status.service.http_port)
}

fn router_panel(ui: &mut Ui, spec: &'static RouterSpec, status: &ServiceStatus, running: bool, state: &mut RouterState, busy: &[String], out: &mut Vec<Action>) {
    let service_id = status.service.id.clone();
    let base = base_url(status);
    ui.horizontal_wrapped(|ui| {
        let open_title = (!running).then(|| format!("Start {} first", spec.name));
        let r = widgets::button_sm(ui, Kind::Primary, Some(lucide::LAYOUT_DASHBOARD), "Open dashboard", running);
        let r = if let Some(t) = &open_title { r.on_hover_text(t) } else { r };
        if r.clicked() {
            out.push(Action::OpenRouter(service_id.clone(), None));
        }
        let copied = state.copied_url.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
        let label = if copied { "Copied".to_owned() } else { base.clone() };
        if widgets::button_sm(ui, Kind::Outline, Some(if copied { lucide::CHECK } else { lucide::COPY }), &label, true).clicked() {
            ui.ctx().copy_text(base.clone());
            state.copied_url = Some(Instant::now());
        }
        if copied {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
    });
    ui.add_space(6.0);
    match state.status.get() {
        None => widgets::loading(ui, "Loading router status…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(router)) => {
            shortcuts(ui, spec.component, &service_id, running, &router.pages, out);
            update_row(ui, spec, router, busy, out)
        }
    }
    ui.add_space(6.0);
    match state.codex.get() {
        None => widgets::loading(ui, "Loading Codex status…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(codex_status)) => {
            let codex_status = codex_status.clone();
            codex_row::show(ui, spec, &base, running, &codex_status, state, busy, out);
        }
    }
}

pub const fn update_key(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Omniroute => "router:omniroute:update",
        ComponentId::Ninerouter => "router:ninerouter:update",
        _ => "router:update",
    }
}

pub const fn auto_update_key(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Omniroute => "router:omniroute:auto",
        ComponentId::Ninerouter => "router:ninerouter:auto",
        _ => "router:auto",
    }
}

fn us_date(iso: &str) -> String {
    let date = &iso[..iso.len().min(10)];
    let mut parts = date.splitn(3, '-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(m), Some(d)) => format!("{}/{}/{y}", m.trim_start_matches('0'), d.trim_start_matches('0')),
        _ => date.to_owned(),
    }
}

pub fn update_note(name: &str, status: &RouterStatus) -> Note {
    if let Some(err) = &status.last_error {
        return Note::new(Tone::Error, format!("{name} was not updated"), err.clone());
    }
    match (&status.latest, &status.auto_from) {
        (Some(latest), _) if status.installed.as_ref() == Some(latest) => Note::new(Tone::Success, format!("{name} {latest} is the newest"), ""),
        (Some(latest), Some(from)) => Note::new(
            Tone::Success,
            format!("{name} {latest} is available"),
            format!("New releases wait a week before auto-update installs them (from {}). Use Install now to get it earlier.", us_date(from)),
        ),
        _ => Note::new(Tone::Success, format!("{name} checked"), ""),
    }
}

fn update_row(ui: &mut Ui, spec: &'static RouterSpec, router: &RouterStatus, busy: &[String], out: &mut Vec<Action>) {
    let p = palette(ui);
    let component = spec.component;
    let updating = router.updating || busy.iter().any(|k| k == update_key(component));
    let newer = router.latest.is_some() && router.installed.is_some() && router.latest != router.installed;
    ui.horizontal_wrapped(|ui| {
        let text = if updating {
            "Checking for an update…".to_owned()
        } else if newer {
            let auto = router.auto_from.as_ref().filter(|_| router.auto_update);
            match auto {
                Some(from) => format!("Version {} is available · installs automatically from {}", router.latest.as_deref().unwrap_or(""), us_date(from)),
                None => format!("Version {} is available", router.latest.as_deref().unwrap_or("")),
            }
        } else if let Some(checked) = &router.last_checked {
            format!("Up to date · checked {}", &checked[..checked.len().min(16)])
        } else {
            "Not checked yet".to_owned()
        };
        ui.label(egui::RichText::new(text).color(p.muted).small());
        let label = if newer { "Install now" } else { "Check for updates" };
        if widgets::button_sm(ui, Kind::Outline, Some(lucide::REFRESH_CW), label, !updating).clicked() {
            let now = newer;
            let name = spec.name;
            out.push(Action::Run(Task::reporting(update_key(component), format!("Couldn't update {name}"), move |c| Ok(vec![update_note(name, &c.router_update(component, now)?)]))));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let pending = busy.iter().any(|k| k == auto_update_key(component));
            let r = widgets::toggle(ui, router.auto_update, !pending, &format!("Update {} automatically", spec.name));
            if r.clicked() {
                let on = !router.auto_update;
                out.push(Action::Run(Task::new(auto_update_key(component), "Auto-update", move |c| c.router_set_auto_update(component, on).map(|_| ()))));
            }
            ui.label(egui::RichText::new("Auto-update").color(p.muted).small());
        });
    });
    if let Some(err) = &router.last_error {
        widgets::error_panel(ui, err);
    }
}

pub fn shortcut_groups(component: ComponentId) -> &'static [(&'static str, &'static [&'static str])] {
    match component {
        ComponentId::Omniroute => &[
            ("Providers", &["providers", "combos"]),
            ("Access", &["api-manager", "api-endpoints"]),
            ("Spend", &["usage", "quota", "costs"]),
            ("Watch", &["logs", "health"]),
        ],
        ComponentId::Ninerouter => &[
            ("Providers", &["providers", "combos"]),
            ("Access", &["endpoint", "cli-tools"]),
            ("Spend", &["usage", "quota"]),
            ("Setup", &["settings"]),
        ],
        _ => &[],
    }
}

fn shortcuts(ui: &mut Ui, component: ComponentId, service_id: &str, running: bool, pages: &[RouterPage], out: &mut Vec<Action>) {
    let groups = shortcut_groups(component);
    for pair in groups.chunks(2) {
        ui.columns(2, |columns| {
            for (i, (group, ids)) in pair.iter().enumerate() {
                let ui = &mut columns[i];
                ui.horizontal_wrapped(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(64.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(widgets::mono(widgets::tracked(&group.to_uppercase()), 10.0).color(palette(ui).muted));
                    });
                    for id in *ids {
                        let label = pages.iter().find(|p| p.id == *id).map_or(*id, |p| p.label);
                        let r = widgets::button_sm(ui, Kind::Ghost, None, label, running);
                        if r.clicked() {
                            out.push(Action::OpenRouter(service_id.to_owned(), Some((*id).to_owned())));
                        }
                    }
                });
            }
        });
    }
    ui.add_space(4.0);
}
