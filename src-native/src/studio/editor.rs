use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::text::{CCursor, CCursorRange, LayoutJob};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{self, vec2, Align, Color32, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout, Modifiers, Rect, RichText, Sense, Stroke, TextBuffer, Ui};
use zenv_core::app_core::Core;
use zenv_core::error::{AppError, AppResult};
use zenv_core::project_files;

use super::highlight::{self, Highlight};
use super::{hline, icons, Colors};
use crate::lucide;
use crate::widgets::{self, ModalChoice};

pub const EDIT_MAX: usize = 1024 * 1024;
pub const EDIT_LINE_MAX: usize = 40 * 1024;
const POLL: Duration = Duration::from_millis(1500);
const BG: Color32 = Color32::from_rgb(0x0d, 0x0e, 0x0e);
const GUTTER: Color32 = Color32::from_rgb(0x6e, 0x76, 0x81);
const GUTTER_ACTIVE: Color32 = Color32::from_rgb(0xc6, 0xc6, 0xc6);
const SELECTION: Color32 = Color32::from_rgb(0x26, 0x4f, 0x78);
const CARET: Color32 = Color32::from_rgb(0xae, 0xaf, 0xad);
const MATCH: Color32 = Color32::from_rgb(0x9e, 0x6a, 0x03);
const WARN_BG: Color32 = Color32::from_rgb(0x2a, 0x22, 0x10);
const WARN_FG: Color32 = Color32::from_rgb(0xf3, 0xd2, 0x7a);
const FAIL_BG: Color32 = Color32::from_rgb(0x2a, 0x12, 0x10);

pub struct Env<'a> {
    pub root: &'a Path,
    pub core: &'a Arc<Core>,
    pub site_id: &'a str,
}

pub fn formatter_for(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "go" => Some("gofmt"),
        "rs" => Some("rustfmt"),
        "py" | "pyi" => Some("ruff"),
        _ => None,
    }
}

pub fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(b, _)| b)
}

fn chars_before(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

pub fn indent_of(text: &str) -> &'static str {
    if text.lines().any(|l| l.starts_with('\t')) { "\t" } else { "    " }
}

struct Find {
    query: String,
    current: usize,
    focus: bool,
    scroll: bool,
    hits: Option<(u64, Vec<(usize, usize)>)>,
}

impl Find {
    fn hits(&mut self, text: &str) -> &[(usize, usize)] {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&(text, &self.query), &mut h);
        let key = std::hash::Hasher::finish(&h);
        if self.hits.as_ref().is_none_or(|(k, _)| *k != key) {
            self.hits = Some((key, find_all(text, &self.query)));
        }
        self.hits.as_ref().map_or(&[], |(_, v)| v.as_slice())
    }
}

struct Formatting {
    before: String,
    then_save: bool,
    result: Receiver<AppResult<String>>,
}

pub struct Doc {
    pub path: String,
    id: Id,
    pub text: String,
    saved: String,
    modified: u64,
    crlf: bool,
    pub read_only: bool,
    pub large: bool,
    pub error: Option<String>,
    pub conflict: bool,
    pub save_error: Option<String>,
    highlight: Highlight,
    job: Option<(u64, LayoutJob)>,
    indent: &'static str,
    pub cursor: (usize, usize),
    formatting: Option<Formatting>,
    pub format_error: Option<String>,
    pub format_missing: Option<String>,
    find: Option<Find>,
    focus: bool,
    fresh: bool,
    pub gone: bool,
    polled: Instant,
}

impl Doc {
    pub fn open(root: &Path, path: &str) -> Self {
        let mut doc = Doc {
            path: path.to_owned(),
            id: Id::unique(("studio-doc", root, path)),
            text: String::new(),
            saved: String::new(),
            modified: 0,
            crlf: false,
            read_only: false,
            large: false,
            error: None,
            conflict: false,
            save_error: None,
            highlight: Highlight::new(name_of(path), ""),
            job: None,
            indent: "    ",
            cursor: (1, 1),
            formatting: None,
            format_error: None,
            format_missing: None,
            find: None,
            focus: true,
            fresh: true,
            gone: false,
            polled: Instant::now(),
        };
        match project_files::read(root, path) {
            Ok(f) => doc.load(f.content, f.modified, f.read_only),
            Err(e) => doc.error = Some(e.to_string()),
        }
        doc
    }

