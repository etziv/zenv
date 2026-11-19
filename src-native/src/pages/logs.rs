use eframe::egui::{self, CornerRadius, FontId, Sense, Stroke, Ui};
use zenv_core::service_manager::ServiceStatus;

use super::{Action, Cx};
use crate::theme::{self, palette};
use crate::widgets;

pub mod diagnostics;
pub mod log_panel;
pub mod project_logs;
pub mod service_logs;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Projects,
    Services,
    Diagnostics,
    Ports,
}

pub struct LogsPage {
    tab: Tab,
    projects: project_logs::State,
    services: service_logs::State,
    diagnostics: diagnostics::State,
}

impl Default for LogsPage {
    fn default() -> Self {
        Self { tab: Tab::Projects, projects: project_logs::State::default(), services: service_logs::State::default(), diagnostics: diagnostics::State::default() }
    }
}

impl LogsPage {
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        project_logs::poll(&mut self.projects, ui, cx);
        service_logs::poll(&mut self.services, ui, cx);
        diagnostics::poll(&mut self.diagnostics, ui, cx);

        widgets::page_header(ui, "Logs", "", |_ui| {});
        tab_bar(ui, &mut self.tab);

        let services = cx.shared.services.ok().map(Vec::as_slice);
        let jump = view(ui, self.tab, &mut self.projects, &mut self.services, &mut self.diagnostics, services, &mut cx.actions);
        if let Some(id) = jump {
            self.services.target = service_logs::Target::Service(id);
            self.tab = Tab::Services;
        }
    }
}

pub fn tab_bar(ui: &mut Ui, tab: &mut Tab) {
    let p = palette(ui);
    let row = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (t, label) in [(Tab::Projects, "Projects"), (Tab::Services, "Services"), (Tab::Diagnostics, "Diagnostics"), (Tab::Ports, "Port inspector")] {
                if tab_button(ui, label, *tab == t).clicked() {
                    *tab = t;
                }
            }
        })
        .response
        .rect;
    ui.painter().hline(ui.max_rect().x_range(), row.bottom(), Stroke::new(1.0, p.border));
    ui.add_space(12.0);
}

fn tab_button(ui: &mut Ui, label: &str, active: bool) -> egui::Response {
    let p = palette(ui);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::new(13.0, theme::medium()), p.foreground);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20.0, 32.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, label));
    let color = if active || r.hovered() { p.foreground } else { p.muted };
    ui.painter().galley_with_override_text_color(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if active {
        let bar = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), egui::pos2(rect.right(), rect.bottom()));
        ui.painter().rect_filled(bar, CornerRadius::same(0), p.primary);
    }
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub fn view(
    ui: &mut Ui,
    tab: Tab,
    projects: &mut project_logs::State,
    services_state: &mut service_logs::State,
    diagnostics_state: &mut diagnostics::State,
    services: Option<&[ServiceStatus]>,
    out: &mut Vec<Action>,
) -> Option<String> {
    match tab {
        Tab::Projects => project_logs::show(ui, projects, services, out),
        Tab::Services => {
            service_logs::show(ui, services_state, services, out);
            None
        }
        Tab::Diagnostics => {
            diagnostics::show_checks(ui, diagnostics_state);
            None
        }
        Tab::Ports => {
            diagnostics::show_ports(ui, diagnostics_state);
            None
        }
    }
}
