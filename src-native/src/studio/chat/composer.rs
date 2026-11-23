use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, Rect, RichText, Sense, Stroke, Ui};
use zenv_core::studio_chat::{Attachment, AttachmentKind, ChatOptions, PermissionMode};

use super::model::Limits;
use super::options::{self, Catalog, Command, Model, EFFORTS, MODES};
use super::palette::{self, Palette};
use super::{job, take, Job, Mcp};
use crate::studio::{icons, Colors};
use crate::{lucide, theme};

pub enum Action {
    Send(String, Vec<Attachment>),
    Stop,
    Options(ChatOptions),
    Clear,
    Export,
    Terminal(Option<String>),
    Instructions,
    Docs,
    McpLoad,
    McpToggle(String, bool),
    McpReconnect(String),
}

pub struct Env<'a> {
    pub busy: bool,
    pub commands: &'a [Command],
    pub models: &'a [Model],
    pub catalog: Option<&'a Catalog>,
    pub running_model: Option<&'a str>,
    pub reported_mode: Option<&'a str>,
    pub limits: Limits,
    pub active_file: Option<&'a str>,
    pub root: &'a Path,
    pub options: &'a ChatOptions,
    pub mcp: &'a Mcp,
    pub focus: bool,
    pub repaint: &'a Arc<dyn Fn() + Send + Sync>,
}

pub struct Pending {
    pub id: u64,
    pub file: Attachment,
    thumb: Option<(String, Arc<[u8]>)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuKind {
    Slash,
    Mention,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Menu {
    pub kind: MenuKind,
    pub query: String,
    pub start: usize,
}

const MAX_FILES: usize = 10;
const MAX_HEIGHT: f32 = 220.0;
const SEARCH_DELAY: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct Composer {
    pub text: String,
    pub files: Vec<Pending>,
    pub error: Option<String>,
    pub menu: Option<Menu>,
    found: Vec<String>,
    search_due: Option<(String, Instant)>,
    search_job: Job<(String, Vec<String>)>,
    pick: usize,
    pub palette: Option<Palette>,
    next_id: u64,
    picker_job: Job<Vec<PathBuf>>,
    paste_down: bool,
    caret_to: Option<usize>,
    pub focused: bool,
    pub wants_catalog: bool,
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(i, _)| i)
}

pub fn menu_at(value: &str, caret: usize) -> Option<Menu> {
    let before = &value[..byte_at(value, caret)];
    if let Some(q) = before.strip_prefix('/').filter(|q| !q.chars().any(char::is_whitespace)) {
        return Some(Menu { kind: MenuKind::Slash, query: q.to_owned(), start: 0 });
    }
    let at = before.rfind('@')?;
    let query = &before[at + 1..];
    let starts_word = before[..at].chars().next_back().is_none_or(char::is_whitespace);
    (starts_word && !query.chars().any(|c| c.is_whitespace() || c == '@')).then(|| Menu { kind: MenuKind::Mention, query: query.to_owned(), start: caret - query.chars().count() - 1 })
}

pub fn slash_options(commands: &[Command], query: &str) -> Vec<(String, Option<String>)> {
    let q = query.to_lowercase();
    let mut hits: Vec<&Command> = commands.iter().filter(|c| c.name.to_lowercase().contains(&q)).collect();
    hits.sort_by_key(|c| !c.name.to_lowercase().starts_with(&q));
    hits.into_iter().take(10).map(|c| (c.name.clone(), c.description.clone())).collect()
}

fn paste_pressed() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL};
        let down = |k: i32| unsafe { GetKeyState(k) } < 0;
        down(VK_CONTROL as i32) && down(0x56)
    }
    #[cfg(not(windows))]
    false
}

impl Composer {
    fn options(&self, env: &Env) -> Vec<(String, Option<String>)> {
        match &self.menu {
            Some(Menu { kind: MenuKind::Slash, query, .. }) => slash_options(env.commands, query),
            Some(Menu { kind: MenuKind::Mention, .. }) => self.found.iter().map(|f| (f.clone(), None)).collect(),
            None => Vec::new(),
        }
    }