    fn load(&mut self, content: String, modified: u64, read_only: bool) {
        self.large = content.len() > EDIT_MAX || highlight::longest_line(&content) > EDIT_LINE_MAX;
        self.read_only = read_only || self.large;
        if self.highlight.coloured() != highlight::colourable(&content) {
            self.highlight = Highlight::new(name_of(&self.path), &content);
            self.job = None;
        }
        self.crlf = content.contains("\r\n");
        self.text = if self.crlf { content.replace("\r\n", "\n") } else { content };
        self.saved.clone_from(&self.text);
        self.modified = modified;
        self.indent = indent_of(&self.text);
        self.conflict = false;
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn language(&self) -> Option<&'static str> {
        self.highlight.language()
    }

    pub fn reload(&mut self, root: &Path) {
        match project_files::read(root, &self.path) {
            Ok(f) => self.load(f.content, f.modified, f.read_only),
            Err(e) => self.save_error = Some(e.to_string()),
        }
    }

    pub fn write(&mut self, root: &Path, force: bool) {
        if self.read_only {
            return;
        }
        let content = if self.crlf { self.text.replace('\n', "\r\n") } else { self.text.clone() };
        match project_files::write(root, &self.path, &content, (!force).then_some(self.modified)) {
            Ok(modified) => {
                self.saved.clone_from(&self.text);
                self.modified = modified;
                self.conflict = false;
                self.save_error = None;
            }
            Err(AppError::Conflict(_)) => self.conflict = true,
            Err(e) => self.save_error = Some(e.to_string()),
        }
    }

    pub fn save(&mut self, env: &Env, format_on_save: bool, ctx: &egui::Context) {
        if format_on_save && formatter_for(&self.path).is_some() {
            self.format(env, true, ctx);
        } else {
            self.write(env.root, false);
        }
    }

    pub fn format(&mut self, env: &Env, then_save: bool, ctx: &egui::Context) {
        if self.read_only || self.formatting.is_some() || formatter_for(&self.path).is_none() {
            return;
        }
        self.format_error = None;
        self.format_missing = None;
        let (tx, rx) = channel();
        let (core, site, path, text, ctx) = (env.core.clone(), env.site_id.to_owned(), self.path.clone(), self.text.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(core.format_code(&site, &path, &text));
            ctx.request_repaint();
        });
        self.formatting = Some(Formatting { before: self.text.clone(), then_save, result: rx });
    }

    pub fn formatting(&self) -> bool {
        self.formatting.is_some()
    }

    fn poll_format(&mut self, root: &Path) {
        let Some(f) = &self.formatting else { return };
        let Ok(result) = f.result.try_recv() else { return };
        let Some(f) = self.formatting.take() else { return };
        match result {
            Ok(text) if self.text == f.before => self.text = text.replace("\r\n", "\n"),
            Ok(_) => {}
            Err(AppError::NotFound { entity: "formatter", id }) => {
                self.format_error = Some(format!("{id} isn't installed"));
                self.format_missing = Some(id);
            }
            Err(e) => self.format_error = Some(e.to_string()),
        }
        if f.then_save {
            self.write(root, false);
        }
    }

    pub fn poll_disk(&mut self, root: &Path) {
        self.polled = Instant::now();
        let Some(stat) = project_files::stat(root, std::slice::from_ref(&self.path)).first().copied() else { return };
        self.gone = stat.is_none();
        let Some(modified) = stat else { return };
        if modified == self.modified {
            return;
        }
        if self.dirty() {
            self.conflict = true;
        } else {
            self.reload(root);
        }
    }

    pub fn open_find(&mut self) {
        let query = self.find.take().map(|f| f.query).unwrap_or_default();
        self.find = Some(Find { query, current: 0, focus: true, scroll: true, hits: None });
    }


}

