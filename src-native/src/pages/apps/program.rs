use eframe::egui::{Align, Layout, RichText, Ui};
use zenv_core::app_process::AppProcessStatus;
use zenv_core::docker_apps::DockerAppStatus;
use zenv_core::local_sites::SiteView;

use super::{support, AppsPage, DetailsTab, Overlay};
use crate::loader::{Note, Task, Tone};
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind, Led, SwitchState};
use crate::lucide;

fn describe(s: &AppProcessStatus) -> (String, Led) {
    let port = s.port.map(|p| format!(":{p}")).unwrap_or_else(|| "its port".to_owned());
    match s.state {
        "running" => (format!("Running on {port}"), Led::On),
        "starting" => (format!("Starting… waiting for {port}"), Led::On),
        "exited" => {
            if s.listening {
                (format!("Answering on {port} (started outside Zenv)"), Led::On)
            } else {
                (format!("Exited{}; see the output", s.exit_code.map(|c| format!(" (code {c})")).unwrap_or_default()), Led::Fail)
            }
        }
        _ => {
            if s.listening {
                (format!("Answering on {port} (started outside Zenv)"), Led::On)
            } else {
                (format!("Not running; nothing answers on {port}"), Led::Off)
            }
        }
    }
}

fn status_row(ui: &mut Ui, text: &str, led: Led) {
    ui.horizontal(|ui| {
        widgets::led(ui, led, 7.0);
        ui.add(eframe::egui::Label::new(RichText::new(text).small()).truncate());
    });
}

fn restart(id: String) -> impl FnOnce(&zenv_core::app_core::Core) -> zenv_core::error::AppResult<()> + Send + 'static {
    move |c| {
        c.stop_app_process(&id)?;
        c.start_app_process_by_user(&id)?;
        Ok(())
    }
}

pub const NO_FREE_PORT: &str = "Couldn't find a free port";

pub fn view(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, cx: &mut Cx) {
    let status = match page.process.get() {
        None => {
            widgets::loading(ui, "Checking the program…");
            return;
        }
        Some(Err(e)) => {
            widgets::error_panel(ui, e);
            return;
        }
        Some(Ok(s)) => s.clone(),
    };
    if let Some(docker) = status.docker.clone() {
        docker_view(ui, page, site, &status, &docker, cx);
    } else {
        program_view(ui, page, site, &status, cx);
    }
}

fn show_output(page: &mut AppsPage, site: &SiteView) {
    page.overlay = Some(Overlay::Details { site_id: site.site.id.clone(), tab: DetailsTab::Process });
}

fn program_view(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, s: &AppProcessStatus, cx: &mut Cx) {
    let id = site.site.id.clone();
    let (text, led) = describe(s);
    let busy = cx.busy.iter().any(|k| k.starts_with(&format!("process:{id}")));
    ui.horizontal(|ui| {
        widgets::led(ui, led, 7.0);
        ui.add(eframe::egui::Label::new(if led == Led::Fail { RichText::new(&text).small().color(palette(ui).destructive) } else { RichText::new(&text).small() }).truncate());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, lucide::FILE_TEXT, "Show output").clicked() {
                show_output(page, site);
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Port").small().color(palette(ui).muted));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, Kind::Ghost, Some(lucide::WAND_SPARKLES), "Best free port", !busy).clicked() {
                let from = zenv_core::app_process::preferred_port(site.site.project_type.as_str());
                match zenv_core::app_process::free_port(cx.core, from, Some(&site.site.id)) {
                    Err(e) => cx.actions.push(Action::Toast(Note::new(Tone::Error, NO_FREE_PORT, e.to_string()))),
                    Ok(port) => {
                        if Some(port) != s.port {
                            let was_running = s.state == "running" || s.state == "starting";
                            let mut input = support::to_input(&site.site);
                            input.upstream_port = Some(port);
                            let sid = site.site.id.clone();
                            cx.actions.push(Action::Run(Task::new(format!("site:{sid}:save"), "Change port", move |c| {
                                c.update_site(&sid, &input)?;
                                if was_running {
                                    c.stop_app_process(&sid)?;
                                    c.start_app_process_by_user(&sid)?;
                                }
                                Ok(())
                            })));
                        }
                    }
                }
            }
        });
    });
    ui.label(widgets::mono(format!(":{}{}", s.port.unwrap_or(0), if s.port_vars.is_empty() { String::new() } else { format!(" -> {}", s.port_vars.join(", ")) }), 11.5));
    ui.label(RichText::new("Zenv's port, on this computer only; other settings come from the project's .env.").small().color(palette(ui).muted));
    if s.exposed {
        ui.label(RichText::new(format!("The program listens on every network; make it listen on 127.0.0.1 (port :{}).", s.port.unwrap_or(0))).small().color(palette(ui).warning));
    }

    let baseline = if s.detected { String::new() } else { s.command.clone().unwrap_or_default() };
    let mut text = page.process_draft.clone().unwrap_or_else(|| baseline.clone());
    ui.label(RichText::new("Start command").small().color(palette(ui).muted));
    ui.horizontal(|ui| {
        let placeholder = if s.detected { s.command.clone().map(|c| format!("{c} (detected)")).unwrap_or_default() } else { "e.g. cargo run, npm run dev".to_owned() };
        let resp = ui.add(eframe::egui::TextEdit::singleline(&mut text).hint_text(placeholder));
        if resp.changed() {
            page.process_draft = Some(text.clone());
        }
        let dirty = text.trim() != baseline;
        if dirty && widgets::button(ui, Kind::Primary, None, "Save", !busy).clicked() {
            let command = text.trim().to_owned();
            let autostart = s.autostart;
            let key = format!("process:{id}:save");
            let sid = id.clone();
            cx.actions.push(Action::Run(Task::new(key, "Save start command", move |c| {
                c.set_app_command(&sid, if command.is_empty() { None } else { Some(command.as_str()) }, autostart).map(|_| ())
            })));
            page.process_draft = None;
        }
    });
    if s.detected {
        if let Some(cmd) = &s.command {
            ui.label(RichText::new(format!("Detected from the project, not run yet. Zenv runs it once you press Start (or save it); it runs {cmd}.")).small().color(palette(ui).warning));
        }
    }

    ui.horizontal(|ui| {
        ui.label("Start with the web server");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::status_switch(ui, &SwitchState { on: s.autostart, failed: false, pending: busy }, "Start with the web server").clicked() && !busy {
                let command = if s.detected && s.autostart { None } else { s.command.clone() };
                let on = !s.autostart;
                let id2 = id.clone();
                cx.actions.push(Action::Run(Task::new(format!("process:{id}:autostart"), "Save", move |c| c.set_app_command(&id2, command.as_deref(), on).map(|_| ()))));
            }
        });
    });

    let own_process = s.state == "running" || s.state == "starting";
    ui.horizontal(|ui| {
        if own_process {
            if widgets::button(ui, Kind::Outline, Some(lucide::ROTATE_CW), "Restart", !busy).clicked() {
                cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Restart", restart(id.clone()))));
            }
            if widgets::button(ui, Kind::Outline, Some(lucide::SQUARE), "Stop", !busy).clicked() {
                let sid = id.clone();
                cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Stop", move |c| c.stop_app_process(&sid).map(|_| ()))));
            }
        } else {
            let can_start = !busy && s.command.is_some() && !s.listening && site.site.enabled;
            if widgets::button(ui, Kind::Primary, Some(lucide::PLAY), "Start", can_start).clicked() {
                let sid = id.clone();
                cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Start", move |c| c.start_app_process_by_user(&sid).map(|_| ()))));
            }
        }
    });
}

