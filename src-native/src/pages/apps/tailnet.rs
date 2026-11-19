use eframe::egui::{self, Align, Layout, RichText, Ui};
use zenv_core::local_sites::SiteView;
use zenv_core::tailnet::TailnetStatus;

use super::{support, AppsPage};
use crate::loader::{Note, Task, Tone};
use crate::pages::{Action, Cx};
use crate::theme::palette;
use crate::widgets::{self, Kind};
use crate::lucide;

fn service_problem(st: Option<&TailnetStatus>) -> Option<String> {
    let st = st?;
    if let Some(p) = &st.problem {
        return Some(p.clone());
    }
    if st.tags.is_empty() {
        return Some("Tag this computer in Tailscale first (for example tag:server): Services need a tagged host.".to_owned());
    }
    if !st.https {
        return Some("Turn on HTTPS certificates in the Tailscale admin console (DNS page) first.".to_owned());
    }
    None
}

pub fn view(ui: &mut Ui, page: &mut AppsPage, site: &SiteView, serving: bool, cx: &mut Cx) {
    let id = site.site.id.clone();
    let apps = page.tailnet_apps.ok().cloned().unwrap_or_default();
    let status = page.tailnet_status.ok().cloned();
    let open = apps.into_iter().find(|a| a.site_id == id);
    let busy = cx.busy.iter().any(|k| k == &format!("tailnet:{id}"));
    let p = palette(ui);

    if let Some(open) = open {
        let url = open.url.clone().or_else(|| open.full_url.clone()).unwrap_or_else(|| format!("port {}", open.port));
        ui.horizontal(|ui| {
            widgets::led(ui, widgets::Led::On, 7.0);
            ui.add(eframe::egui::Label::new(widgets::mono(url.replace("http://", "").replace("https://", ""), 11.5)).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, lucide::EXTERNAL_LINK, "Open link").clicked() {
                    cx.actions.push(Action::OpenUrl(url.clone()));
                }
                if widgets::icon_button(ui, lucide::COPY, "Copy link").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
            });
        });
        if let Some(service) = &open.service {
            ui.label(
                RichText::new(format!(
                    "Tailscale Service svc:{service}. If the link doesn't load yet: in the admin console -> Services, define svc:{service} with port tcp:443 and approve this computer."
                ))
                .small()
                .color(p.muted),
            );
        } else if let Some(ip_url) = &open.ip_url {
            ui.horizontal(|ui| {
                ui.label(RichText::new("or").small().color(p.muted));
                if ui.link(RichText::new(ip_url.replace("http://", "")).small()).clicked() {
                    ui.ctx().copy_text(ip_url.clone());
                }
            });
        }
        ui.horizontal(|ui| {
            let note = if serving { "Devices on your Tailscale account." } else { "Devices on your Tailscale account (while its web server runs)." };
            ui.label(RichText::new(note).small().color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, Kind::Outline, Some(lucide::X), "Close", !busy).clicked() {
                    let sid = id.clone();
                    cx.actions.push(Action::Run(Task::new(format!("tailnet:{id}"), "Close", move |c| c.tailnet_close(&sid))));
                }
            });
        });
        return;
    }

    let own_name = page.tailnet_own_name;
    ui.horizontal(|ui| {
        ui.label("Own name (Tailscale Service, HTTPS)");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut v = own_name;
            let checkbox = ui.checkbox(&mut v, "");
            support::name(&checkbox, eframe::egui::Role::CheckBox, "Own name (Tailscale Service, HTTPS)");
            if checkbox.changed() {
                page.tailnet_own_name = v;
            }
        });
    });
    if page.tailnet_own_name {
        ui.horizontal(|ui| {
            ui.add(eframe::egui::TextEdit::singleline(&mut page.tailnet_service).desired_width(160.0));
            page.tailnet_service = page.tailnet_service.to_lowercase();
            let tailnet = status.as_ref().and_then(|s| s.dns_name.as_deref()).and_then(|d| d.split_once('.')).map(|(_, rest)| rest.to_owned());
            ui.label(RichText::new(format!(".{}", tailnet.unwrap_or_else(|| "<tailnet>.ts.net".to_owned()))).small().color(p.muted));
        });
    }

    let base = status.as_ref().and_then(|s| s.problem.clone()).or_else(|| (!site.site.enabled).then(|| "Switch the app on first.".to_owned()));
    let svc_problem = if page.tailnet_own_name { service_problem(status.as_ref()) } else { None };
    let name_problem = if page.tailnet_own_name && !support::valid_service_name(&page.tailnet_service) {
        Some("Use lower-case letters, digits and hyphens.".to_owned())
    } else {
        None
    };
    let problem = base.or(svc_problem).or(name_problem);
    let tailnet_suffix = status.as_ref().and_then(|s| s.dns_name.as_deref()).and_then(|d| d.split_once('.')).map(|(_, rest)| rest.to_owned()).unwrap_or_else(|| "<tailnet>.ts.net".to_owned());
    ui.horizontal(|ui| {
        let text = problem.clone().unwrap_or_else(|| {
            if page.tailnet_own_name {
                format!("https://{}.{tailnet_suffix} on your other devices.", page.tailnet_service)
            } else {
                format!("Open it on your other devices with Tailscale, at {}:<port>.", status.as_ref().and_then(|s| s.host.clone()).unwrap_or_else(|| "this computer".to_owned()))
            }
        });
        ui.add(egui::Label::new(RichText::new(text).small().color(p.muted)).wrap());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, Kind::Outline, Some(lucide::NETWORK), "Open on tailnet", problem.is_none() && !busy).clicked() {
                let service = page.tailnet_own_name.then(|| page.tailnet_service.clone());
                let sid = id.clone();
                let domain = site.site.domain.clone();
                cx.actions.push(Action::Run(Task::reporting(format!("tailnet:{id}"), format!("Couldn't open {domain} on the tailnet"), move |c| {
                    let app = c.tailnet_open(&sid, service.as_deref())?;
                    Ok(vec![Note::new(Tone::Success, format!("{domain} is open on your tailnet"), app.url.unwrap_or_default())])
                })));
            }
        });
    });
}