fn match_at(text: &str, start: usize, query: &[char]) -> Option<usize> {
    let mut k = 0;
    for (j, c) in text[start..].char_indices() {
        for l in c.to_lowercase() {
            if query.get(k) != Some(&l) {
                return None;
            }
            k += 1;
        }
        if k == query.len() {
            return Some(start + j + c.len_utf8());
        }
    }
    None
}

pub fn find_all(text: &str, query: &str) -> Vec<(usize, usize)> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let Some(&first) = query.first() else { return vec![] };
    let starts: Box<dyn Iterator<Item = usize>> = if first.is_ascii() {
        let b = first as u8;
        Box::new(text.bytes().enumerate().filter(move |(_, x)| x.to_ascii_lowercase() == b).map(|(i, _)| i))
    } else {
        Box::new(text.char_indices().filter(move |(_, c)| c.to_lowercase().next() == Some(first)).map(|(i, _)| i))
    };
    let mut out = Vec::new();
    let mut next = 0;
    for i in starts {
        if i < next {
            continue;
        }
        if let Some(end) = match_at(text, i, &query) {
            out.push((i, end));
            next = end;
        }
    }
    out
}

pub struct Editor {
    pub docs: Vec<Doc>,
    pub active: Option<String>,
    scroll_tab: bool,
    closing: Option<String>,
    pub format_on_save: bool,
    installing: Option<(String, Receiver<AppResult<String>>)>,
    install_error: Option<String>,
}

const FORMAT_ON_SAVE: &str = "zenv.studio.formatOnSave";

impl Editor {
    pub fn new(ctx: &egui::Context) -> Self {
        let format_on_save = ctx.data_mut(|d| d.get_persisted::<bool>(Id::unique(FORMAT_ON_SAVE))).unwrap_or(false);
        Self { docs: Vec::new(), active: None, scroll_tab: false, closing: None, format_on_save, installing: None, install_error: None }
    }

    pub fn open(&mut self, root: &Path, path: &str) {
        if !self.docs.iter().any(|d| d.path == path) {
            self.docs.push(Doc::open(root, path));
        }
        if let Some(d) = self.docs.iter_mut().find(|d| d.path == path) {
            d.focus = true;
        }
        self.active = Some(path.to_owned());
        self.scroll_tab = true;
    }

    pub fn close(&mut self, path: &str) {
        self.docs.retain(|d| d.path != path);
        if self.active.as_deref() == Some(path) {
            self.active = self.docs.last().map(|d| d.path.clone());
        }
    }

    pub fn current(&mut self) -> Option<&mut Doc> {
        let active = self.active.as_deref()?;
        self.docs.iter_mut().find(|d| d.path == active)
    }

