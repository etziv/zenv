use eframe::egui::{self, RichText, Ui};
use zenv_core::runtime_catalog::{Category, ComponentId, ComponentInfo, WindowsSupport};
use zenv_core::runtime_installer::{InstallJob, Phase};
use zenv_core::runtime_manager::{InstallSource, Installation};

use super::dialogs::{Confirm, Dialog};
use super::sidebar;
use super::Data;
use crate::loader::{Loader, Note, Task, Tone};
use crate::pages::dashboard;
use crate::pages::Action;
use crate::theme::{self, palette};
use crate::widgets::{self, Kind};
use crate::lucide;

pub fn render(ui: &mut Ui, d: &Data, current: &ComponentInfo, dialog: &mut Dialog, busy: &[String], out: &mut Vec<Action>, pane_height: f32) {
    let p = palette(ui);
    let top = ui.cursor().top();
    let versions: Vec<&Installation> = d.installations.iter().filter(|i| i.component == current.id).collect();

    ui.horizontal_top(|ui| {
        sidebar::logo(ui, d.icons, current.id, !versions.is_empty(), 48.0);
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 300.0).max(160.0));
            ui.horizontal(|ui| {
                ui.label(widgets::display(current.name, 24.0));
                if current.windows_support == WindowsSupport::NoOfficialBuild {
                    warning_badge(ui, "unofficial Windows build");
                }
            });
            ui.add(egui::Label::new(RichText::new(current.distribution).small().color(p.muted)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if widgets::button(ui, Kind::Outline, Some(lucide::PLUS), "Add from folder", true).clicked() {
                *dialog = Dialog::register(current.id);
            }
            if current.installable
                && widgets::button(ui, Kind::Primary, Some(lucide::DOWNLOAD), "Install version", true).clicked()
            {
                *dialog = Dialog::install(current.id);
            }
        });
    });

    let recent: Vec<&InstallJob> = d.jobs.iter().filter(|j| j.component == current.id && shown_job(j)).collect();
    if !recent.is_empty() {
        ui.add_space(8.0);
        for job in recent {
            progress_row(ui, job, current.name);
        }
    }

    ui.add_space(10.0);
    let show_servers = current.category != Category::Language;
    let label = if show_servers { "VERSIONS & SERVERS" } else { "INSTALLED VERSIONS" };
    let count_label = format!("{label} · {}", versions.len());
    let r = ui.label(widgets::mono(widgets::tracked(&count_label), 11.0).color(p.muted));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &count_label));
    ui.add_space(6.0);

    path_conflicts_panel(ui, d, current.id);

    if versions.is_empty() {
        empty_install_hint(ui, current);
    } else if show_servers {
        super::servers::list(ui, d, current, &versions, dialog, busy, out);
    } else {
        component_card(ui, d, &versions, dialog, out);
    }

    let language_note = current.category == Category::Language && !versions.is_empty();
    if !current.notes.is_empty() || language_note {
        let measured = egui::Id::unique(("component-notes-height", current.id));
        let estimate = 6.0 + (current.notes.len() + usize::from(language_note)) as f32 * 20.0;
        let notes_block_height = ui.data(|m| m.get_temp::<f32>(measured)).unwrap_or(estimate);
        let consumed = ui.cursor().top() - top;
        ui.add_space((pane_height - consumed - notes_block_height).max(10.0));
        let notes_top = ui.cursor().top();
        ui.separator();
        ui.add_space(4.0);
        for note in current.notes {
            ui.label(RichText::new(*note).small().color(p.muted));
        }
        if language_note {
            ui.label(RichText::new("Each app picks its own version; the default one is used where none is chosen.").small().color(p.muted));
        }
        let height = ui.cursor().top() - notes_top;
        ui.data_mut(|m| m.insert_temp(measured, height));
    }
}

pub(super) fn mono_strong(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).font(egui::FontId::new(size, theme::mono_bold()))
}

const FAILED_JOB_SHOWN_SECS: i64 = 30 * 60;

pub(super) fn is_active(j: &InstallJob) -> bool {
    !matches!(j.phase, Phase::Done | Phase::Failed)
}

pub(super) fn shown_job(j: &InstallJob) -> bool {
    is_active(j) || (j.phase == Phase::Failed && zenv_core::util::seconds_ago(&j.started_at).is_some_and(|s| s < FAILED_JOB_SHOWN_SECS))
}

pub(super) fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Downloading => "Downloading",
        Phase::Verifying => "Verifying the download",
        Phase::Extracting => "Unpacking",
        Phase::Validating => "Checking the installed version",
        Phase::Done => "Installed",
        Phase::Failed => "Failed",
    }
}