    pub fn update_menu(&mut self, caret: usize, active_file: Option<&str>) {
        let next = menu_at(&self.text, caret);
        if next.as_ref().map(|m| (m.kind, &m.query)) != self.menu.as_ref().map(|m| (m.kind, &m.query)) {
            self.pick = 0;
            match &next {
                Some(Menu { kind: MenuKind::Mention, query, .. }) if query.is_empty() => {
                    self.found = active_file.map(|f| vec![f.to_owned()]).unwrap_or_default();
                    self.search_due = None;
                }
                Some(Menu { kind: MenuKind::Mention, query, .. }) => self.search_due = Some((query.clone(), Instant::now() + SEARCH_DELAY)),
                _ => self.search_due = None,
            }
        }
        self.menu = next;
    }

    fn insert(&mut self, value: &str, at: usize, remove_to: usize) {
        let (a, b) = (byte_at(&self.text, at), byte_at(&self.text, remove_to));
        self.text = format!("{}{value}{}", &self.text[..a], &self.text[b..]);
        self.menu = None;
        self.caret_to = Some(at + value.chars().count());
    }

    pub fn choose(&mut self, choice: &str, caret: usize) {
        let Some(menu) = self.menu.clone() else { return };
        match menu.kind {
            MenuKind::Slash => self.insert(&format!("/{choice} "), 0, caret),
            MenuKind::Mention => self.insert(&format!("@{choice} "), menu.start, caret),
        }
    }

    pub fn start_with(&mut self, prefix: char, caret: usize, active_file: Option<&str>) {
        if prefix == '/' {
            self.text = "/".into();
            self.caret_to = Some(1);
            self.update_menu(1, active_file);
            return;
        }
        let b = byte_at(&self.text, caret);
        let before = &self.text[..b];
        let pad = if !before.is_empty() && !before.ends_with(char::is_whitespace) { " @" } else { "@" };
        self.text = format!("{before}{pad}{}", &self.text[b..]);
        let caret = caret + pad.chars().count();
        self.caret_to = Some(caret);
        self.update_menu(caret, active_file);
    }

    pub fn add(&mut self, ctx: &egui::Context, found: Vec<Result<Attachment, String>>) {
        self.error = None;
        for f in found {
            match f {
                Ok(file) => {
                    let thumb = (file.kind == AttachmentKind::Image)
                        .then(|| base64::engine::general_purpose::STANDARD.decode(&file.data).ok())
                        .flatten()
                        .map(|bytes| (format!("bytes://chat-attachment-{}-{}", self.next_id, file.name), Arc::<[u8]>::from(bytes)));
                    self.files.push(Pending { id: self.next_id, file, thumb });
                    self.next_id += 1;
                }
                Err(e) => self.error = Some(e),
            }
        }
        for gone in self.files.drain(MAX_FILES.min(self.files.len())..) {
            if let Some((uri, _)) = gone.thumb {
                ctx.forget_image(&uri);
            }
        }
    }

    fn remove(&mut self, ctx: &egui::Context, id: u64) {
        if let Some(i) = self.files.iter().position(|f| f.id == id) {
            if let Some((uri, _)) = self.files.remove(i).thumb {
                ctx.forget_image(&uri);
            }
        }
    }

    pub fn submit(&mut self, ctx: &egui::Context) -> Option<Action> {
        let t = self.text.trim().to_owned();
        if t.is_empty() && self.files.is_empty() {
            return None;
        }
        let files = std::mem::take(&mut self.files);
        for f in &files {
            if let Some((uri, _)) = &f.thumb {
                ctx.forget_image(uri);
            }
        }
        self.text.clear();
        self.menu = None;
        self.error = None;
        Some(Action::Send(t, files.into_iter().map(|f| f.file).collect()))
    }

