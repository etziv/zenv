use eframe::egui::{self, Align, Layout, RichText, Ui};
use zenv_core::editors::{EditorId, EditorInfo};
use zenv_core::local_sites::{ConfigStatus, SiteInput, SiteView, Strategy};
use zenv_core::runtime_manager::Installation;
use zenv_core::service_manager::{Health, ServiceStatus};

use super::{program, support, tailnet, AppsPage};
use crate::loader::Task;
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Badge};
use crate::lucide;

fn hosts_problem(state: zenv_core::domain_manager::hosts::DomainState) -> Option<&'static str> {
    use zenv_core::domain_manager::hosts::DomainState;
    match state {
        DomainState::NotNeeded | DomainState::Managed | DomainState::External => None,
        DomainState::Missing => Some("Not in the hosts file yet"),
        DomainState::Conflict => Some("The hosts file points this domain elsewhere"),
    }
}

fn change(_ui: &Ui, cx: &mut Cx, site: &SiteView, patch: impl FnOnce(&mut SiteInput)) {
    let mut input = support::to_input(&site.site);
    patch(&mut input);
    let id = site.site.id.clone();
    cx.actions.push(Action::Run(
        Task::new(format!("site:{id}:save"), "Save", move |c| c.update_site(&id, &input).map(|_| ()))
        .inline(),
    ));
}

fn set_enabled(_ui: &Ui, cx: &mut Cx, site: &SiteView, enabled: bool) {
    let id = site.site.id.clone();
    let label = if enabled { "Enable app" } else { "Disable app" };
    cx.actions.push(Action::Run(
        Task::new(format!("site:{id}:enable"), label, move |c| c.set_site_enabled(&id, enabled).map(|_| ()))
        .inline(),
    ));
}

fn ghost_menu_button(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui)) {
    let button = egui::Button::new(widgets::icon(lucide::MORE_HORIZONTAL, 16.0)).frame(false).min_size(egui::vec2(24.0, 24.0));
    let (resp, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| add_contents(ui));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn outline_icon_button(ui: &mut Ui, glyph: &str, label: &str) -> egui::Response {
    let p = palette(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let fill = if r.hovered() { p.accent } else { p.card };
    ui.painter().rect(rect, egui::CornerRadius::same(crate::theme::RADIUS), fill, egui::Stroke::new(1.0, p.input), egui::StrokeKind::Inside);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::new(14.0, crate::theme::icons()), p.foreground);
    r.on_hover_text(label)
}

fn group<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui);
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, p.border));
    ui.add_space(14.0 - ui.spacing().item_spacing.y);
    let heading = ui.label(widgets::mono(widgets::tracked(title), 11.0).color(p.muted));
    heading.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Heading, true, title.to_uppercase()));
    ui.add_space(10.0 - ui.spacing().item_spacing.y);
    let out = body(ui);
    ui.add_space(14.0 - ui.spacing().item_spacing.y);
    out
}

fn stretch_primary_button(ui: &mut Ui, glyph: &str, label: &str, enabled: bool, width: f32) -> egui::Response {
    let p = palette(ui);
    let fg = if enabled { p.on_primary } else { p.on_primary.gamma_multiply(0.5) };
    let font = egui::FontId::new(13.5, crate::theme::medium());
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let icon_w = 16.0 + 6.0;
    let content_w = icon_w + text.size().x;
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width.max(content_w), 32.0), sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let hovered = enabled && r.hovered();
    let fill = if hovered { p.primary.gamma_multiply(0.9) } else { p.primary };
    let fill = if enabled { fill } else { fill.gamma_multiply(0.5) };
    ui.painter().rect(rect, egui::CornerRadius::same(crate::theme::RADIUS), fill, egui::Stroke::NONE, egui::StrokeKind::Inside);
    let mut x = rect.center().x - content_w / 2.0;
    ui.painter().text(egui::pos2(x + 8.0, rect.center().y), egui::Align2::CENTER_CENTER, glyph, egui::FontId::new(15.0, crate::theme::icons()), fg);
    x += icon_w;
    ui.painter().galley_with_override_text_color(egui::pos2(x, rect.center().y - text.size().y / 2.0), text, fg);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

fn editor_atom(ui: &Ui, editor: Option<&EditorInfo>) -> egui::Atom<'static> {
    let image = editor.and_then(|e| Some((e.name, widgets::data_url_bytes(e.icon.as_deref()?)?)));
    match image {
        Some((name, bytes)) => egui::Image::from_bytes(format!("bytes://editor-{name}"), bytes).fit_to_exact_size(egui::vec2(16.0, 16.0)).into(),
        None => widgets::icon(lucide::CODE, 16.0).color(palette(ui).muted).into(),
    }
}

