use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Color32, FontFamily, FontId, Key, RichText, Sense, Ui};
use zenv_core::project_files::{self, Entry};

use super::{icons, Colors};

const DEBOUNCE: Duration = Duration::from_millis(120);

pub enum Out {
    Close,
    Open(String),
}

#[derive(Default)]
pub struct QuickOpen {
    pub query: String,
    pub hits: Vec<Entry>,
    index: usize,
    typed: Option<Instant>,
    search: Option<Receiver<(String, Vec<Entry>)>>,
    focused: bool,
    moved: bool,
}

impl QuickOpen {
    fn search(&mut self, root: &Path, ctx: &egui::Context) {
        let q = self.query.trim().to_owned();
        if q.is_empty() {
            self.hits.clear();
            return;
        }
        let (tx, rx) = channel();
        let (root, ctx): (PathBuf, _) = (root.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            let hits = project_files::search(&root, &q).unwrap_or_default();
            let _ = tx.send((q, hits));
            ctx.request_repaint();
        });
        self.search = Some(rx);
    }

    fn poll(&mut self, root: &Path, ctx: &egui::Context) {
        if self.typed.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
            self.typed = None;
            self.search(root, ctx);
        } else if let Some(t) = self.typed {
            ctx.request_repaint_after(DEBOUNCE.saturating_sub(t.elapsed()));
        }
        if let Some(Ok((q, hits))) = self.search.as_ref().map(Receiver::try_recv) {
            self.search = None;
            if q == self.query.trim() {
                self.hits = hits;
                self.index = 0;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, root: &Path) -> Option<Out> {
        let ctx = ui.ctx().clone();
        self.poll(root, &ctx);
        let c = Colors::of(ui);
        let screen = ui.max_rect();
        let mut out = None;
        let (down, up, enter, escape) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.key_pressed(Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if down {
            self.index = (self.index + 1).min(self.hits.len().saturating_sub(1));
        }
        if up {
            self.index = self.index.saturating_sub(1);
        }
        self.moved |= down || up;
        if escape {
            return Some(Out::Close);
        }
        let width = (screen.width() * 0.9).min(560.0);
        let id = egui::Id::unique("studio-quick-open");
        let area = egui::Modal::default_area(id).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 64.0));
        let modal = egui::Modal::new(id).area(area).frame(egui::Frame::NONE).backdrop_color(Color32::from_black_alpha(102)).show(&ctx, |ui| {
            egui::Frame::new()
                .fill(c.raised)
                .stroke(egui::Stroke::new(1.0, c.line))
                .corner_radius(8.0)
                .shadow(egui::Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(120) })
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let before = self.query.clone();
                    let edit = egui::TextEdit::singleline(&mut self.query)
                        .hint_text(RichText::new("Go to file… (name or part of its path)").color(c.muted))
                        .font(FontId::new(13.0, FontFamily::Monospace))
                        .text_color(Color32::WHITE)
                        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 11)))
                        .desired_width(width);
                    let r = ui.add(edit);
                    if !self.focused {
                        r.request_focus();
                        self.focused = true;
                    }
                    if self.query != before {
                        self.typed = Some(Instant::now());
                        if self.query.trim().is_empty() {
                            self.hits.clear();
                        }
                    }
                    if enter && r.lost_focus() {
                        match self.hits.get(self.index) {
                            Some(h) => out = Some(Out::Open(h.path.clone())),
                            None => r.request_focus(),
                        }
                    }
                    super::hline(ui, c.line);
                    let list_h = (self.hits.len() as f32 * 26.0 + 8.0).min(320.0);
                    let moved = std::mem::take(&mut self.moved);
                    egui::ScrollArea::vertical().max_height(320.0).min_scrolled_height(list_h).show(ui, |ui| {
                        ui.add_space(4.0);
                        let mut hovered = None;
                        for (i, h) in self.hits.iter().enumerate() {
                            let r = hit_row(ui, h, i == self.index, &c);
                            if moved && i == self.index {
                                r.scroll_to_me(None);
                            }
                            if r.clicked() {
                                out = Some(Out::Open(h.path.clone()));
                            }
                            if r.hovered() {
                                hovered = Some(i);
                            }
                        }
                        if let Some(i) = hovered {
                            self.index = i;
                        }
                        if !self.query.trim().is_empty() && self.hits.is_empty() && self.search.is_none() && self.typed.is_none() {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                                ui.label(RichText::new("No matching file.").size(12.0).color(c.muted));
                            });
                        }
                        ui.add_space(4.0);
                    });
                });
        });
        if modal.should_close() && out.is_none() {
            out = Some(Out::Close);
        }
        out
    }
}

fn hit_row(ui: &mut Ui, h: &Entry, selected: bool, c: &Colors) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 0.0, c.led.gamma_multiply(0.15));
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.spacing_mut().item_spacing.x = 8.0;
    child.add(icons::image(icons::file_icon(&h.name), 16.0));
    child.label(RichText::new(&h.name).size(12.5).color(if selected { Color32::WHITE } else { c.text }));
    child.add(egui::Label::new(RichText::new(&h.path).font(FontId::new(11.0, FontFamily::Monospace)).color(c.muted)).truncate());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &h.path));
    resp
}
