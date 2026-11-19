use eframe::egui::{self, Color32, CornerRadius, FontId, Sense, Stroke, StrokeKind, Ui};
use zenv_core::tls::CaStatus;

use crate::loader::Task;
use crate::pages::Action;
use crate::lucide;
use crate::theme::{self, palette};
use crate::widgets::{self, Badge, Kind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Trust,
    Untrust,
}

#[derive(Default)]
pub struct HttpsState {
    confirm: Option<Confirm>,
}

pub const CREATE_KEY: &str = "tls:create";
pub const TRUST_KEY: &str = "tls:trust";
pub const UNTRUST_KEY: &str = "tls:untrust";

pub fn show(ui: &mut Ui, ca: &CaStatus, state: &mut HttpsState, busy: &[String], out: &mut Vec<Action>) {
    let p = palette(ui);
    let trusted = ca.trusted_user || ca.trusted_machine;
    let pending = busy.iter().any(|k| k == CREATE_KEY || k == TRUST_KEY || k == UNTRUST_KEY);
    widgets::card(ui, p.border).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (glyph, color) = if !ca.exists {
                (lucide::SHIELD_OFF, p.foreground)
            } else if trusted {
                (lucide::SHIELD_CHECK, p.success)
            } else {
                (lucide::SHIELD, p.warning)
            };
            icon_box(ui, glyph, color);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(widgets::strong("Local certificate authority", 14.0));
                    if !ca.exists {
                        widgets::badge(ui, "Not created", Badge::Outline);
                    } else if trusted {
                        widgets::badge(ui, "Trusted", Badge::Success);
                    } else {
                        widgets::badge(ui, "Not trusted yet", Badge::Warning);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(egui::RichText::new("Issues certificates for ").color(p.muted).small());
                    ui.label(widgets::mono(format!(".{}", ca.suffixes.join(", .")), 11.0).color(p.muted));
                    ui.label(egui::RichText::new(" only. It cannot sign certificates for real domains, and its key never leaves this computer.").color(p.muted).small());
                });
            });
        });

        if ca.exists {
            ui.add_space(8.0);
            egui::Grid::new("ca-details").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Subject").color(p.muted).small());
                ui.label(widgets::mono(ca.subject.as_deref().unwrap_or(""), 11.0));
                ui.end_row();
                ui.label(egui::RichText::new("SHA-256").color(p.muted).small());
                ui.label(widgets::mono(ca.sha256.as_deref().unwrap_or(""), 11.0));
                ui.end_row();
                ui.label(egui::RichText::new("Valid until").color(p.muted).small());
                ui.label(widgets::mono(ca.not_after.as_deref().map(|d| &d[..d.len().min(10)]).unwrap_or(""), 11.0));
                ui.end_row();
                ui.label(egui::RichText::new("Certificate").color(p.muted).small());
                ui.label(widgets::mono(ca.cert_path.display().to_string(), 11.0));
                ui.end_row();
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if !ca.exists {
                if widgets::button_sm(ui, Kind::Primary, None, "Create certificate authority", !pending).clicked() {
                    out.push(Action::Run(Task::new(CREATE_KEY, "Create certificate authority", |c| c.tls.create_ca().map(|_| ()))));
                }
            }
            if ca.exists && !ca.trusted_user {
                if widgets::button_sm(ui, Kind::Primary, None, "Trust in Windows…", !pending).clicked() {
                    state.confirm = Some(Confirm::Trust);
                }
            }
            if ca.trusted_user {
                if widgets::button_sm(ui, Kind::Outline, None, "Remove trust…", !pending).clicked() {
                    state.confirm = Some(Confirm::Untrust);
                }
            }
        });
        if ca.exists && !trusted {
            ui.label(egui::RichText::new("HTTPS sites already work; until the authority is trusted, browsers show a certificate warning.").color(p.muted).small());
        }
    });

    if let Some(confirm) = state.confirm {
        let (title, label, key, body) = match confirm {
            Confirm::Trust => (
                "Trust Zenv's certificate authority?",
                "Continue to Windows",
                TRUST_KEY,
                "It is added to your Windows user's Trusted Root store, so Edge, Chrome and other Windows apps accept your local HTTPS sites. Windows shows its own security warning next.",
            ),
            Confirm::Untrust => (
                "Remove trust?",
                "Continue to Windows",
                UNTRUST_KEY,
                "Browsers will warn about your local HTTPS sites again. The authority and its certificates are kept.",
            ),
        };
        let choice = widgets::confirm_modal(ui.ctx(), key, title, label, false, pending, |ui| {
            ui.label(egui::RichText::new(body).color(palette(ui).muted));
        });
        match choice {
            widgets::ModalChoice::Confirm => {
                out.push(Action::Run(match confirm {
                    Confirm::Trust => Task::new(TRUST_KEY, "Trust the certificate authority", |c| c.tls.trust().map(|_| ())),
                    Confirm::Untrust => Task::new(UNTRUST_KEY, "Remove trust", |c| c.tls.untrust().map(|_| ())),
                }));
                state.confirm = None;
            }
            widgets::ModalChoice::Cancel => state.confirm = None,
            widgets::ModalChoice::None => {}
        }
    }
}

fn icon_box(ui: &mut Ui, glyph: &str, color: Color32) {
    let p = palette(ui);
    let (rect, _r) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(theme::RADIUS), p.muted_bg, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::new(16.0, theme::icons()), color);
}