fn docker_view(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, s: &AppProcessStatus, d: &DockerAppStatus, cx: &mut Cx) {
    let id = site.site.id.clone();
    let busy = cx.busy.iter().any(|k| k.starts_with(&format!("process:{id}")) || k == "docker-engine");
    let (text, led) = if !d.engine_running {
        ("Docker Desktop is not running".to_owned(), Led::Off)
    } else if d.state == "missing" {
        (format!("Container {} no longer exists", d.container), Led::Fail)
    } else {
        describe(s)
    };
    status_row(ui, &text, led);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::icon_button(ui, lucide::FILE_TEXT, "Show the container's output").clicked() {
            show_output(page, site);
        }
    });

    ui.label(widgets::mono(format!("Container: {}", d.container), 11.0));
    if let Some(compose) = &d.compose {
        ui.label(widgets::mono(format!("Compose: {}/{}", compose.project, compose.service), 11.0));
    }
    if let Some(image) = &d.image {
        ui.label(widgets::mono(format!("Image: {image}"), 11.0));
    }
    ui.label(widgets::mono(format!("Port: :{}", s.port.unwrap_or(0)), 11.0));
    if let Some(problem) = &d.problem {
        if d.engine_running {
            ui.label(RichText::new(problem).small().color(palette(ui).destructive));
        }
    }
    if s.exposed {
        ui.label(RichText::new(format!("Docker publishes :{} on every network; publish it as 127.0.0.1:{}:<port> instead.", s.port.unwrap_or(0), s.port.unwrap_or(0))).small().color(palette(ui).warning));
    }

    ui.horizontal(|ui| {
        ui.label("Start with the web server");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::status_switch(ui, &SwitchState { on: s.autostart, failed: false, pending: busy }, "Start with the web server").clicked() && !busy {
                let on = !s.autostart;
                let sid = id.clone();
                cx.actions.push(Action::Run(Task::new(format!("process:{id}:autostart"), "Save", move |c| c.set_app_command(&sid, None, on).map(|_| ()))));
            }
        });
    });

    let up = s.state == "running" || s.state == "starting";
    ui.horizontal(|ui| {
        if !d.engine_running {
            if widgets::button(ui, Kind::Primary, Some(lucide::BOX), "Start Docker Desktop", !busy).clicked() {
                cx.actions.push(Action::Run(Task::new("docker-engine", "Start Docker Desktop", |_c| zenv_core::docker_apps::ensure_engine().map(|_| ()))));
            }
        } else if up {
            if widgets::button(ui, Kind::Outline, Some(lucide::ROTATE_CW), "Restart", !busy).clicked() {
                cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Restart", restart(id.clone()))));
            }
            if widgets::button(ui, Kind::Outline, Some(lucide::SQUARE), "Stop", !busy).clicked() {
                let sid = id.clone();
                cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Stop", move |c| c.stop_app_process(&sid).map(|_| ()))));
            }
        } else if widgets::button(ui, Kind::Primary, Some(lucide::PLAY), "Start container", !busy && d.state != "missing" && site.site.enabled).clicked() {
            let sid = id.clone();
            cx.actions.push(Action::Run(Task::new(format!("process:{id}"), "Start container", move |c| c.start_app_process_by_user(&sid).map(|_| ()))));
        }
    });
}
