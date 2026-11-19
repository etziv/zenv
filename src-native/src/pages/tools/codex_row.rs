use std::time::{Duration, Instant};

use eframe::egui::{self, FontId, Ui};
use zenv_core::codex::{self, CodexStatus};
use zenv_core::ai_router::RouterSpec;
use zenv_core::runtime_catalog::ComponentId;

use super::{Action, Picker, RouterState};
use crate::loader::{Note, Task, Tone};
use crate::lucide;
use crate::theme::{self, palette};
use crate::widgets::{self, Kind};

pub const fn setup_key(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Omniroute => "codex:omniroute:setup",
        ComponentId::Ninerouter => "codex:ninerouter:setup",
        _ => "codex:setup",
    }
}

pub const fn default_key(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Omniroute => "codex:omniroute:default",
        ComponentId::Ninerouter => "codex:ninerouter:default",
        _ => "codex:default",
    }
}

pub const fn model_key(component: ComponentId) -> &'static str {
    match component {
        ComponentId::Omniroute => "codex:omniroute:model",
        ComponentId::Ninerouter => "codex:ninerouter:model",
        _ => "codex:model",
    }
}

const CODEX_ERROR: &str = "Couldn't change Codex's config";

fn codex_note(title: String, status: &CodexStatus) -> Vec<Note> {
    vec![Note::new(Tone::Success, title, format!("{} (backed up first). Start a new Codex session to use it.", status.config_path.display()))]
}

fn tokens(n: Option<u64>) -> String {
    match n {
        None | Some(0) => "?".to_owned(),
        Some(n) if n >= 1_000_000 => format!("{}M", round1(n as f64 / 1_000_000.0)),
        Some(n) if n >= 1_000 => format!("{}K", round1(n as f64 / 1_000.0)),
        Some(n) => n.to_string(),
    }
}

fn round1(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r:.1}") }
}

fn to_number(s: &str) -> Option<u64> {
    let cleaned: String = s.chars().filter(|c| !matches!(c, '_' | ',' | '.' | ' ')).collect();
    let n: i64 = cleaned.parse().ok()?;
    (n > 0).then(|| n as u64)
}

pub fn show(
    ui: &mut Ui,
    spec: &'static RouterSpec,
    base_url: &str,
    running: bool,
    status: &CodexStatus,
    state: &mut RouterState,
    busy: &[String],
    out: &mut Vec<Action>,
) {
    let p = palette(ui);
    let component = spec.component;
    let codex_target = spec.codex;
    let stale = status.configured && status.base_url.as_deref() != Some(base_url);
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(widgets::mono("CODEX", 10.0).color(p.muted));
        let other_default = status.default_profile.clone().or_else(|| status.top_level_provider.clone());
        let text = if !status.configured {
            match &other_default {
                Some(o) => format!("Not set up · Codex now uses {o}"),
                None => "Not set up".to_owned(),
            }
        } else if status.is_default {
            format!("Uses {} by default", spec.name)
        } else {
            match &other_default {
                Some(o) => format!("Profile ready: codex --profile {} · default is {o}", codex_target.id),
                None => format!("Profile ready: codex --profile {}", codex_target.id),
            }
        };
        ui.label(egui::RichText::new(text).color(p.muted).small());
        if !status.configured || stale {
            let pending = busy.iter().any(|k| k == setup_key(component));
            let label = if stale { "Update address" } else { "Add to Codex" };
            if widgets::button_sm(ui, Kind::Outline, None, label, !pending).clicked() {
                let base = base_url.to_owned();
                let name = spec.name;
                out.push(Action::Run(Task::reporting(setup_key(component), CODEX_ERROR, move |_c| Ok(codex_note(format!("{name} is in Codex's config"), &codex::setup(&codex_target, &base, false)?)))));
            }
        }
        if status.configured {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pending = busy.iter().any(|k| k == default_key(component));
                let r = widgets::toggle(ui, status.is_default, !pending, &format!("Codex uses {} by default", spec.name));
                if r.clicked() {
                    let on = !status.is_default;
                    let name = spec.name;
                    out.push(Action::Run(Task::reporting(default_key(component), CODEX_ERROR, move |_c| {
                        let status = codex::use_by_default(&codex_target, on)?;
                        let title = if on { format!("Codex uses {name} by default") } else { format!("Codex is back to {}", status.top_level_provider.as_deref().unwrap_or("its previous setup")) };
                        Ok(codex_note(title, &status))
                    })));
                }
                ui.label(egui::RichText::new("Default for Codex").color(p.muted).small());
            });
        }
    });
    if status.configured {
        model_picker(ui, spec, status, &mut state.picker, running, busy, out);
    }
    ui.horizontal(|ui| {
        ui.label(widgets::mono(status.config_path.display().to_string(), 10.5).color(p.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if state.show_text { "Hide config text" } else { "Show config text" };
            if widgets::button_sm(ui, Kind::Ghost, None, label, true).clicked() {
                state.show_text = !state.show_text;
            }
        });
    });
    if state.show_text {
        let snippet = codex::snippet(&codex_target, base_url);
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            ui.add(egui::Label::new(widgets::mono(snippet.clone(), 11.0)));
        });
        let copied = state.copied_snippet.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
        if widgets::button_sm(ui, Kind::Outline, Some(if copied { lucide::CHECK } else { lucide::COPY }), if copied { "Copied" } else { "Copy" }, true).clicked() {
            ui.ctx().copy_text(snippet);
            state.copied_snippet = Some(Instant::now());
        }
        if copied {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
    }
}