    fn intake(&mut self, ui: &Ui, focused: bool) {
        let ctx = ui.ctx().clone();
        let (dropped, images) = ui.input(|i| {
            let images: Vec<Arc<egui::ColorImage>> = i.events.iter().filter_map(|e| if let egui::Event::PasteImage(img) = e { Some(img.clone()) } else { None }).collect();
            (i.raw.dropped_files.clone(), images)
        });
        let mut found: Vec<Result<Attachment, String>> = dropped.iter().map(|d| options::attachment_from_path(d.path())).collect();
        if focused {
            found.extend(images.iter().map(|img| options::attachment_from_image(img)));
            let down = paste_pressed();
            if down && !self.paste_down {
                let files = arboard::Clipboard::new().and_then(|mut c| c.get().file_list()).unwrap_or_default();
                found.extend(files.iter().map(|p| options::attachment_from_path(p)));
            }
            self.paste_down = down;
        }
        if let Some(paths) = take(&mut self.picker_job) {
            found.extend(paths.iter().map(|p| options::attachment_from_path(p)));
        }
        if !found.is_empty() {
            self.add(&ctx, found);
        }
    }

    fn poll_search(&mut self, env: &Env) {
        if let Some((query, due)) = self.search_due.clone() {
            if Instant::now() >= due {
                self.search_due = None;
                let root = env.root.to_path_buf();
                self.search_job = job(env.repaint, move || {
                    let found = zenv_core::project_files::search(&root, &query).map(|r| r.into_iter().filter(|e| !e.dir).take(8).map(|e| e.path).collect()).unwrap_or_default();
                    (query, found)
                });
            }
        }
        if let Some((query, found)) = take(&mut self.search_job) {
            if self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Mention && m.query == query) {
                self.found = found;
                self.pick = 0;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, env: &Env, c: &Colors) -> Vec<Action> {
        let ctx = ui.ctx().clone();
        let mut out = Vec::new();
        let edit_id = ui.scope_id().with("chat-composer-text");
        if env.focus {
            ui.memory_mut(|m| m.request_focus(edit_id));
        }
        let focused = ui.memory(|m| m.has_focus(edit_id));
        self.focused = focused || self.palette.is_some();
        self.intake(ui, focused);
        self.poll_search(env);
        if self.search_due.is_some() {
            ctx.request_repaint_after(SEARCH_DELAY);
        }
        let caret_now = || egui::text_edit::TextEditState::load(&ctx, edit_id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
        let choices = self.options(env);
        if focused && self.palette.is_none() {
            let caret = caret_now().unwrap_or(self.text.chars().count());
            let n = choices.len();
            let (down, up, pick, esc, enter) = ui.input_mut(|i| {
                let open = self.menu.is_some() && n > 0;
                let plain = !i.modifiers.shift;
                (
                    open && i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    open && i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    open && plain && (i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Tab)),
                    self.menu.is_some() && i.consume_key(Modifiers::NONE, Key::Escape),
                    plain && i.consume_key(Modifiers::NONE, Key::Enter),
                )
            });
            if down || up {
                self.pick = (self.pick + if down { 1 } else { n - 1 }) % n;
            }
            if pick {
                let value = choices.get(self.pick).or(choices.first()).map(|(v, _)| v.clone()).unwrap_or_default();
                self.choose(&value, caret);
            } else if esc {
                self.menu = None;
            } else if enter {
                out.extend(self.submit(&ctx));
            }
        }
        if let Some(caret) = self.caret_to.take() {
            let mut state = egui::text_edit::TextEditState::load(&ctx, edit_id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(caret))));
            state.store(&ctx, edit_id);
            ui.memory_mut(|m| m.request_focus(edit_id));
        }