fn editor_split_button(ui: &mut Ui, list: &[EditorInfo], preferred: Option<&EditorInfo>) -> (egui::Response, Option<EditorId>) {
    let p = palette(ui);
    let mut picked = None;
    let main_resp = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let left_radius = egui::CornerRadius { nw: crate::theme::RADIUS, ne: 0, sw: crate::theme::RADIUS, se: 0 };
            let label = preferred.map_or("No editor", |e| e.name);
            let main = egui::Button::new((editor_atom(ui, preferred), widgets::medium(label, 13.5)))
                .corner_radius(left_radius)
                .stroke(egui::Stroke::new(1.0, p.input))
                .fill(p.card)
                .min_size(egui::vec2(0.0, 32.0));
            let main_resp = ui.add_enabled(preferred.is_some(), main);
            let right_radius = egui::CornerRadius { nw: 0, ne: crate::theme::RADIUS, sw: 0, se: crate::theme::RADIUS };
            let chevron = egui::Button::new(widgets::icon(lucide::CHEVRON_DOWN, 13.0))
                .corner_radius(right_radius)
                .stroke(egui::Stroke::new(1.0, p.input))
                .fill(p.card)
                .min_size(egui::vec2(28.0, 32.0));
            let (chevron_resp, _) = egui::containers::menu::MenuButton::from_button(chevron).ui(ui, |ui| {
                ui.label(widgets::mono(widgets::tracked("OPEN IN"), 10.0).color(p.muted));
                for e in list {
                    let default = preferred.is_some_and(|d| d.id == e.id);
                    let item = egui::Button::new((editor_atom(ui, Some(e)), RichText::new(e.name).size(14.0)))
                        .right_text(RichText::new(if default { "default" } else { "" }).size(11.0).color(p.muted))
                        .frame_when_inactive(false)
                        .min_size(egui::vec2(ui.available_width().max(190.0), 32.0));
                    if ui.add(item).on_hover_text(e.program.display().to_string()).clicked() {
                        picked = Some(e.id);
                        ui.close();
                    }
                }
                if list.is_empty() {
                    ui.add_enabled(false, egui::Button::new("No editor found: VS Code, Cursor, Windsurf, Zed, PhpStorm, Sublime…").frame_when_inactive(false));
                }
            });
            chevron_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Choose editor"));
            main_resp
        })
        .inner;
    (main_resp, picked)
}

fn row(ui: &mut Ui, label: &str, hint: Option<&str>, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0));
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    if let Some(h) = hint {
        ui.add(egui::Label::new(RichText::new(h).small().color(palette(ui).muted)).wrap());
    }
}

