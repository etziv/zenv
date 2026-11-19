use eframe::egui::{self, Align, Layout, RichText, Ui};
use zenv_core::docker_apps::DockerBinding;
use zenv_core::local_sites::detect::DetectedProject;
use zenv_core::local_sites::{PhpEngine, ProjectType, SiteInput, SiteView, Strategy};
use zenv_core::runtime_manager::Installation;
use zenv_core::service_manager::ServiceStatus;

use super::docker_picker::{self, PickedContainer};
use super::{support, AppsPage, Overlay};
use crate::loader::Task;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Folder,
    Docker,
}

pub struct FormState {
    pub editing: Option<SiteView>,
    pub step: u8,
    pub source: Source,
    pub picked: Option<PickedContainer>,
    pub project_root: String,
    pub name: String,
    pub domain: String,
    pub document_root: String,
    pub project_type: ProjectType,
    pub service_id: String,
    pub strategy: Strategy,
    pub upstream_port: String,
    pub php_id: String,
    pub php_engine: PhpEngine,
    pub https: bool,
    pub https_redirect: bool,
    pub enabled: bool,
    pub detected: Option<DetectedProject>,
    pub error: Option<String>,
}

fn default_php(php_versions: &[Installation]) -> String {
    php_versions.iter().find(|p| p.is_default).or_else(|| php_versions.first()).map(|p| p.id.clone()).unwrap_or_default()
}

impl FormState {
    pub fn create(php_versions: &[Installation]) -> Self {
        Self {
            editing: None,
            step: 1,
            source: Source::Folder,
            picked: None,
            project_root: String::new(),
            name: String::new(),
            domain: String::new(),
            document_root: ".".to_owned(),
            project_type: ProjectType::Custom,
            service_id: String::new(),
            strategy: Strategy::Static,
            upstream_port: String::new(),
            php_id: default_php(php_versions),
            php_engine: PhpEngine::Auto,
            https: false,
            https_redirect: false,
            enabled: true,
            detected: None,
            error: None,
        }
    }

    pub fn edit(site: &SiteView) -> Self {
        let root = site.site.project_root.display().to_string();
        let doc = site.site.document_root.display().to_string();
        Self {
            editing: Some(site.clone()),
            step: 2,
            source: Source::Folder,
            picked: None,
            project_root: root.clone(),
            name: site.site.name.clone(),
            domain: site.site.domain.clone(),
            document_root: support::relative_doc_root(&root, &doc),
            project_type: site.site.project_type,
            service_id: site.site.service_id.clone().unwrap_or_default(),
            strategy: site.site.strategy,
            upstream_port: site.site.upstream_port.map(|p| p.to_string()).unwrap_or_default(),
            php_id: site.site.runtime_installation_id.clone().unwrap_or_default(),
            php_engine: site.site.php_engine,
            https: site.site.https_enabled,
            https_redirect: site.site.https_redirect,
            enabled: site.site.enabled,
            detected: None,
            error: None,
        }
    }
}

fn run_detect(form: &mut FormState, core: &std::sync::Arc<zenv_core::app_core::Core>) {
    let path = form.project_root.trim().to_owned();
    if path.is_empty() {
        return;
    }
    match core.detect_project(&path) {
        Ok(d) => {
            form.project_root = d.project_root.display().to_string();
            form.name = d.name.clone();
            form.domain = d.domain.clone();
            form.document_root = support::relative_doc_root(&form.project_root, &d.document_root.display().to_string());
            form.project_type = d.project_type;
            form.strategy = d.strategy;
            form.upstream_port = d.upstream_port.map(|p| p.to_string()).unwrap_or_default();
            form.detected = Some(d);
            form.error = None;
            form.step = 2;
        }
        Err(e) => form.error = Some(e.to_string()),
    }
}