        let mut box_rect = Rect::NOTHING;
        egui::Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let border = if focused { c.muted.gamma_multiply(0.7) } else { c.line };
            let r = egui::Frame::new().fill(c.raised).stroke(Stroke::new(1.0, border)).corner_radius(CornerRadius::same(8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                self.chips(ui, c);
                let hint = if env.busy { "Claude is working… (your message waits its turn)" } else { "Ask Claude Code… (@ file, / command, paste files)" };
                let output = egui::ScrollArea::vertical().id_salt(edit_id.with("scroll")).max_height(MAX_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                    egui::TextEdit::multiline(&mut self.text)
                        .id(edit_id)
                        .frame(egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 10, bottom: 4 }))
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(13.0))
                        .text_color(Color32::WHITE)
                        .hint_text(RichText::new(hint).color(c.muted))
                        .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                        .show(ui)
                });
                let output = output.inner;
                if output.response.changed() || output.response.clicked() {
                    let caret = output.cursor_range.map_or(self.text.chars().count(), |r| r.primary.index.0);
                    self.update_menu(caret, env.active_file);
                }
                if let Some(e) = &self.error {
                    egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 0, bottom: 4 }).show(ui, |ui| {
                        ui.label(RichText::new(e).size(11.0).color(c.fail));
                    });
                }
                egui::Frame::new().inner_margin(Margin { left: 6, right: 6, top: 2, bottom: 6 }).show(ui, |ui| self.toolbar(ui, env, c, &mut out, caret_now));
            });
            box_rect = r.response.rect;
        });

        let caret = caret_now().unwrap_or(self.text.chars().count());
        if let Some(p) = &mut self.palette {
            let acts = p.ui(&ctx, ui.scope_id().with("palette"), box_rect, env, c);
            for a in acts {
                match a {
                    palette::Out::Act(a) => out.push(a),
                    palette::Out::Close => {
                        self.palette = None;
                        ui.memory_mut(|m| m.request_focus(edit_id));
                    }
                    palette::Out::Attach => self.pick_files(env),
                    palette::Out::Mention => self.start_with('@', caret, env.active_file),
                    palette::Out::Commands => self.start_with('/', caret, env.active_file),
                }
            }
        } else if self.menu.is_some() && !choices.is_empty() {
            if let Some(value) = self.menu_ui(&ctx, ui.scope_id().with("menu"), box_rect, &choices, c) {
                self.choose(&value, caret);
            }
        }
        out
    }

    fn pick_files(&mut self, env: &Env) {
        self.picker_job = job(env.repaint, || rfd::FileDialog::new().set_title("Attach files").pick_files().unwrap_or_default());
    }

    fn chips(&mut self, ui: &mut Ui, c: &Colors) {
        if self.files.is_empty() {
            return;
        }
        let mut remove = None;
        egui::Frame::new().inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 0 }).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for f in &self.files {
                    egui::Frame::new().fill(c.panel).stroke(Stroke::new(1.0, c.line)).corner_radius(CornerRadius::same(4)).inner_margin(Margin { left: 4, right: 2, top: 2, bottom: 2 }).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            match &f.thumb {
                                Some((uri, bytes)) => {
                                    ui.add(egui::Image::from_bytes(uri.clone(), egui::load::Bytes::Shared(bytes.clone())).fit_to_exact_size(vec2(20.0, 20.0)).corner_radius(2));
                                }
                                None => {
                                    ui.label(RichText::new(lucide::FILE).font(FontId::new(13.0, theme::icons())).color(c.muted));
                                }
                            }
                            ui.add(egui::Label::new(RichText::new(&f.file.name).size(11.0).color(c.text)).truncate()).on_hover_text(&f.file.name);
                            if crate::studio::panel_icon_button(ui, lucide::X, &format!("Remove {}", f.file.name), false, 20.0, 12.0).clicked() {
                                remove = Some(f.id);
                            }
                        });
                    });
                }
            });
        });
        if let Some(id) = remove {
            self.remove(ui.ctx(), id);
        }
    }

    fn menu_ui(&mut self, ctx: &egui::Context, id: Id, above: Rect, choices: &[(String, Option<String>)], c: &Colors) -> Option<String> {
        let kind = self.menu.as_ref().map(|m| m.kind)?;
        let mut chosen = None;
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos2(above.left(), above.top() - 4.0)).pivot(Align2::LEFT_BOTTOM).show(ctx, |ui| {
            egui::Frame::new().fill(c.raised).stroke(Stroke::new(1.0, c.line)).corner_radius(CornerRadius::same(theme::RADIUS)).inner_margin(Margin::same(4)).show(ui, |ui| {
                ui.set_width(above.width() - 8.0);
                egui::ScrollArea::vertical().max_height(256.0).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, (value, hint)) in choices.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListItem, true, i == self.pick, value));
                        if r.hovered() {
                            self.pick = i;
                        }
                        if i == self.pick {
                            ui.painter().rect_filled(rect, CornerRadius::same(4), c.line);
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                        }
                        let y = rect.center().y;
                        let p = ui.painter().with_clip_rect(rect);
                        match kind {
                            MenuKind::Slash => {
                                p.text(pos2(rect.left() + 8.0 + 6.0, y), Align2::CENTER_CENTER, lucide::SLASH, FontId::new(12.0, theme::icons()), c.muted);
                            }
                            MenuKind::Mention => {
                                let name = value.rsplit('/').next().unwrap_or(value);
                                icons::image(icons::file_icon(name), 14.0).paint_at(ui, Rect::from_center_size(pos2(rect.left() + 8.0 + 7.0, y), vec2(14.0, 14.0)));
                            }
                        }
                        let color = if i == self.pick { Color32::WHITE } else { c.text };
                        let name = p.text(pos2(rect.left() + 30.0, y), Align2::LEFT_CENTER, value, FontId::new(12.0, FontFamily::Monospace), color);
                        if let Some(h) = hint {
                            p.text(pos2(name.right() + 8.0, y), Align2::LEFT_CENTER, h, FontId::proportional(11.0), c.muted);
                        }
                        if r.clicked() {
                            chosen = Some(value.clone());
                        }
                    }
                });
            });
        });
        chosen
    }

    fn toolbar(&mut self, ui: &mut Ui, env: &Env, c: &Colors, out: &mut Vec<Action>, caret_now: impl Fn() -> Option<usize>) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let attach = chip(ui, Some(lucide::PLUS), None, "Attach", false, c, None);
            let caret = caret_now().unwrap_or(self.text.chars().count());
            egui::Popup::menu(&attach).align(egui::RectAlign::TOP_START).show(|ui| {
                if crate::widgets::menu_item(ui, lucide::PAPERCLIP, "Upload files or images…", true, false) {
                    self.pick_files(env);
                }
                if crate::widgets::menu_item(ui, lucide::AT_SIGN, "Mention a project file", true, false) {
                    self.start_with('@', caret, env.active_file);
                }
                if let Some(file) = env.active_file {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    if crate::widgets::menu_item(ui, lucide::FILE_PLUS, format!("Add {name}"), true, false) {
                        let end = self.text.chars().count();
                        self.insert(&format!("@{file} "), end, end);
                    }
                }
            });
            let open = self.palette.is_some();
            if chip(ui, Some(lucide::SLASH), None, "Actions: model, MCP servers, account, settings…", open, c, None).clicked() {
                self.palette = if open { None } else { Some(Palette::default()) };
                self.wants_catalog = true;
            }
            usage(ui, env.limits, c);
            self.model_menu(ui, env, c, out);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let empty = self.text.trim().is_empty() && self.files.is_empty();
                if env.busy && empty {
                    if square_button(ui, lucide::SQUARE, "Stop", c.line, Color32::WHITE, true).clicked() {
                        out.push(Action::Stop);
                    }
                } else if square_button(ui, lucide::ARROW_UP, "Send (Enter)", c.led, Color32::BLACK, !empty).clicked() {
                    out.extend(self.submit(ui.ctx()));
                }
                mode_menu(ui, env, c, out);
            });
        });
    }

    fn model_menu(&mut self, ui: &mut Ui, env: &Env, c: &Colors, out: &mut Vec<Action>) {
        let o = env.options;
        let chosen = env.models.iter().find(|m| m.value == o.model.as_deref().unwrap_or("default"));
        let fallback = env.models.iter().find(|m| m.value == "default");
        let model = match &o.model {
            Some(m) => chosen.map_or(m.clone(), |c| c.display_name.clone()),
            None => options::pretty_model(env.running_model).or_else(|| options::pretty_model(fallback.and_then(|f| f.resolved_model.as_deref()))).unwrap_or_else(|| "Default".into()),
        };
        let levels = chosen.and_then(|m| m.supported_effort_levels.clone());
        let fixed = chosen.is_some_and(|m| m.resolved_model.is_some()) && levels.is_none();
        let effort = o.effort.map(|e| options::effort_label(Some(e)));
        let r = chip(ui, None, Some((&model, effort)), "Model and effort", false, c, Some(c.line.gamma_multiply(0.5)));
        self.wants_catalog |= r.clicked();
        egui::Popup::menu(&r).width(288.0).align(egui::RectAlign::TOP_START).show(|ui| {
            ui.set_max_width(288.0);
            let muted = theme::palette(ui).muted;
            egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.7).show(ui, |ui| {
                ui.label(RichText::new("Model").size(11.0).color(muted));
                for m in env.models {
                    let value = (m.value != "default").then(|| m.value.clone());
                    if choice(ui, o.model == value, &m.display_name, m.description.as_deref()) {
                        out.push(Action::Options(ChatOptions { model: value, ..o.clone() }));
                    }
                }
                ui.separator();
                ui.label(RichText::new("Effort").size(11.0).color(muted));
                if fixed {
                    ui.label(RichText::new("This model has no effort setting.").size(11.0).color(muted));
                }
                for (value, label) in EFFORTS {
                    let allowed = value.is_none() || (!fixed && levels.as_ref().is_none_or(|l| value.is_some_and(|v| l.contains(&v))));
                    if allowed && choice(ui, o.effort == value, label, None) {
                        out.push(Action::Options(ChatOptions { effort: value, ..o.clone() }));
                    }
                }
            });
        });
    }
}

