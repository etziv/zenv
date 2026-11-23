use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Align, Color32, Layout, RichText, Sense, Ui};
use zenv_core::project_files::{self, Entry, NewKind};

use super::{icons, panel_icon_button, Colors};
use crate::lucide;

const REFRESH: Duration = Duration::from_secs(5);
const ROW: f32 = 24.0;

struct Draft {
    parent: String,
    kind: NewKind,
    name: String,
    focused: bool,
}

pub struct Tree {
    children: HashMap<String, Vec<Entry>>,
    expanded: BTreeSet<String>,
    pub selected_dir: String,
    draft: Option<Draft>,
    pub error: Option<String>,
    refreshed: Instant,
    listing: Option<Receiver<Listings>>,
}

type Listings = Vec<(String, Result<Vec<Entry>, String>)>;

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

impl Tree {
    pub fn new(root: &Path) -> Self {
        let mut tree = Self { children: HashMap::new(), expanded: BTreeSet::from([String::new()]), selected_dir: String::new(), draft: None, error: None, refreshed: Instant::now(), listing: None };
        tree.load(root, "");
        tree
    }

    fn load(&mut self, root: &Path, dir: &str) {
        match project_files::list(root, dir) {
            Ok(l) => {
                self.children.insert(dir.to_owned(), l.entries);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn refresh(&mut self, root: &Path, ctx: &egui::Context) {
        self.refreshed = Instant::now();
        if self.listing.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (root, dirs, ctx) = (root.to_owned(), self.expanded.clone(), ctx.clone());
        std::thread::spawn(move || {
            let lists: Listings = dirs.into_iter().map(|d| {
                let l = project_files::list(&root, &d).map(|l| l.entries).map_err(|e| e.to_string());
                (d, l)
            }).collect();
            let _ = tx.send(lists);
            ctx.request_repaint();
        });
        self.listing = Some(rx);
    }

    fn take_listing(&mut self) {
        let Some(Ok(lists)) = self.listing.as_ref().map(Receiver::try_recv) else { return };
        self.listing = None;
        self.error = None;
        for (dir, l) in lists {
            match l {
                Ok(entries) => {
                    self.children.insert(dir, entries);
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    pub fn is_expanded(&self, dir: &str) -> bool {
        self.expanded.contains(dir)
    }

    pub fn toggle(&mut self, root: &Path, dir: &str) {
        self.selected_dir = dir.to_owned();
        if !self.expanded.remove(dir) {
            self.expanded.insert(dir.to_owned());
            self.load(root, dir);
        }
    }

    pub fn start_draft(&mut self, root: &Path, kind: NewKind) {
        let parent = self.selected_dir.clone();
        self.expanded.insert(parent.clone());
        self.load(root, &parent);
        self.draft = Some(Draft { parent, kind, name: String::new(), focused: false });
    }

    pub fn finish_draft(&mut self, root: &Path, name: &str) -> Option<String> {
        let draft = self.draft.take()?;
        let clean = name.trim().replace('\\', "/");
        let clean = clean.trim_start_matches('/');
        if clean.is_empty() {
            return None;
        }
        let path = if draft.parent.is_empty() { clean.to_owned() } else { format!("{}/{clean}", draft.parent) };
        match project_files::create(root, &path, draft.kind) {
            Ok(entry) => {
                self.load(root, parent_of(&entry.path));
                if draft.kind == NewKind::File {
                    return Some(entry.path);
                }
                self.expanded.insert(entry.path.clone());
                self.load(root, &entry.path);
                None
            }
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }


    pub fn ui(&mut self, ui: &mut Ui, root: &Path, root_name: &str, active: Option<&str>) -> Option<String> {
        if self.refreshed.elapsed() >= REFRESH || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(true)))) {
            self.refresh(root, ui.ctx());
        }
        self.take_listing();
        ui.ctx().request_repaint_after(REFRESH);
        let c = Colors::of(ui);
        let mut open = None;
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 4, top: 0, bottom: 0 }).show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if panel_icon_button(ui, lucide::REFRESH_CW, "Refresh", false, 24.0, 14.0).clicked() {
                        self.refresh(root, ui.ctx());
                    }
                    if panel_icon_button(ui, lucide::FOLDER_PLUS, "New folder", false, 24.0, 14.0).clicked() {
                        self.start_draft(root, NewKind::Folder);
                    }
                    if panel_icon_button(ui, lucide::FILE_PLUS, "New file", false, 24.0, 14.0).clicked() {
                        self.start_draft(root, NewKind::File);
                    }
                    let label = RichText::new(crate::widgets::tracked(&root_name.to_uppercase())).size(10.0).family(crate::theme::medium()).color(c.muted);
                    let r = ui.with_layout(Layout::left_to_right(Align::Center), |ui| ui.add(egui::Label::new(label).truncate().sense(Sense::click())));
                    if r.inner.on_hover_text("Project folder").clicked() {
                        self.selected_dir.clear();
                    }
                });
            });
        });
        super::hline(ui, c.line);
        if let Some(err) = self.error.clone() {
            let r = egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(err).size(11.0).color(c.fail)).wrap().sense(Sense::click()))
            });
            if r.inner.on_hover_text("Dismiss").clicked() {
                self.error = None;
            }
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);
            self.dir_ui(ui, root, "", 0, active, &mut open);
            ui.add_space(4.0);
        });
        open
    }

    fn dir_ui(&mut self, ui: &mut Ui, root: &Path, dir: &str, depth: usize, active: Option<&str>, open: &mut Option<String>) {
        if self.draft.as_ref().is_some_and(|d| d.parent == dir) {
            if let Some(path) = self.draft_row(ui, root, depth) {
                *open = Some(path);
            }
        }
        let Some(entries) = self.children.get(dir).cloned() else {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0 + depth as f32 * 12.0);
                ui.label(RichText::new("…").size(11.0).color(Colors::of(ui).muted));
            });
            return;
        };
        for e in entries {
            if e.dir {
                let expanded = self.expanded.contains(&e.path);
                let selected = self.selected_dir == e.path;
                let glyph = if expanded { lucide::CHEVRON_DOWN } else { lucide::CHEVRON_RIGHT };
                if row(ui, depth, selected, Some(glyph), icons::folder_icon(&e.name, expanded), &e.name).on_hover_text(&e.path).clicked() {
                    self.toggle(root, &e.path);
                }
                if self.expanded.contains(&e.path) {
                    self.dir_ui(ui, root, &e.path, depth + 1, active, open);
                }
            } else if row(ui, depth, active == Some(e.path.as_str()), None, icons::file_icon(&e.name), &e.name).on_hover_text(&e.path).clicked() {
                self.selected_dir = parent_of(&e.path).to_owned();
                *open = Some(e.path.clone());
            }
        }
    }

    fn draft_row(&mut self, ui: &mut Ui, root: &Path, depth: usize) -> Option<String> {
        let c = Colors::of(ui);
        let draft = self.draft.as_mut()?;
        let icon = if draft.kind == NewKind::File { icons::file_icon(&draft.name) } else { icons::folder_icon(&draft.name, false) };
        let hint = if draft.kind == NewKind::File { "name.php" } else { "folder" };
        let mut done = None;
        ui.horizontal(|ui| {
            ui.set_height(ROW);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(8.0 + depth as f32 * 12.0 + 12.0);
            ui.add(icons::image(icon, 16.0));
            let edit = egui::TextEdit::singleline(&mut draft.name)
                .hint_text(hint)
                .font(egui::FontId::monospace(11.5))
                .text_color(Color32::WHITE)
                .background_color(c.raised)
                .desired_width(ui.available_width() - 8.0)
                .margin(vec2(4.0, 1.0));
            let r = ui.add(edit);
            if !draft.focused {
                r.request_focus();
                draft.focused = true;
            }
            ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(1.0, c.led.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(String::new());
            } else if r.lost_focus() {
                done = Some(draft.name.clone());
            }
        });
        done.and_then(|name| self.finish_draft(root, &name))
    }
}

fn row(ui: &mut Ui, depth: usize, selected: bool, chevron: Option<&str>, icon: u16, name: &str) -> egui::Response {
    let c = Colors::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let fill = if selected {
        c.raised
    } else if resp.hovered() {
        c.raised.gamma_multiply(0.6)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(0.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    child.spacing_mut().item_spacing.x = 6.0;
    child.add_space(8.0 + depth as f32 * 12.0);
    match chevron {
        Some(g) => {
            child.add_sized(vec2(12.0, ROW), egui::Label::new(crate::widgets::icon(g, 12.0).color(c.muted)).selectable(false));
        }
        None => child.add_space(12.0),
    }
    child.add(icons::image(icon, 16.0));
    let text = RichText::new(name).size(12.5).color(if selected { Color32::WHITE } else { c.text });
    child.add(egui::Label::new(text).truncate().selectable(false));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    resp
}