pub fn progress_row(ui: &mut Ui, job: &InstallJob, name: &str) {
    let p = palette(ui);
    widgets::card(ui, p.border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(12.0).color(p.primary));
            ui.label(widgets::strong(format!("{name} {}", job.version), 13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pct = if job.phase == Phase::Downloading && job.total > 0 {
                    Some((job.downloaded * 100 / job.total.max(1)).min(100))
                } else {
                    None
                };
                let mut text = phase_label(job.phase).to_owned();
                if let Some(pct) = pct {
                    text.push_str(&format!(" · {pct}%"));
                }
                ui.label(RichText::new(text).small().color(p.muted));
            });
        });
        if let Some(err) = &job.error {
            ui.label(RichText::new(err).small().color(p.destructive));
        }
    });
}

fn empty_install_hint(ui: &mut Ui, current: &ComponentInfo) {
    let p = palette(ui);
    widgets::card(ui, p.border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(widgets::strong(format!("{} is not installed yet", current.name), 13.0));
        let hint = if current.installable {
            "Install a version from its official source, or add one that is already on this computer."
        } else {
            "Add an installation that is already on this computer."
        };
        ui.label(RichText::new(hint).small().color(p.muted));
    });
}

fn warning_badge(ui: &mut Ui, text: &str) {
    widgets::badge(ui, text, widgets::Badge::Warning);
}

pub(super) fn source_badge(ui: &mut Ui, inst: &Installation) {
    if inst.is_default {
        widgets::badge(ui, "default", widgets::Badge::Primary);
    }
    let (text, title) = if inst.source == InstallSource::Managed {
        ("zenv", "Installed by Zenv")
    } else {
        ("system", "Installed outside Zenv; detected and used as it is")
    };
    widgets::badge(ui, text, widgets::Badge::Outline).on_hover_text(title);
    if !inst.executable_exists {
        widgets::badge(ui, "missing", widgets::Badge::Destructive);
    }
    if !inst.enabled {
        widgets::badge(ui, "off", widgets::Badge::Outline);
    }
}

fn path_conflicts_panel(ui: &mut Ui, d: &Data, component: ComponentId) {
    let Some(Ok(status)) = d.path_status.get(&component).and_then(Loader::get) else { return };
    if !status.eligible || status.conflicts.is_empty() {
        return;
    }
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.warning.gamma_multiply(0.08))
        .stroke(egui::Stroke::new(1.0, p.warning.gamma_multiply(0.5)))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Already on PATH from another folder, so no version here can be added to PATH yet. Remove it in System Properties, Environment Variables:",
                )
                .small()
                .color(p.warning),
            );
            for c in &status.conflicts {
                ui.label(widgets::mono(format!("{} ({} PATH, {})", c.entry, c.scope, c.program), 10.5).color(p.muted));
            }
        });
    ui.add_space(4.0);
}

const PATH_HINT: &str = "Open a new terminal to use it; terminals already open keep their old PATH.";

pub(super) fn path_button(ui: &mut Ui, d: &Data, inst: &Installation, out: &mut Vec<Action>) {
    let Some(Ok(status)) = d.path_status.get(&inst.component).and_then(Loader::get) else { return };
    if !status.eligible {
        return;
    }
    let p = palette(ui);
    let on_path = status.installation_id.as_deref() == Some(inst.id.as_str());
    if on_path {
        ui.label(RichText::new("PATH").small().color(p.success)).on_hover_text("New terminals use this version");
        if widgets::icon_button(ui, lucide::X, &format!("Take {} off PATH", inst.version)).clicked() {
            let component = inst.component;
            let label = format!("{} {}", zenv_core::runtime_catalog::info(component).name, inst.version);
            out.push(Action::Run(Task::reporting(format!("path:remove:{component}"), format!("Couldn't take {label} off PATH"), move |c| {
                c.path_remove(component)?;
                Ok(vec![Note::new(Tone::Success, format!("{label} is off PATH"), PATH_HINT)])
            })));
        }
    } else {
        let blocked = !status.conflicts.is_empty();
        let r = widgets::button(ui, Kind::Outline, Some(lucide::TERMINAL), "Add to PATH", !blocked && inst.executable_exists);
        let r = if blocked {
            let entries: Vec<String> = status.conflicts.iter().map(|c| c.entry.clone()).collect();
            r.on_hover_text(format!("Remove {} from PATH first", entries.join(", ")))
        } else {
            r.on_hover_text("New terminals and editors use this version")
        };
        if r.clicked() {
            let (component, id) = (inst.component, inst.id.clone());
            let label = format!("{} {}", zenv_core::runtime_catalog::info(component).name, inst.version);
            out.push(Action::Run(Task::reporting(format!("path:add:{component}"), format!("Couldn't add {label} to PATH"), move |c| {
                c.path_add(&id)?;
                Ok(vec![Note::new(Tone::Success, format!("{label} is on PATH"), PATH_HINT)])
            })));
        }
    }
}

fn version_menu(ui: &mut Ui, inst: &Installation, dialog: &mut Dialog, out: &mut Vec<Action>) {
    let trigger = widgets::icon_button(ui, lucide::MORE_HORIZONTAL, "More actions");
    egui::Popup::menu(&trigger).show(|ui| installation_items(ui, inst, true, false, dialog, out));
}