    pub fn save_active(&mut self, env: &Env, ctx: &egui::Context) {
        let on_save = self.format_on_save;
        if let Some(d) = self.current() {
            d.save(env, on_save, ctx);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, env: &Env) {
        let ctx = ui.ctx().clone();
        if let Some(d) = self.current() {
            d.poll_format(env.root);
            if d.polled.elapsed() >= POLL {
                d.poll_disk(env.root);
            }
            ctx.request_repaint_after(POLL);
        }
        self.poll_install(env, &ctx);
        let (save, format, find) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::SHIFT | Modifiers::ALT, Key::F)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F)),
            )
        });
        if save {
            self.save_active(env, &ctx);
        }
        if let Some(d) = self.current() {
            if format {
                d.format(env, false, &ctx);
            }
            if find {
                d.open_find();
            }
        }
        let c = Colors::of(ui);
        self.tabs(ui, &c);
        hline(ui, c.line);
        let status_h = 24.0;
        let body = ui.available_rect_before_wrap().with_max_y(ui.available_rect_before_wrap().bottom() - status_h - 1.0);
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        match self.active.clone().and_then(|a| self.docs.iter_mut().find(|d| d.path == a)) {
            Some(doc) => doc_ui(&mut body_ui, doc, env, &c),
            None => empty(&mut body_ui, &c),
        }
        ui.advance_cursor_after_rect(body);
        hline(ui, c.line);
        self.status(ui, env, &c);
        self.confirm_close(&ctx);
    }

    fn tabs(&mut self, ui: &mut Ui, c: &Colors) {
        let mut activate = None;
        let mut close = None;
        let scroll = std::mem::take(&mut self.scroll_tab);
        ui.style_mut().always_scroll_the_only_direction = true;
        egui::ScrollArea::horizontal().id_salt("studio-tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(36.0);
                ui.spacing_mut().item_spacing.x = 0.0;
                for d in &self.docs {
                    let active = self.active.as_deref() == Some(d.path.as_str());
                    let (open, shut) = tab(ui, d, active && scroll, active, c);
                    if open {
                        activate = Some(d.path.clone());
                    }
                    if shut {
                        close = Some((d.path.clone(), d.dirty()));
                    }
                }
            });
        });
        if let Some(p) = activate {
            if let Some(d) = self.docs.iter_mut().find(|d| d.path == p) {
                d.focus = true;
            }
            self.active = Some(p);
            self.scroll_tab = true;
        }
        match close {
            Some((p, true)) => self.closing = Some(p),
            Some((p, false)) => self.close(&p),
            None => {}
        }
    }

    fn confirm_close(&mut self, ctx: &egui::Context) {
        let Some(path) = self.closing.clone() else { return };
        let title = format!("{} has unsaved changes", name_of(&path));
        match widgets::confirm_modal(ctx, "studio-close-dirty", &title, "Close anyway", true, false, |ui| {
            ui.label("Closing it throws the changes away. Ctrl+S saves them first.");
        }) {
            ModalChoice::Confirm => {
                self.closing = None;
                self.close(&path);
            }
            ModalChoice::Cancel => self.closing = None,
            ModalChoice::None => {}
        }
    }

    fn poll_install(&mut self, env: &Env, ctx: &egui::Context) {
        let Some((_, rx)) = &self.installing else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.installing = None;
        match result {
            Ok(_) => {
                if let Some(d) = self.current() {
                    d.format(env, false, ctx);
                }
            }
            Err(e) => self.install_error = Some(e.to_string()),
        }
    }

    fn status(&mut self, ui: &mut Ui, env: &Env, c: &Colors) {
        let ctx = ui.ctx().clone();
        let font = FontId::new(10.5, FontFamily::Monospace);
        let text = |s: &str, color: Color32| RichText::new(s).font(font.clone()).color(color);
        let mut format_on_save = self.format_on_save;
        let mut install = None;
        let mut format = false;
        let installing = self.installing.as_ref().map(|(t, _)| t.clone());
        let install_error = self.install_error.clone();
        ui.horizontal(|ui| {
            ui.set_height(24.0);
            ui.add_space(12.0);
            ui.spacing_mut().item_spacing.x = 16.0;
            let Some(d) = self.active.as_deref().and_then(|a| self.docs.iter().find(|d| d.path == a)) else {
                ui.label(text("No file open", c.muted));
                return;
            };
            ui.add(egui::Label::new(text(&d.path, c.muted)).truncate());
            if let Some(tool) = d.format_missing.as_deref().filter(|t| *t != "gofmt") {
                let label = match &installing {
                    Some(t) => format!("Installing {t}…"),
                    None => format!("Install {tool}"),
                };
                let hint = if tool == "rustfmt" { "rustup component add rustfmt" } else { "The official ruff release from GitHub, into the Zenv home" };
                let b = egui::Button::new(text(&label, c.led)).fill(c.led.gamma_multiply(0.15)).corner_radius(3.0);
                if ui.add_enabled(installing.is_none(), b).on_hover_text(hint).clicked() {
                    install = Some(tool.to_owned());
                }
            }
            if d.format_missing.as_deref() == Some("gofmt") {
                ui.label(text("Install Go in Zenv → Languages", c.muted));
            }
            if let Some(e) = &install_error {
                ui.add(egui::Label::new(text(e, c.fail)).truncate()).on_hover_text(e);
            }
            if let (Some(e), None) = (&d.format_error, &d.format_missing) {
                ui.add(egui::Label::new(text(e, c.fail)).truncate()).on_hover_text(e);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if d.read_only {
                    ui.label(text("read-only", c.muted));
                } else if d.dirty() {
                    ui.label(text("unsaved", c.led));
                } else {
                    ui.label(text("saved", c.muted));
                }
                if let (Some(tool), false) = (formatter_for(&d.path), d.read_only) {
                    let r = ui.add(egui::Checkbox::new(&mut format_on_save, text("format on save", c.muted)));
                    r.on_hover_text("Format the file each time it is saved");
                    let glyph = if d.formatting() { lucide::LOADER_CIRCLE } else { lucide::WAND_SPARKLES };
                    let b = egui::Button::new((widgets::icon(glyph, 11.0).color(c.muted), text(tool, c.muted))).frame_when_inactive(false);
                    if ui.add(b).on_hover_text(format!("Format with {tool} (Shift+Alt+F)")).clicked() {
                        format = true;
                    }
                }
                ui.label(text(d.language().unwrap_or("Plain text"), c.muted));
                ui.label(text(&format!("Ln {}, Col {}", d.cursor.0, d.cursor.1), c.muted));
            });
        });
        if format_on_save != self.format_on_save {
            self.format_on_save = format_on_save;
            ctx.data_mut(|d| d.insert_persisted(Id::unique(FORMAT_ON_SAVE), format_on_save));
        }
        if format {
            if let Some(d) = self.current() {
                d.format(env, false, &ctx);
            }
        }
        if let Some(tool) = install {
            let (tx, rx) = channel();
            let (core, site, ctx2) = (env.core.clone(), env.site_id.to_owned(), ctx.clone());
            let t = tool.clone();
            std::thread::spawn(move || {
                let _ = tx.send(core.install_formatter(&site, &t));
                ctx2.request_repaint();
            });
            self.install_error = None;
            self.installing = Some((tool, rx));
        }
    }
}