pub fn view(
    ui: &mut Ui,
    page: &mut AppsPage,
    site: &SiteView,
    web_servers: &[ServiceStatus],
    php_versions: &[Installation],
    frankens: &[Installation],
    cx: &mut Cx,
) {
    let p = palette(ui);
    let enabling = cx.busy.iter().any(|k| k == &format!("site:{}:enable", site.site.id));
    let saving = cx.busy.iter().any(|k| k == &format!("site:{}:save", site.site.id));
    widgets::card(ui, p.border).show(ui, |ui| {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 70.0);
                ui.add(egui::Label::new(widgets::display(&site.site.domain, 17.0)).truncate());
                ui.add(egui::Label::new(RichText::new(site.site.project_root.display().to_string()).small().color(p.muted)).truncate());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ghost_menu_button(ui, &format!("More for {}", site.site.domain), |ui| {
                    super::grid::actions_menu(ui, page, site, cx);
                });
                let (text, kind) = status_badge(site.site.config_status);
                widgets::badge(ui, text, kind);
            });
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let folder_w = 32.0;
            let open_w = (ui.available_width() - folder_w - ui.spacing().item_spacing.x).max(0.0);
            if stretch_primary_button(ui, lucide::EXTERNAL_LINK, "Open in browser", site.site.enabled, open_w).clicked() {
                cx.actions.push(Action::OpenUrl(site.url.clone()));
            }
            if outline_icon_button(ui, lucide::FOLDER_OPEN, "Open project folder").clicked() {
                let path = site.site.project_root.clone();
                cx.actions.push(Action::Run(crate::loader::Task::new(
                    format!("reveal-root:{}", site.site.id),
                    "Open project folder",
                    move |_c| crate::os::reveal(&path, false),
                )));
            }
        });
        if let Some(editors) = page.editors.ok() {
            let preferred = editors.preferred.and_then(|id| editors.editors.iter().find(|e| e.id == id)).or(editors.editors.first());
            let (main_resp, picked) = editor_split_button(ui, &editors.editors, preferred);
            let editor = picked.or_else(|| main_resp.clicked().then(|| preferred.map(|e| e.id)).flatten());
            if let Some(editor) = editor {
                let id = site.site.id.clone();
                cx.actions.push(Action::Run(crate::loader::Task::new(format!("editor:{id}"), "Open in editor", move |c| {
                    c.open_in_editor(&id, Some(editor)).map(|_| ())
                })));
            }
        }

        if site.site.config_status == ConfigStatus::Failed {
            if let Some(msg) = &site.site.config_message {
                ui.label(RichText::new(msg).small().color(p.destructive));
            }
        }
        if site.site.enabled {
            if let Some(problem) = hosts_problem(site.hosts_state) {
                ui.horizontal(|ui| {
                    ui.label(widgets::icon(lucide::TRIANGLE_ALERT, 12.0).color(p.warning));
                    ui.label(RichText::new(problem).small().color(p.warning));
                });
            }
        }
        if let Some(err) = cx.shared.errors.get(&super::grid::thumb_key(&site.site.id)) {
            ui.add(egui::Label::new(RichText::new(format!("Preview: {err}")).size(11.0).color(p.muted)).wrap());
        }

        group(ui, "Status", |ui| {
            row(ui, "Serve this app", if site.site.enabled { None } else { Some("Kept in Zenv, not served.") }, |ui| {
                if widgets::toggle(ui, site.site.enabled, !enabling, "Serve this app").clicked() && !enabling {
                    set_enabled(ui, cx, site, !site.site.enabled);
                }
            });
        });

        group(ui, "Serving", |ui| {
            let mut chosen = site.site.service_id.clone();
            row(ui, "Web server", None, |ui| {
                let selected = chosen.as_deref().and_then(|id| web_servers.iter().find(|s| s.service.id == id)).map(|s| s.service.name.clone()).unwrap_or_else(|| "Any running".to_owned());
                let resp = widgets::select(ui, ("apps-web-server", &site.site.id), &selected, 160.0, |ui| {
                    ui.selectable_value(&mut chosen, None, "Any running");
                    for s in web_servers {
                        ui.selectable_value(&mut chosen, Some(s.service.id.clone()), &s.service.name);
                    }
                });
                support::name(&resp, egui::Role::ComboBox, "Web server");
            });
            if chosen != site.site.service_id {
                change(ui, cx, site, |i| i.service_id = chosen);
            }
            if site.site.strategy == Strategy::Php {
                row(
                    ui,
                    "FrankenPHP / Nginx",
                    (!site.compat.frankenphp_ready).then(|| "Only Apache reads .htaccess".to_owned()).as_deref(),
                    |ui| {
                        if site.compat.frankenphp_ready {
                            ui.label(RichText::new("Ready").color(p.success));
                        } else {
                            ui.label(RichText::new("Needs Apache").color(p.warning));
                        }
                    },
                );
            }
            row(ui, "Serve as", None, |ui| {
                let text = match site.site.strategy {
                    Strategy::ReverseProxy => format!("Proxy to :{}", site.site.upstream_port.unwrap_or_default()),
                    other => support::strategy_label(other).to_owned(),
                };
                ui.label(RichText::new(text).color(p.muted));
            });
            row(ui, "Type", None, |ui| {
                ui.label(RichText::new(support::project_type_label(site.site.project_type)).color(p.muted));
            });
        });

        match site.site.strategy {
            Strategy::ReverseProxy => {
                group(ui, "Program", |ui| program::view(ui, page, site, cx));
            }
            Strategy::Php => {
                group(ui, "Environment", |ui| {
                    let php = php_versions.iter().find(|v| Some(v.id.as_str()) == site.site.runtime_installation_id.as_deref()).cloned();
                    let mut chosen = site.site.runtime_installation_id.clone();
                    row(ui, "PHP version", None, |ui| {
                        let selected = php.as_ref().map(|p| format!("PHP {}", p.version)).unwrap_or_else(|| "(missing)".to_owned());
                        let resp = widgets::select(ui, ("apps-php", &site.site.id), &selected, 160.0, |ui| {
                            for v in php_versions {
                                ui.selectable_value(&mut chosen, Some(v.id.clone()), format!("PHP {}", v.version));
                            }
                        });
                        support::name(&resp, egui::Role::ComboBox, "PHP version");
                    });
                    if chosen != site.site.runtime_installation_id {
                        change(ui, cx, site, |i| i.runtime_installation_id = chosen);
                    }
                    let resolution = php.as_ref().map(|p| support::resolve_backend(site.site.php_engine, p, frankens));
                    let hint = resolution.as_ref().map(|r| match &r.error {
                        Some(e) => e.clone(),
                        None => {
                            let version = r.franken.as_ref().map(|f| format!(" {}", f.version)).unwrap_or_default();
                            format!("Now: {}{version}", support::backend_label(r.backend.unwrap_or("fastcgi")))
                        }
                    });
                    let mut engine = site.site.php_engine;
                    row(ui, "Runs on", hint.as_deref(), |ui| {
                        let resp = widgets::select(ui, ("apps-engine", &site.site.id), support::engine_label(engine), 160.0, |ui| {
                            for e in [zenv_core::local_sites::PhpEngine::Auto, zenv_core::local_sites::PhpEngine::Fastcgi, zenv_core::local_sites::PhpEngine::Frankenphp] {
                                ui.selectable_value(&mut engine, e, support::engine_label(e));
                            }
                        });
                        support::name(&resp, egui::Role::ComboBox, "Runs on");
                    });
                    if engine != site.site.php_engine {
                        change(ui, cx, site, |i| i.php_engine = engine);
                    }
                });
            }
            Strategy::Static => {}
        }

        group(ui, "HTTPS", |ui| {
            let tls = page.tls.ok();
            let tls_ok = tls.map(|t| t.suffixes.iter().any(|s| {
                let d = site.site.domain.to_lowercase();
                d == *s || d.ends_with(&format!(".{s}"))
            })).unwrap_or(false);
            let blocker = match tls {
                Some(t) if !t.exists => Some("Create the local certificate authority in Settings first.".to_owned()),
                Some(t) if !tls_ok => Some(format!("Available for .{} domains.", t.suffixes.join(", .").as_str())),
                _ => None,
            };
            row(ui, "HTTPS", blocker.as_deref(), |ui| {
                let on = site.site.https_enabled;
                let disabled = saving || (blocker.is_some() && !on);
                if widgets::toggle(ui, on, !disabled, "HTTPS").clicked() && !disabled {
                    change(ui, cx, site, |i| {
                        i.https_enabled = !on;
                        if !i.https_enabled {
                            i.https_redirect = false;
                        }
                    });
                }
            });
            if site.site.https_enabled {
                row(ui, "Redirect HTTP to HTTPS", None, |ui| {
                    let on = site.site.https_redirect;
                    if widgets::toggle(ui, on, !saving, "Redirect HTTP to HTTPS").clicked() && !saving {
                        change(ui, cx, site, |i| i.https_redirect = !on);
                    }
                });
            }
        });

        group(ui, "Tailscale", |ui| {
            let serving = match &site.site.service_id {
                Some(id) => web_servers.iter().any(|s| &s.service.id == id && s.health == Health::Running),
                None => web_servers.iter().any(|s| s.health == Health::Running),
            };
            tailnet::view(ui, page, site, serving, cx);
        });

        group(ui, "Paths", |ui| {
            let label = if site.site.strategy == Strategy::ReverseProxy { "Project folder" } else { "Document root" };
            let path = if site.site.strategy == Strategy::ReverseProxy { site.site.project_root.display().to_string() } else { site.site.document_root.display().to_string() };
            ui.label(RichText::new(label).small().color(p.muted));
            ui.add(egui::Label::new(widgets::mono(path, 11.5)).wrap());
        });

        if let Some(err) =
            cx.shared.errors.get(&format!("site:{}:save", site.site.id)).or_else(|| cx.shared.errors.get(&format!("site:{}:enable", site.site.id)))
        {
            widgets::error_panel(ui, err);
        }
    });
}

fn status_badge(status: ConfigStatus) -> (&'static str, Badge) {
    match status {
        ConfigStatus::Applied => ("Active", Badge::Success),
        ConfigStatus::Pending => ("Pending", Badge::Warning),
        ConfigStatus::Failed => ("Failed", Badge::Destructive),
        ConfigStatus::Disabled => ("Disabled", Badge::Outline),
    }
}