fn model_picker(ui: &mut Ui, spec: &'static RouterSpec, status: &CodexStatus, picker: &mut Picker, running: bool, busy: &[String], out: &mut Vec<Action>) {
    let p = palette(ui);
    let component = spec.component;
    let codex_target = spec.codex;
    if !picker.open {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(egui::RichText::new("Model ").color(p.muted).size(12.0));
            match &status.model {
                Some(m) => {
                    ui.label(egui::RichText::new(m).font(FontId::new(12.0, theme::mono_bold())).color(p.foreground));
                }
                None => {
                    ui.label(egui::RichText::new(format!("not set: Codex would ask for a model {} doesn't have", spec.name)).color(p.warning).size(12.0));
                }
            }
            if status.is_default && status.model.is_some() {
                ui.label(
                    egui::RichText::new(format!(" · context {} · output {}", tokens(status.context_window.map(|n| n as u64)), tokens(status.max_output.map(|n| n as u64))))
                        .color(p.muted)
                        .size(12.0),
                );
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            let title = (!running).then(|| format!("Start {} first", spec.name));
            let r = widgets::button_sm(ui, Kind::Outline, None, "Choose model", running);
            let r = if let Some(t) = &title { r.on_hover_text(t) } else { r };
            if r.clicked() {
                picker.open = true;
                picker.model = status.model.clone().unwrap_or_default();
                picker.context = status.context_window.map(|n| n.to_string()).unwrap_or_default();
                picker.output = status.max_output.map(|n| n.to_string()).unwrap_or_default();
            }
        });
        return;
    }
    widgets::card(ui, p.border).fill(p.muted_bg.gamma_multiply(0.3)).corner_radius(egui::CornerRadius::same(crate::theme::RADIUS)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Model").color(p.muted).small());
            let hint = if picker.models.loading() { "Loading models…" } else { "Type to search, e.g. a combo name" };
            ui.add(egui::TextEdit::singleline(&mut picker.model).desired_width(220.0).hint_text(hint));
            if widgets::icon_button(ui, lucide::REFRESH_CW, "Reload the list").clicked() {
                picker.models.invalidate();
            }
        });
        match picker.models.get() {
            Some(Err(e)) => widgets::error_panel(ui, e),
            Some(Ok(models)) => {
                let trimmed = picker.model.trim().to_owned();
                let known = models.iter().find(|m| m.id == trimmed);
                let info = match known {
                    Some(m) => {
                        let source = if m.combo { "Combo".to_owned() } else { format!("From {}", m.owned_by.clone().unwrap_or_else(|| "the router".into())) };
                        format!("{source} · context {} · output {}", tokens(m.context_window), tokens(m.max_output))
                    }
                    None if !trimmed.is_empty() => format!("Not in {}'s list of {}; Codex will still ask for it", spec.name, models.len()),
                    None => format!("{} combos and {} models", models.iter().filter(|m| m.combo).count(), models.iter().filter(|m| !m.combo).count()),
                };
                ui.label(egui::RichText::new(info).color(p.muted).small());
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    for m in models.iter().take(60) {
                        let label = match m.context_window {
                            Some(c) => format!("{} · {} context", m.id, tokens(Some(c))),
                            None => m.id.clone(),
                        };
                        if ui.selectable_label(trimmed == m.id, label).clicked() {
                            picker.model = m.id.clone();
                            if let Some(c) = m.context_window {
                                picker.context = c.to_string();
                            }
                            if let Some(o) = m.max_output {
                                picker.output = o.to_string();
                            }
                        }
                    }
                });
            }
            None => widgets::loading(ui, "Asking the router…"),
        }
        if status.is_default {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Limits").color(p.muted).small());
                ui.label("context");
                if ui.add(egui::TextEdit::singleline(&mut picker.context).hint_text("context").desired_width(90.0)).changed() {
                    picker.context.retain(|c| c.is_ascii_digit());
                }
                ui.label("output");
                if ui.add(egui::TextEdit::singleline(&mut picker.output).hint_text("output").desired_width(80.0)).changed() {
                    picker.output.retain(|c| c.is_ascii_digit());
                }
                ui.label(egui::RichText::new("tokens").color(p.muted).small());
            });
        } else {
            ui.label(
                egui::RichText::new(format!("Only the {} profile changes. Context limits are set when {} is Codex's default.", codex_target.id, spec.name))
                    .color(p.muted)
                    .small(),
            );
        }
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_save = !picker.model.trim().is_empty() && !busy.iter().any(|k| k == model_key(component));
                if widgets::button_sm(ui, Kind::Primary, None, "Save to config.toml", can_save).clicked() {
                    let model = picker.model.trim().to_owned();
                    let is_default = status.is_default;
                    let context = to_number(&picker.context).filter(|_| is_default);
                    let output = to_number(&picker.output).filter(|_| is_default);
                    let name = spec.name;
                    out.push(Action::Run(Task::reporting(model_key(component), CODEX_ERROR, move |_c| {
                        let status = codex::set_model(&codex_target, &model, context, output)?;
                        Ok(codex_note(format!("Codex asks {name} for {model}"), &status))
                    })));
                    picker.open = false;
                }
                if widgets::button_sm(ui, Kind::Ghost, None, "Cancel", true).clicked() {
                    picker.open = false;
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::{to_number, tokens};

    #[test]
    fn token_counts_are_abbreviated() {
        assert_eq!(tokens(Some(1_000_000)), "1M");
        assert_eq!(tokens(Some(1_500_000)), "1.5M");
        assert_eq!(tokens(Some(32_000)), "32K");
        assert_eq!(tokens(Some(500)), "500");
        assert_eq!(tokens(None), "?");
        assert_eq!(tokens(Some(0)), "?");
    }

    #[test]
    fn numbers_parse_loose_formatting() {
        assert_eq!(to_number("200_000"), Some(200_000));
        assert_eq!(to_number("32,000"), Some(32_000));
        assert_eq!(to_number("0"), None);
        assert_eq!(to_number("-5"), None);
        assert_eq!(to_number(""), None);
    }
}