fn tab(ui: &mut Ui, d: &Doc, scroll: bool, active: bool, c: &Colors) -> (bool, bool) {
    let name = name_of(&d.path);
    let galley = ui.painter().layout_no_wrap(name.to_owned(), FontId::proportional(12.0), Color32::WHITE);
    let width = 12.0 + 14.0 + 6.0 + galley.size().x + 6.0 + 20.0 + 4.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    let resp = resp.on_hover_text(&d.path);
    if scroll {
        resp.scroll_to_me(None);
    }
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 0.0, c.raised.gamma_multiply(0.7));
        painter.rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), 2.0)), 0.0, c.led);
    }
    painter.vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0, c.line));
    let fg = if active { Color32::WHITE } else if resp.hovered() { c.text } else { c.muted };
    let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 12.0 + 7.0, rect.center().y), vec2(14.0, 14.0));
    if d.read_only {
        painter.text(icon_rect.center(), egui::Align2::CENTER_CENTER, lucide::LOCK, FontId::new(12.0, crate::theme::icons()), c.muted);
    } else {
        icons::image(icons::file_icon(name), 14.0).paint_at(ui, icon_rect);
    }
    let painter = ui.painter();
    painter.galley(egui::pos2(icon_rect.right() + 6.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    let close_rect = Rect::from_center_size(egui::pos2(rect.right() - 4.0 - 10.0, rect.center().y), vec2(20.0, 20.0));
    let close = ui.interact(close_rect, ui.scope_id().with(("tab-close", &d.path)), Sense::click());
    let close = close.on_hover_text(if d.dirty() { "Unsaved changes (Ctrl+S saves). Close anyway" } else { "Close" });
    let painter = ui.painter();
    if close.hovered() {
        painter.rect_filled(close_rect, 2.0, c.raised);
    }
    let group_hover = resp.hovered() || close.hovered();
    if d.dirty() && !group_hover {
        painter.circle_filled(close_rect.center(), 4.0, c.text);
    } else if active || group_hover {
        painter.text(close_rect.center(), egui::Align2::CENTER_CENTER, lucide::X, FontId::new(12.0, crate::theme::icons()), fg);
    }
    close.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Close {name}")));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, name));
    let middle = resp.clicked_by(egui::PointerButton::Middle);
    (resp.clicked() && !close.clicked(), close.clicked() || middle)
}

fn empty(ui: &mut Ui, c: &Colors) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, c.panel);
    ui.add_space((ui.available_height() / 2.0 - 50.0).max(0.0));
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.label(widgets::icon(lucide::FILE_CODE, 40.0).color(c.muted.gamma_multiply(0.4)));
        ui.label(RichText::new("Pick a file on the left to edit it.").size(14.0).color(c.muted));
        ui.label(RichText::new("Ctrl P go to file · Ctrl S save · Ctrl F find").font(FontId::new(11.0, FontFamily::Monospace)).color(c.muted));
    });
}