fn fill_from_container(form: &mut FormState, picked: &PickedContainer) {
    let base = if let Some(compose) = &picked.container.compose {
        if compose.service != compose.project { format!("{}-{}", compose.project, compose.service) } else { picked.container.name.clone() }
    } else {
        picked.container.name.clone()
    };
    let name = support::slug(&base);
    form.project_root = picked.container.compose.as_ref().and_then(|c| c.dir.clone()).unwrap_or_default();
    form.name = name.clone();
    form.domain = format!("{name}.test");
    form.document_root = ".".to_owned();
    form.project_type = ProjectType::Custom;
    form.strategy = Strategy::ReverseProxy;
    form.upstream_port = picked.port.host_port.to_string();
    form.step = 2;
}

fn step2_ok(form: &FormState, services: &[ServiceStatus]) -> bool {
    let strategy_ok = match form.strategy {
        Strategy::Static => true,
        Strategy::ReverseProxy => form.upstream_port.parse::<u16>().is_ok(),
        Strategy::Php => !form.php_id.is_empty(),
    };
    !form.name.trim().is_empty() && !form.domain.trim().is_empty() && !services.is_empty() && strategy_ok && (form.editing.is_some() || !form.project_root.trim().is_empty())
}

fn https_blocker(ui: &Ui, page: &AppsPage, domain: &str) -> Option<String> {
    let tls = page.tls.ok()?;
    if !tls.exists {
        return Some("Create Zenv's local certificate authority in Settings to use HTTPS.".to_owned());
    }
    let d = domain.trim().to_lowercase();
    let ok = tls.suffixes.iter().any(|s| d == *s || d.ends_with(&format!(".{s}")));
    if ok {
        None
    } else {
        let _ = ui;
        Some(format!("HTTPS works for domains ending in .{}.", tls.suffixes.join(", .")))
    }
}

fn build_input(form: &FormState, blocker: Option<&str>) -> SiteInput {
    SiteInput {
        name: form.name.clone(),
        domain: form.domain.clone(),
        project_root: form.project_root.clone(),
        document_root: if form.document_root.trim().is_empty() { ".".to_owned() } else { form.document_root.clone() },
        project_type: form.project_type,
        service_id: (!form.service_id.is_empty()).then(|| form.service_id.clone()),
        strategy: form.strategy,
        upstream_port: (form.strategy == Strategy::ReverseProxy).then(|| form.upstream_port.parse().ok()).flatten(),
        runtime_installation_id: (form.strategy == Strategy::Php).then(|| form.php_id.clone()),
        php_engine: if form.strategy == Strategy::Php { form.php_engine } else { PhpEngine::Auto },
        https_enabled: form.https && blocker.is_none(),
        https_redirect: form.https && blocker.is_none() && form.https_redirect,
        enabled: form.enabled,
    }
}