fn mode_icon(mode: Option<&str>) -> &'static str {
    match mode {
        Some("manual" | "default") => lucide::HAND,
        Some("acceptEdits") => lucide::FILE_PEN,
        Some("auto") => lucide::ZAP,
        Some("plan") => lucide::LIST_TODO,
        Some("dontAsk") => lucide::BAN,
        Some("bypassPermissions") => lucide::SHIELD_OFF,
        _ => lucide::SETTINGS_2,
    }
}

pub fn mode_label(chosen: Option<PermissionMode>, reported: Option<&str>) -> &'static str {
    if let Some(m) = chosen {
        return MODES.iter().find(|x| x.value == Some(m)).map_or("Settings", |x| x.label);
    }
    match reported {
        Some("default" | "manual") => "Ask first",
        Some(r) => options::mode_of(r).and_then(|m| MODES.iter().find(|x| x.value == Some(m))).map_or("Settings", |x| x.label),
        None => "Settings",
    }
}

fn mode_menu(ui: &mut Ui, env: &Env, c: &Colors, out: &mut Vec<Action>) {
    let o = env.options;
    let mode = o.mode.map(options::mode_name).or(env.reported_mode);
    let info = MODES.iter().find(|m| m.value == o.mode).unwrap_or(&MODES[0]);
    let bypass = mode == Some("bypassPermissions");
    let r = chip(ui, Some(mode_icon(mode)), Some((mode_label(o.mode, env.reported_mode), None)), &format!("Permission mode: {}", info.hint), false, c, None);
    if bypass {
        ui.painter().rect_stroke(r.rect, CornerRadius::same(theme::RADIUS), Stroke::new(1.0, c.fail.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    }
    egui::Popup::menu(&r).width(256.0).align(egui::RectAlign::TOP_END).show(|ui| {
        ui.set_min_width(256.0);
        for m in &MODES {
            let icon = mode_icon(m.value.map(options::mode_name));
            if mode_row(ui, icon, m.label, m.hint, o.mode == m.value) {
                out.push(Action::Options(ChatOptions { mode: m.value, ..o.clone() }));
            }
        }
    });
}

fn mode_row(ui: &mut Ui, icon: &str, label: &str, hint: &str, on: bool) -> bool {
    let p = theme::palette(ui);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::MenuItem, true, on, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(4), p.accent);
    }
    let painter = ui.painter();
    painter.text(pos2(rect.left() + 8.0 + 7.0, rect.top() + 12.0), Align2::CENTER_CENTER, icon, FontId::new(14.0, theme::icons()), p.muted);
    painter.text(pos2(rect.left() + 30.0, rect.top() + 12.0), Align2::LEFT_CENTER, label, FontId::proportional(12.5), if on { p.primary } else { p.foreground });
    painter.text(pos2(rect.left() + 30.0, rect.bottom() - 9.0), Align2::LEFT_CENTER, hint, FontId::proportional(11.0), p.muted);
    if r.clicked() {
        ui.close();
    }
    r.clicked()
}