fn banner(ui: &mut Ui, bg: Color32, fg: Color32, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(bg).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.visuals_mut().override_text_color = Some(fg);
        ui.horizontal(body);
    });
}

fn doc_ui(ui: &mut Ui, doc: &mut Doc, env: &Env, c: &Colors) {
    if let Some(e) = &doc.error {
        ui.painter().rect_filled(ui.max_rect(), 0.0, c.panel);
        ui.centered_and_justified(|ui| ui.label(RichText::new(e).size(14.0).color(c.muted)));
        return;
    }
    if doc.conflict {
        let (mut theirs, mut mine) = (false, false);
        banner(ui, WARN_BG, WARN_FG, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                mine = ui.button(RichText::new("Keep mine").size(12.0)).clicked();
                theirs = ui.button(RichText::new("Load theirs").size(12.0)).clicked();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new("This file changed on disk (Claude Code or another program edited it) while you had unsaved changes.").size(12.0)).wrap());
                });
            });
        });
        if theirs {
            doc.reload(env.root);
        }
        if mine {
            doc.write(env.root, true);
        }
    }
    if doc.large {
        banner(ui, c.raised, c.muted, |ui| {
            ui.label(RichText::new("Over 1 MB or with very long lines: read-only here. Edit it in your editor (top right).").size(12.0));
        });
    }
    if doc.gone {
        banner(ui, WARN_BG, WARN_FG, |ui| {
            ui.label(RichText::new("This file is gone from disk (deleted or renamed).").size(12.0));
        });
    }
    if let Some(e) = doc.save_error.clone() {
        banner(ui, FAIL_BG, c.fail, |ui| {
            ui.label(RichText::new(format!("Couldn't save: {e}")).size(12.0));
        });
    }
    if doc.find.is_some() {
        find_bar(ui, doc, c);
    }
    code(ui, doc);
}