pub(super) fn installation_items(ui: &mut Ui, inst: &Installation, terminal: bool, switchable: bool, dialog: &mut Dialog, out: &mut Vec<Action>) {
    if !inst.is_default && widgets::menu_item(ui, lucide::STAR, format!("Make {} the default", inst.version), true, false) {
        out.push(run_inline(format!("installation:default:{}", inst.id), format!("Make {} the default", inst.version), {
            let id = inst.id.clone();
            move |c| zenv_core::runtime_manager::set_default(&c.db, &id).map(|_| ())
        }));
        ui.close();
    }
    if widgets::menu_item(ui, lucide::REFRESH_CW, "Re-detect version", true, false) {
        out.push(run_inline(format!("installation:refresh:{}", inst.id), "Re-detect version", {
            let id = inst.id.clone();
            move |c| zenv_core::runtime_manager::refresh(&c.db, &id).map(|_| ())
        }));
        ui.close();
    }
    if terminal && widgets::menu_item(ui, lucide::TERMINAL, "Open terminal with this version", true, false) {
        out.push(run_inline(format!("installation:terminal:{}", inst.id), "Open terminal", {
            let id = inst.id.clone();
            move |c| c.open_terminal(&[id], None)
        }));
        ui.close();
    }
    if widgets::menu_item(ui, lucide::FOLDER_OPEN, "Open installation folder", true, false) {
        out.push(Action::Run(open_folder_task(&inst.id).inline()));
        ui.close();
    }
    if switchable {
        let label = if inst.enabled { format!("Switch {} off", inst.version) } else { format!("Switch {} on", inst.version) };
        if widgets::menu_item(ui, lucide::POWER, label, true, false) {
            let (id, enabled) = (inst.id.clone(), !inst.enabled);
            out.push(run_inline(format!("installation:enable:{id}"), "Switch", move |c| c.set_installation_enabled(&id, enabled).map(|_| ())));
            ui.close();
        }
    }
    ui.separator();
    let label = if inst.source == InstallSource::Managed { format!("Uninstall {}", inst.version) } else { format!("Remove {} from Zenv", inst.version) };
    if widgets::menu_item(ui, lucide::TRASH_2, label, true, true) {
        *dialog = Dialog::Confirm(Confirm::Uninstall(inst.clone()));
        ui.close();
    }
}

fn open_folder_task(id: &str) -> Task {
    let id = id.to_owned();
    Task::new(format!("installation:reveal:{id}"), "Open folder", move |c| {
        let inst = zenv_core::runtime_manager::get(&c.db, &id)?;
        crate::os::reveal(&inst.root_path, false)
    })
}

pub(super) fn run_inline(key: impl Into<String>, label: impl Into<String>, job: impl FnOnce(&zenv_core::app_core::Core) -> zenv_core::error::AppResult<()> + Send + 'static) -> Action {
    Action::Run(Task::new(key, label, job).inline())
}

pub(super) fn installation_error<'a>(errors: &'a std::collections::HashMap<String, String>, id: &str) -> Option<&'a str> {
    for prefix in ["installation:default:", "installation:refresh:", "installation:terminal:", "installation:reveal:", "installation:uninstall:", "installation:unregister:", "installation:enable:"] {
        if let Some(e) = errors.get(&format!("{prefix}{id}")) {
            return Some(e.as_str());
        }
    }
    None
}

pub(super) fn service_error<'a>(errors: &'a std::collections::HashMap<String, String>, id: &str) -> Option<&'a str> {
    let keys = [dashboard::service_key(id)].into_iter().chain(["restart", "reload", "probe", "validate", "init", "delete", "reveal"].map(|a| format!("service:{a}:{id}")));
    for key in keys {
        if let Some(e) = errors.get(&key) {
            return Some(e.as_str());
        }
    }
    None
}

fn component_card(ui: &mut Ui, d: &Data, versions: &[&Installation], dialog: &mut Dialog, out: &mut Vec<Action>) {
    let p = palette(ui);
    widgets::card(ui, p.border).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, &inst) in versions.iter().enumerate() {
            if i > 0 {
                let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                ui.painter().rect_filled(line, 0.0, p.border);
            }
            egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.label(mono_strong(&inst.version, 13.0));
                    source_badge(ui, inst);
                    if let Some(rt) = d.runtimes.iter().find(|r| r.installation_id == inst.id) {
                        if rt.running > 0 {
                            let text = if rt.kind == "php-cgi" { format!("{} workers", rt.running) } else { "running".to_owned() };
                            widgets::badge(ui, &text, widgets::Badge::Success).on_hover_text(format!("127.0.0.1:{:?}", rt.ports));
                        }
                    }
                    ui.label(widgets::mono(inst.root_path.display().to_string(), 10.5).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        version_menu(ui, inst, dialog, out);
                        path_button(ui, d, inst, out);
                    });
                });
                if let Some(e) = installation_error(d.errors, &inst.id) {
                    widgets::error_panel(ui, e);
                }
            });
        }
    });
}