pub fn choice(ui: &mut Ui, on: bool, label: &str, detail: Option<&str>) -> bool {
    let p = theme::palette(ui);
    let h = if detail.is_some() { 36.0 } else { 26.0 };
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::MenuItem, true, on, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(4), p.accent);
    }
    let top = rect.top() + 13.0;
    if on {
        ui.painter().text(pos2(rect.left() + 8.0 + 7.0, top), Align2::CENTER_CENTER, lucide::CHECK, FontId::new(13.0, theme::icons()), p.primary);
    }
    ui.painter().text(pos2(rect.left() + 30.0, top), Align2::LEFT_CENTER, label, FontId::proportional(12.5), p.foreground);
    if let Some(d) = detail {
        ui.painter().text(pos2(rect.left() + 30.0, rect.bottom() - 8.0), Align2::LEFT_CENTER, d, FontId::proportional(11.0), p.muted);
    }
    if r.clicked() {
        ui.close();
    }
    r.clicked()
}

fn chip(ui: &mut Ui, glyph: Option<&str>, text: Option<(&str, Option<&str>)>, label: &str, on: bool, c: &Colors, fill: Option<Color32>) -> egui::Response {
    let font = FontId::proportional(11.5);
    let width = |s: &str| ui.fonts_mut(|f| f.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE).size().x);
    let main_w = text.map_or(0.0, |(m, _)| width(m).min(160.0));
    let extra_w = text.and_then(|(_, e)| e).map_or(0.0, |e| 6.0 + width(e));
    let mut w = 8.0;
    if glyph.is_some() {
        w += 14.0;
    }
    if text.is_some() {
        w += if glyph.is_some() { 6.0 } else { 0.0 } + main_w + extra_w;
    }
    w += 8.0;
    if text.is_none() {
        w = 28.0;
    }
    let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
    let bg = if on { Some(c.line) } else if r.hovered() { Some(c.line.gamma_multiply(0.6)) } else { fill };
    if let Some(bg) = bg {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), bg);
    }
    let color = if on { Color32::WHITE } else if r.hovered() { c.text } else { c.muted };
    let p = ui.painter().with_clip_rect(rect);
    let mut x = rect.left() + 8.0;
    if let Some(g) = glyph {
        let cx = if text.is_none() { rect.center().x } else { x + 7.0 };
        p.text(pos2(cx, rect.center().y), Align2::CENTER_CENTER, g, FontId::new(14.0, theme::icons()), color);
        x += 14.0 + 6.0;
    }
    if let Some((main, extra)) = text {
        let main_rect = Rect::from_min_max(pos2(x, rect.top()), pos2(x + main_w, rect.bottom()));
        let g = ui.painter().layout_no_wrap(main.to_owned(), font.clone(), if glyph.is_some() { color } else { c.text });
        p.with_clip_rect(main_rect).galley(pos2(x, rect.center().y - g.size().y / 2.0), g, c.text);
        if let Some(e) = extra {
            p.text(pos2(main_rect.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, e, font, color);
        }
    }
    r.on_hover_text(label)
}