fn find_bar(ui: &mut Ui, doc: &mut Doc, c: &Colors) {
    let Some(f) = &mut doc.find else { return };
    let count = f.hits(&doc.text).len();
    let mut close = false;
    egui::Frame::new().fill(c.raised).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let before = f.query.clone();
            let r = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("Find").font(FontId::monospace(12.0)).desired_width(220.0).background_color(BG));
            if std::mem::take(&mut f.focus) {
                r.request_focus();
            }
            if f.query != before {
                f.current = 0;
                f.scroll = true;
            }
            let step = |f: &mut Find, back: bool| {
                if count > 0 {
                    f.current = if back { (f.current + count - 1) % count } else { (f.current + 1) % count };
                    f.scroll = true;
                }
            };
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                step(f, ui.input(|i| i.modifiers.shift));
                r.request_focus();
            }
            let label = match (f.query.is_empty(), count) {
                (true, _) => String::new(),
                (false, 0) => "No results".into(),
                _ => format!("{} of {count}", f.current.min(count - 1) + 1),
            };
            ui.label(RichText::new(label).font(FontId::monospace(11.0)).color(c.muted));
            if widgets::icon_button(ui, lucide::ARROW_UP, "Previous match (Shift+Enter)").clicked() {
                step(f, true);
            }
            if widgets::icon_button(ui, lucide::ARROW_DOWN, "Next match (Enter)").clicked() {
                step(f, false);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                close = widgets::icon_button(ui, lucide::X, "Close (Escape)").clicked() || ((r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(Key::Escape)));
            });
        });
    });
    if close {
        let current = f.current;
        let hit = f.hits(&doc.text).get(current).copied();
        doc.find = None;
        if let Some((a, b)) = hit {
            select(ui.ctx(), doc.id, chars_before(&doc.text, a), chars_before(&doc.text, b));
        }
        doc.focus = true;
    }
}

fn select(ctx: &egui::Context, id: Id, a: usize, b: usize) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
    state.store(ctx, id);
}

fn indent_with_spaces(ui: &mut Ui, doc: &mut Doc, id: Id) {
    if doc.read_only || doc.indent == "\t" || !ui.memory(|m| m.has_focus(id)) {
        return;
    }
    if !ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
        return;
    }
    let ctx = ui.ctx().clone();
    let Some(mut state) = TextEditState::load(&ctx, id) else { return };
    let Some(range) = state.cursor.char_range() else { return };
    let [a, b] = range.sorted_cursors();
    let (ba, bb) = (byte_at(&doc.text, a.index.0), byte_at(&doc.text, b.index.0));
    doc.text.replace_range(ba..bb, doc.indent);
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(a.index.0 + doc.indent.len()))));
    state.store(&ctx, id);
}

fn cached_job(highlight: &mut Highlight, cache: &mut Option<(u64, LayoutJob)>, text: &str) -> LayoutJob {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(text, &mut h);
    let hash = std::hash::Hasher::finish(&h);
    if cache.as_ref().is_none_or(|(h, _)| *h != hash) {
        *cache = Some((hash, highlight.job(text)));
    }
    cache.as_ref().map(|(_, j)| j.clone()).unwrap_or_default()
}

fn visible_chars(galley: &egui::Galley, visible: Rect) -> std::ops::Range<usize> {
    let (mut at, mut range) = (0, usize::MAX..0);
    for row in &galley.rows {
        let n = row.char_count_including_newline().0;
        let r = row.rect();
        if r.max.y >= visible.min.y && r.min.y <= visible.max.y {
            range = range.start.min(at)..at + n;
        }
        at += n;
    }
    range
}

fn match_rects(galley: &egui::Galley, text: &str, hits: &[(usize, usize)], visible: Rect, current: usize) -> Vec<(usize, Rect)> {
    let shown = visible_chars(galley, visible);
    let (mut byte, mut chars) = (0, 0);
    let mut out = Vec::new();
    for (i, &(a, b)) in hits.iter().enumerate() {
        chars += text[byte..a].chars().count();
        let ca = chars;
        chars += text[a..b].chars().count();
        byte = b;
        if i == current || shown.contains(&ca) {
            out.push((i, galley.pos_from_cursor(CCursor::new(ca)).union(galley.pos_from_cursor(CCursor::new(chars)))));
        }
    }
    out
}

