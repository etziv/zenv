use std::net::IpAddr;
use std::time::Duration;

use eframe::egui::{self, Ui};
use zenv_core::diagnostics::{self, Check, CheckStatus};
use zenv_core::port_manager::{self, PortInspection, PortStatus};

use crate::loader::Loader;
use crate::lucide;
use crate::pages::Cx;
use crate::theme::palette;
use crate::widgets::{self, Kind};

pub struct State {
    pub checks: Loader<Vec<Check>>,
    pub port: String,
    pub address: String,
    query: Option<(u16, IpAddr)>,
    pub result: Loader<PortInspection>,
}

impl Default for State {
    fn default() -> Self {
        Self { checks: Loader::default(), port: "80".to_owned(), address: "127.0.0.1".to_owned(), query: None, result: Loader::default() }
    }
}

pub fn poll(state: &mut State, ui: &Ui, cx: &Cx) {
    let ctx = ui.ctx().clone();
    state.checks.refresh_every(Duration::from_secs(300), &ctx, cx.core, diagnostics::run);
    if let Some((port, address)) = state.query {
        state.result.refresh_every(Duration::from_secs(3600), &ctx, cx.core, move |_c| port_manager::inspect(port, address));
    }
}

pub fn show_checks(ui: &mut Ui, state: &mut State) {
    if widgets::button(ui, Kind::Outline, Some(lucide::REFRESH_CW), "Run checks again", !state.checks.loading()).clicked() {
        state.checks.invalidate();
    }
    ui.add_space(6.0);
    match state.checks.get() {
        None => widgets::loading(ui, "Running checks…"),
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(checks)) => checks_list(ui, checks),
    }
}

pub fn show_ports(ui: &mut Ui, state: &mut State) {
    port_inspector(ui, state);
}

fn checks_list(ui: &mut Ui, checks: &[Check]) {
    let p = palette(ui);
    widgets::card(ui, p.border).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for c in checks {
            ui.horizontal(|ui| {
                let (glyph, color) = match c.status {
                    CheckStatus::Ok => (lucide::CIRCLE_CHECK_BIG, p.success),
                    CheckStatus::Warning => (lucide::TRIANGLE_ALERT, p.warning),
                    CheckStatus::Error => (lucide::CIRCLE_ALERT, p.destructive),
                };
                ui.label(widgets::icon(glyph, 15.0).color(color));
                ui.vertical(|ui| {
                    ui.label(widgets::strong(&c.label, 13.0));
                    if !c.detail.is_empty() {
                        ui.label(widgets::mono(&c.detail, 11.0).color(p.muted));
                    }
                });
            });
            ui.separator();
        }
    });
}

fn port_inspector(ui: &mut Ui, state: &mut State) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.label("Address");
        ui.add(egui::TextEdit::singleline(&mut state.address).hint_text("Address").desired_width(140.0));
        ui.label("Port");
        if ui.add(egui::TextEdit::singleline(&mut state.port).hint_text("Port").desired_width(80.0)).changed() {
            state.port.retain(|c| c.is_ascii_digit());
        }
        let valid_port = state.port.parse::<u16>().is_ok_and(|p| p != 0);
        let valid_addr = state.address.parse::<IpAddr>().is_ok();
        if widgets::button(ui, Kind::Primary, None, "Check port", valid_port && valid_addr).clicked() {
            if let (Ok(port), Ok(address)) = (state.port.parse::<u16>(), state.address.parse::<IpAddr>()) {
                state.query = Some((port, address));
                state.result.invalidate();
            }
        }
    });
    match state.result.get() {
        None if state.query.is_some() => widgets::loading(ui, "Checking…"),
        None => {}
        Some(Err(e)) => widgets::error_panel(ui, e),
        Some(Ok(r)) => {
            widgets::card(ui, p.border).show(ui, |ui| match &r.status {
                PortStatus::Free => {
                    ui.label(format!("{}:{} is free.", r.address, r.port));
                }
                PortStatus::InUse { owners } => {
                    ui.label(format!("{}:{} is in use by:", r.address, r.port));
                    for o in owners {
                        ui.label(widgets::mono(format!("{} · PID {} · {}", o.process_name.as_deref().unwrap_or("unknown process"), o.pid, o.address), 11.5));
                    }
                }
                PortStatus::Unavailable { reason } => {
                    ui.label(format!("Cannot bind: {reason}"));
                }
            });
        }
    }
}