fn square_button(ui: &mut Ui, glyph: &str, label: &str, fill: Color32, fg: Color32, enabled: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(28.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let fill = if !enabled { fill.gamma_multiply(0.4) } else if r.hovered() { fill.gamma_multiply(0.9) } else { fill };
    ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::new(if glyph == lucide::SQUARE { 12.0 } else { 16.0 }, theme::icons()), fg);
    r.on_hover_text(label)
}

fn usage(ui: &mut Ui, limits: Limits, c: &Colors) {
    let Some(five) = limits.five_hour else { return };
    let now = options::now_ms();
    let pct = |u: f64| format!("{}%", (u * 100.0).round());
    let mut title = format!("5-hour limit: {} used, resets {}", pct(five.utilization), options::reset_at(five.resets_at));
    if let Some(w) = limits.seven_day {
        title.push_str(&format!("\nWeekly limit: {} used, resets {}", pct(w.utilization), options::reset_at(w.resets_at)));
    }
    let high = five.utilization >= 0.75 || limits.seven_day.is_some_and(|w| w.utilization >= 0.9);
    let text = options::until_text(five.resets_at, now);
    let color = if high { theme::panel::AMBER } else { c.muted };
    let font = FontId::proportional(11.5);
    let w = 8.0 + 14.0 + 6.0 + ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), font.clone(), color).size().x) + 8.0;
    let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    ui.painter().text(pos2(rect.left() + 8.0 + 7.0, rect.center().y), Align2::CENTER_CENTER, lucide::CLOCK, FontId::new(14.0, theme::icons()), color);
    ui.painter().text(pos2(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, text, font, color);
    r.on_hover_text(title);
    ui.ctx().request_repaint_after(Duration::from_secs(60));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_opens_on_slash_at_the_start_and_at_after_a_space() {
        assert_eq!(menu_at("/com", 4), Some(Menu { kind: MenuKind::Slash, query: "com".into(), start: 0 }));
        assert_eq!(menu_at("/compact now", 12), None);
        assert_eq!(menu_at("look at @src/ma", 15), Some(Menu { kind: MenuKind::Mention, query: "src/ma".into(), start: 8 }));
        assert_eq!(menu_at("mail a@b", 8), None, "an address is not a mention");
        assert_eq!(menu_at("@", 1), Some(Menu { kind: MenuKind::Mention, query: String::new(), start: 0 }));
        assert_eq!(menu_at("é @x", 4), Some(Menu { kind: MenuKind::Mention, query: "x".into(), start: 2 }), "caret counts characters");
    }

    #[test]
    fn slash_options_put_prefix_matches_first() {
        let cmds: Vec<Command> = ["review", "security-review", "compact"].into_iter().map(|n| Command { name: n.into(), description: None }).collect();
        let names: Vec<String> = slash_options(&cmds, "rev").into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["review", "security-review"]);
    }

    #[test]
    fn choosing_replaces_the_typed_query() {
        let ctx = egui::Context::default();
        let mut c = Composer { text: "fix @ma please".into(), ..Composer::default() };
        c.update_menu(7, None);
        c.choose("src/main.rs", 7);
        assert_eq!(c.text, "fix @src/main.rs  please");
        assert_eq!(c.caret_to, Some(17));
        c.text = "/rev".into();
        c.update_menu(4, None);
        c.choose("review", 4);
        assert_eq!(c.text, "/review ");
        c.text = "hello".into();
        c.start_with('@', 5, Some("a.rs"));
        assert_eq!(c.text, "hello @");
        assert_eq!(c.found, ["a.rs"], "an empty mention offers the open file");
        assert!(matches!(c.submit(&ctx), Some(Action::Send(t, f)) if t == "hello @" && f.is_empty()));
        assert!(c.text.is_empty() && c.submit(&ctx).is_none(), "nothing to send");
    }

    #[test]
    fn mode_labels_follow_the_choice_then_claude_code() {
        assert_eq!(mode_label(Some(PermissionMode::Plan), Some("default")), "Plan");
        assert_eq!(mode_label(None, Some("default")), "Ask first");
        assert_eq!(mode_label(None, Some("acceptEdits")), "Accept edits");
        assert_eq!(mode_label(None, None), "Settings");
    }
}
