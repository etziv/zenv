use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText, Ui};
use zenv_core::docker_apps::{DockerContainer, PublishedPort};

use super::AppsPage;
use crate::loader::Task;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind, Led};
use crate::lucide;

#[derive(Clone)]
pub struct PickedContainer {
    pub container: DockerContainer,
    pub port: PublishedPort,
}

pub fn view(ui: &mut Ui, page: &mut AppsPage, picked: &mut Option<PickedContainer>, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    page.docker_containers.refresh_every(Duration::from_secs(5), &ctx, cx.core, |c| c.docker_containers());
    match page.docker_containers.get() {
        None => widgets::loading(ui, "Asking Docker for its containers…"),
        Some(Err(e)) => {
            widgets::error_panel(ui, e);
            if e.contains("Docker Desktop is not running") {
                let busy = cx.busy.iter().any(|k| k == "docker-engine");
                if widgets::button(ui, Kind::Primary, Some(lucide::BOX), "Start Docker Desktop", !busy).clicked() {
                    cx.actions.push(Action::Run(Task::new("docker-engine", "Start Docker Desktop", |_c| zenv_core::docker_apps::ensure_engine().map(|_| ()))));
                }
            }
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(RichText::new("Docker has no containers yet. Create one (docker run or docker compose up), then reload.").color(palette(ui).muted));
        }
        Some(Ok(list)) => {
            let list = list.clone();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Zenv proxies a .test domain to the port the container publishes on this computer.").small().color(palette(ui).muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, lucide::REFRESH_CW, "Reload").clicked() {
                        page.docker_containers.invalidate();
                    }
                });
            });
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for c in &list {
                    container_row(ui, c, picked);
                }
            });
        }
    }
}

fn container_row(ui: &mut Ui, c: &DockerContainer, picked: &mut Option<PickedContainer>) {
    let usable = !c.ports.is_empty();
    let selected = picked.as_ref().is_some_and(|p| p.container.name == c.name);
    let border = if selected { palette(ui).primary } else { palette(ui).border };
    widgets::card(ui, border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::led(ui, if c.state == "running" { Led::On } else { Led::Off }, 6.0);
            ui.add(egui::Label::new(widgets::strong(&c.name, 13.0)).truncate());
            if let Some(compose) = &c.compose {
                ui.label(widgets::mono(format!("{}/{}", compose.project, compose.service), 10.5).color(palette(ui).muted));
            }
        });
        ui.label(RichText::new(format!("{} · {}", c.image, c.status)).small().color(palette(ui).muted));
        if usable {
            ui.horizontal_wrapped(|ui| {
                for p in &c.ports {
                    let is_selected = selected && picked.as_ref().is_some_and(|picked| picked.port == *p);
                    let text = format!(":{} -> {}", p.host_port, p.container_port);
                    let kind = if is_selected { Kind::Primary } else { Kind::Outline };
                    if widgets::button(ui, kind, None, &text, true).clicked() {
                        *picked = Some(PickedContainer { container: c.clone(), port: *p });
                    }
                }
            });
        } else {
            ui.label(
                RichText::new("Publishes no port (add -p 8080:80 or ports: in compose), so Windows can't reach it.")
                    .small()
                    .color(palette(ui).muted),
            );
        }
    });
    ui.add_space(4.0);
}