fn code(ui: &mut Ui, doc: &mut Doc) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 0.0, BG);
    let id = doc.id;
    indent_with_spaces(ui, doc, id);
    let font = FontId::new(highlight::FONT_SIZE, FontFamily::Monospace);
    let digit = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let lines = doc.text.bytes().filter(|&b| b == b'\n').count() + 1;
    let gutter_w = (lines.to_string().len().max(2) as f32) * digit + 28.0;
    if std::mem::take(&mut doc.fresh) {
        select(ui.ctx(), id, 0, 0);
    }
    let Doc { path, text, highlight, job, read_only, find, focus, cursor, .. } = doc;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (gutter, _) = ui.allocate_exact_size(vec2(gutter_w, rect.height()), Sense::hover());
        let visuals = ui.visuals_mut();
        visuals.selection.bg_fill = SELECTION;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.text_cursor.stroke = Stroke::new(2.0, CARET);
        visuals.extreme_bg_color = BG;
        let out = egui::ScrollArea::both()
            .id_salt(id)
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, _wrap: f32| {
                    let mut j = cached_job(highlight, job, buf.as_str());
                    j.wrap.max_width = f32::INFINITY;
                    ui.fonts_mut(|f| f.layout_job(j))
                };
                let mut ro: &str = text.as_str();
                let edit = if *read_only { egui::TextEdit::multiline(&mut ro) } else { egui::TextEdit::multiline(text) };
                let out = edit
                    .id(id)
                    .font(font.clone())
                    .code_editor()
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 8, right: 12, top: 8, bottom: 8 }))
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size())
                    .layouter(&mut layouter)
                    .show(ui);
                if let Some(f) = find {
                    let shown = out.galley.text();
                    let visible = ui.clip_rect().translate(-out.galley_pos.to_vec2());
                    let wanted = f.current;
                    let hits = f.hits(shown);
                    let current = wanted.min(hits.len().saturating_sub(1));
                    let rects = match_rects(&out.galley, shown, hits, visible, current);
                    for (i, r) in rects {
                        let r = r.translate(out.galley_pos.to_vec2()).expand2(vec2(1.0, 0.0));
                        let fill = MATCH.gamma_multiply(if i == current { 0.45 } else { 0.2 });
                        ui.painter().rect(r, 1.0, fill, Stroke::new(1.0, MATCH), egui::StrokeKind::Outside);
                        if i == current && std::mem::take(&mut f.scroll) {
                            ui.scroll_to_rect(r, Some(Align::Center));
                        }
                    }
                }
                out
            })
            .inner;
        out.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, path.as_str()));
        if std::mem::take(focus) {
            out.response.request_focus();
        }
        let cursor_row = out.cursor_range.map(|r| out.galley.layout_from_cursor(r.primary).row);
        if let Some(r) = out.cursor_range {
            let lc = out.galley.layout_from_cursor(r.primary);
            *cursor = (lc.row + 1, lc.column.0 + 1);
        }
        let painter = ui.painter().with_clip_rect(gutter.intersect(ui.clip_rect()));
        let text_clip = out.text_clip_rect;
        for (i, row) in out.galley.rows.iter().enumerate() {
            let y = out.galley_pos.y + row.rect().min.y;
            if y > text_clip.bottom() {
                break;
            }
            if y + highlight::LINE_HEIGHT < text_clip.top() {
                continue;
            }
            let active = Some(i) == cursor_row && out.response.has_focus();
            if active {
                let band = Rect::from_min_max(egui::pos2(gutter.right(), y), egui::pos2(rect.right(), y + row.rect().height()));
                ui.painter().with_clip_rect(text_clip.with_min_x(gutter.right()).with_max_x(rect.right())).rect_filled(band, 0.0, Color32::from_white_alpha(8));
            }
            let pos = egui::pos2(gutter.right() - 12.0, y + row.rect().height() / 2.0);
            painter.text(pos, egui::Align2::RIGHT_CENTER, (i + 1).to_string(), font.clone(), if active { GUTTER_ACTIVE } else { GUTTER });
        }
    });
}