pub fn dialog(ui: &mut Ui, page: &mut AppsPage, services: &[ServiceStatus], php_versions: &[Installation], frankens: &[Installation], cx: &mut Cx) {
    let mut form = match page.overlay.take() {
        Some(Overlay::Form(f)) => f,
        other => {
            page.overlay = other;
            return;
        }
    };

    let title = form.editing.as_ref().map(|s| format!("Edit {}", s.site.domain)).unwrap_or_else(|| "New app".to_owned());
    let editing = form.editing.is_some();
    let docker = !editing && form.source == Source::Docker;
    let mut close = false;

    let resp = widgets::modal(ui.ctx(), "app-form").show(ui.ctx(), |ui| {
        ui.set_width(480.0);
        close |= widgets::dialog_title(ui, title);
        ui.add_space(6.0);

        if form.step == 1 && !editing {
            ui.horizontal(|ui| {
                if widgets::button(ui, if form.source == Source::Folder { Kind::Primary } else { Kind::Outline }, None, "Project folder", true).clicked() {
                    form.source = Source::Folder;
                }
                if widgets::button(ui, if form.source == Source::Docker { Kind::Primary } else { Kind::Outline }, None, "Docker container", true).clicked() {
                    form.source = Source::Docker;
                }
            });
            ui.add_space(6.0);
        }

        if form.step == 1 && docker {
            docker_picker::view(ui, page, &mut form.picked, cx);
        } else if form.step == 1 {
            ui.label("Project folder");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut form.project_root).desired_width(300.0).hint_text(r"D:\Projects\my-app"));
                if widgets::button(ui, Kind::Outline, Some(crate::lucide::FOLDER_OPEN), "Browse", true).clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_title("Select the project folder").pick_folder() {
                        form.project_root = p.display().to_string();
                        run_detect(&mut form, cx.core);
                    }
                }
            });
        }

        if form.step == 2 {
            if editing {
                ui.label("Project folder");
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut form.project_root).desired_width(300.0));
                    support::name(&r, egui::Role::TextInput, "Project folder");
                    if widgets::button(ui, Kind::Outline, Some(crate::lucide::FOLDER_OPEN), "Browse", true).clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_title("Select the project folder").set_directory(&form.project_root).pick_folder() {
                            form.project_root = p.display().to_string();
                        }
                    }
                });
            }
            if let Some(d) = &form.detected {
                ui.label(RichText::new(format!("Detected {} (found {}).", support::project_type_label(d.project_type), d.evidence)).small().color(palette(ui).primary));
                for n in &d.notes {
                    ui.label(RichText::new(n).small().color(palette(ui).muted));
                }
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(200.0));
                    support::name(&r, egui::Role::TextInput, "Name");
                });
                ui.vertical(|ui| {
                    ui.label("Project type");
                    let resp = widgets::select(ui, "apps-form-type", support::project_type_label(form.project_type), 200.0, |ui| {
                        for t in support::PROJECT_TYPES {
                            ui.selectable_value(&mut form.project_type, t, support::project_type_label(t));
                        }
                    });
                    support::name(&resp, egui::Role::ComboBox, "Project type");
                });
            });
            ui.label("Domain");
            let domain_resp = ui.add(egui::TextEdit::singleline(&mut form.domain).desired_width(300.0));
            support::name(&domain_resp, egui::Role::TextInput, "Domain");
            ui.label(if form.domain.trim().to_lowercase().ends_with(".localhost") { "Works in browsers right away." } else { "Needs a hosts-file entry; Zenv asks before adding it." });

            ui.label("Web server");
            let selected = services.iter().find(|s| s.service.id == form.service_id).map(|s| s.service.name.clone()).unwrap_or_else(|| "Any running web server".to_owned());
            let resp = widgets::select(ui, "apps-form-service", &selected, 300.0, |ui| {
                ui.selectable_value(&mut form.service_id, String::new(), "Any running web server");
                for s in services {
                    ui.selectable_value(&mut form.service_id, s.service.id.clone(), &s.service.name);
                }
            });
            support::name(&resp, egui::Role::ComboBox, "Web server");

            if docker {
                if let Some(p) = &form.picked {
                    ui.label(RichText::new(format!("Proxy to container {}: :{} -> {}", p.container.name, p.port.host_port, p.port.container_port)).small().color(palette(ui).primary));
                }
            } else {
                ui.label("Serve as");
                ui.horizontal(|ui| {
                    for s in [Strategy::Php, Strategy::Static, Strategy::ReverseProxy] {
                        if ui.selectable_label(form.strategy == s, support::strategy_label(s)).clicked() {
                            form.strategy = s;
                        }
                    }
                });
                ui.label(RichText::new(support::strategy_hint(form.strategy)).small().color(palette(ui).muted));

                match form.strategy {
                    Strategy::Php => {
                        if php_versions.is_empty() {
                            ui.label(RichText::new("No PHP version is set up. Add one under Languages & Runtimes first.").small().color(palette(ui).warning));
                        } else {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label("PHP version");
                                    let selected = php_versions.iter().find(|p| p.id == form.php_id).map(|p| format!("PHP {}", p.version)).unwrap_or_default();
                                    let resp = widgets::select(ui, "apps-form-php", &selected, 160.0, |ui| {
                                        for p in php_versions {
                                            ui.selectable_value(&mut form.php_id, p.id.clone(), format!("PHP {}{}", p.version, if p.is_default { " (default)" } else { "" }));
                                        }
                                    });
                                    support::name(&resp, egui::Role::ComboBox, "PHP version");
                                });
                                ui.vertical(|ui| {
                                    ui.label("Runs on");
                                    let resp = widgets::select(ui, "apps-form-engine", support::engine_label(form.php_engine), 160.0, |ui| {
                                        for e in [PhpEngine::Auto, PhpEngine::Fastcgi, PhpEngine::Frankenphp] {
                                            ui.selectable_value(&mut form.php_engine, e, support::engine_label_long(e));
                                        }
                                    });
                                    support::name(&resp, egui::Role::ComboBox, "Runs on");
                                });
                            });
                            if let Some(php) = php_versions.iter().find(|p| p.id == form.php_id) {
                                let r = support::resolve_backend(form.php_engine, php, frankens);
                                let (text, warn) = match r.error {
                                    Some(e) => (e, true),
                                    None => (format!("Served by {}.", support::backend_label(r.backend.unwrap_or("fastcgi"))), false),
                                };
                                ui.label(RichText::new(text).small().color(if warn { palette(ui).destructive } else { palette(ui).muted }));
                            }
                        }
                    }
                    Strategy::ReverseProxy => {
                        ui.label("App port");
                        ui.horizontal(|ui| {
                            let r = ui.add(egui::TextEdit::singleline(&mut form.upstream_port).desired_width(100.0));
                            support::name(&r, egui::Role::TextInput, "App port");
                            form.upstream_port.retain(|c| c.is_ascii_digit());
                            if widgets::button(ui, Kind::Outline, Some(crate::lucide::WAND_SPARKLES), "Best free port", true).clicked() {
                                let from = zenv_core::app_process::preferred_port(form.project_type.as_str());
                                let exclude = form.editing.as_ref().map(|s| s.site.id.clone());
                                match zenv_core::app_process::free_port(cx.core, from, exclude.as_deref()) {
                                    Ok(p) => form.upstream_port = p.to_string(),
                                    Err(e) => cx.actions.push(Action::Toast(crate::loader::Note::new(crate::loader::Tone::Error, super::program::NO_FREE_PORT, e.to_string()))),
                                }
                            }
                        });
                        ui.label(RichText::new("Zenv gives the app this port (PORT / APP_PORT) when it starts it.").small().color(palette(ui).muted));
                    }
                    Strategy::Static => {
                        ui.label("Document root");
                        let r = ui.add(egui::TextEdit::singleline(&mut form.document_root).desired_width(300.0));
                        support::name(&r, egui::Role::TextInput, "Document root");
                        ui.label(RichText::new("Relative to the project folder; \".\" is the folder itself.").small().color(palette(ui).muted));
                    }
                }
            }

            let blocker = https_blocker(ui, page, &form.domain);
            ui.horizontal(|ui| {
                ui.label("HTTPS");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let on = form.https && blocker.is_none();
                    if widgets::toggle(ui, on, blocker.is_none(), "HTTPS").clicked() && blocker.is_none() {
                        form.https = !form.https;
                    }
                });
            });
            ui.label(RichText::new(blocker.clone().unwrap_or_else(|| "Certificate from Zenv's local CA, issued and renewed automatically.".to_owned())).small().color(palette(ui).muted));
            if form.https && blocker.is_none() {
                ui.horizontal(|ui| {
                    ui.label("Redirect HTTP to HTTPS");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::toggle(ui, form.https_redirect, true, "Redirect").clicked() {
                            form.https_redirect = !form.https_redirect;
                        }
                    });
                });
            }
            ui.checkbox(&mut form.enabled, "Serve the app now");
        }

        if form.step == 3 {
            ui.label(RichText::new(format!("Domain: {}", form.domain)).strong());
            ui.label(format!("Project folder: {}", if form.project_root.is_empty() { "Zenv's dev folder".to_owned() } else { form.project_root.clone() }));
            ui.label(format!("Served as: {}", support::strategy_label(form.strategy)));
            ui.label(format!("Web server: {}", if form.service_id.is_empty() { "Any running web server".to_owned() } else { services.iter().find(|s| s.service.id == form.service_id).map(|s| s.service.name.clone()).unwrap_or_default() }));
            ui.label(format!("HTTPS: {}", if form.https { "On" } else { "Off" }));
            ui.label(format!("State: {}", if form.enabled { "Enabled" } else { "Saved, disabled" }));
        }

        let action_key = if let Some(existing) = &form.editing { format!("site:{}:save", existing.site.id) } else { "site:create".to_owned() };
        if let Some(err) = &form.error {
            widgets::error_panel(ui, err);
        } else if let Some(err) = cx.shared.errors.get(&action_key) {
            widgets::error_panel(ui, err);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let pending = page.pending.is_some();
                let label = match (form.step, editing) {
                    (1, _) => "Continue",
                    (2, false) => "Review",
                    (_, true) => if pending { "Saving…" } else { "Save changes" },
                    (_, false) => if pending { "Creating…" } else { "Create app" },
                };
                let enabled = match form.step {
                    1 => !pending && if docker { form.picked.is_some() } else { !form.project_root.trim().is_empty() },
                    2 => !pending && step2_ok(&form, services),
                    _ => !pending,
                };
                if widgets::button(ui, Kind::Primary, None, label, enabled).clicked() {
                    match form.step {
                        1 => {
                            if docker {
                                if let Some(p) = form.picked.clone() {
                                    fill_from_container(&mut form, &p);
                                }
                            } else {
                                run_detect(&mut form, cx.core);
                            }
                        }
                        2 if !editing => form.step = 3,
                        _ => {
                            let blocker = https_blocker(ui, page, &form.domain);
                            let input = build_input(&form, blocker.as_deref());
                            if docker {
                                if let Some(p) = form.picked.clone() {
                                    let binding = DockerBinding { container: p.container.name.clone(), compose: p.container.compose.clone(), autostart: true };
                                    let key = "site:create".to_owned();
                                    page.pending = Some(key.clone());
                                    cx.actions.push(Action::Run(
                                        Task::new(key, "Create app", move |c| c.create_docker_site(&input, &binding).map(|_| ()))
                                        .inline(),
                                    ));
                                }
                            } else if let Some(existing) = &form.editing {
                                let id = existing.site.id.clone();
                                let key = format!("site:{id}:save");
                                page.pending = Some(key.clone());
                                cx.actions.push(Action::Run(
                                    Task::new(key, "Save changes", move |c| c.update_site(&id, &input).map(|_| ()))
                                    .inline(),
                                ));
                            } else {
                                let key = "site:create".to_owned();
                                page.pending = Some(key.clone());
                                cx.actions.push(Action::Run(
                                    Task::new(key, "Create app", move |c| c.create_site(&input).map(|_| ()))
                                    .inline(),
                                ));
                            }
                        }
                    }
                }
                if widgets::button(ui, Kind::Outline, None, "Cancel", !pending).clicked() {
                    close = true;
                }
                if form.step > 1 && !(editing && form.step == 2) && widgets::button(ui, Kind::Ghost, None, "Back", !pending).clicked() {
                    form.step -= 1;
                }
            });
        });
    });

    if resp.should_close() {
        close = true;
    }
    if !close {
        page.overlay = Some(Overlay::Form(form));
    }
}
