pub mod editor;
pub mod highlight;
#[rustfmt::skip]
mod icon_map;
pub mod icons;
pub mod quick_open;
pub mod tree;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, vec2, Align, Color32, CornerRadius, FontFamily, FontId, Id, Key, KeyboardShortcut, Layout, Modifiers, OpenUrl, Rect, RichText, Sense, Stroke, Ui, ViewportBuilder, ViewportCommand, ViewportId};
use zenv_core::app_core::Core;
use zenv_core::editors::{EditorId, Editors};

use crate::theme::{self, palette, panel};
use crate::widgets::{self, ModalChoice};
use crate::{lucide, shell};
use editor::{Editor, Env};
use quick_open::QuickOpen;
use tree::Tree;

const TREE_KEY: &str = "zenv.studio.tree";
const TREE_DEFAULT: f32 = 260.0;
const TREE_RANGE: (f32, f32) = (160.0, 520.0);

pub struct Colors {
    pub panel: Color32,
    pub raised: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub led: Color32,
    pub fail: Color32,
}

impl Colors {
    pub fn of(ui: &Ui) -> Self {
        let p = palette(ui);
        Self { panel: p.panel, raised: p.panel_raised, line: p.panel_line, text: panel::TEXT, muted: panel::MUTED, led: panel::LED, fail: panel::LED_FAIL }
    }
}

pub fn hline(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
}

pub fn panel_icon_button(ui: &mut Ui, glyph: &str, label: &str, on: bool, size: f32, glyph_size: f32) -> egui::Response {
    let c = Colors::of(ui);
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), c.raised);
    }
    let color = match (on, r.hovered()) {
        (true, _) => c.led,
        (false, true) => c.text,
        (false, false) => c.muted,
    };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::new(glyph_size, theme::icons()), color);
    r.on_hover_text(label)
}

#[derive(Clone, Debug)]
pub struct Site {
    pub id: String,
    pub domain: String,
    pub root: PathBuf,
    pub url: String,
    pub enabled: bool,
}

impl From<&zenv_core::local_sites::SiteView> for Site {
    fn from(v: &zenv_core::local_sites::SiteView) -> Self {
        Self { id: v.site.id.clone(), domain: v.site.domain.clone(), root: v.site.project_root.clone(), url: v.url.clone(), enabled: v.site.enabled }
    }
}

pub struct Studio {
    core: Arc<Core>,
    pub site: Site,
    pub tree: Tree,
    pub editor: Editor,
    pub show_tree: bool,
    pub quick: Option<QuickOpen>,
    editors: Option<Editors>,
    editors_rx: Option<Receiver<Editors>>,
    notices: (std::sync::mpsc::Sender<String>, Receiver<String>),
    pub notice: Option<String>,
    dragging: bool,
    ask: Option<Ask>,
    quit_files: Vec<String>,
    pub closed: bool,
    pub quit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Close,
    Quit,
}

impl Studio {
    pub fn new(ctx: &egui::Context, core: Arc<Core>, site: Site) -> Self {
        let (tx, rx) = channel();
        let (c, ctx2) = (core.clone(), ctx.clone());
        std::thread::spawn(move || {
            if let Ok(e) = c.editors() {
                let _ = tx.send(e);
                ctx2.request_repaint();
            }
        });
        Self {
            tree: Tree::new(&site.root),
            editor: Editor::new(ctx),
            core,
            site,
            show_tree: true,
            quick: None,
            editors: None,
            editors_rx: Some(rx),
            notices: channel(),
            notice: None,
            dragging: false,
            ask: None,
            quit_files: Vec::new(),
            closed: false,
            quit: false,
        }
    }

    fn unsaved(&self) -> Vec<String> {
        self.editor.docs.iter().filter(|d| d.dirty()).map(|d| editor::name_of(&d.path).to_owned()).collect()
    }

    pub fn title(&self) -> String {
        format!("{} · Studio · Zenv", self.site.domain)
    }

    pub fn open_file(&mut self, path: &str) {
        self.editor.open(&self.site.root, path);
    }

    fn run(&self, ctx: &egui::Context, what: &'static str, job: impl FnOnce(&Core) -> zenv_core::error::AppResult<()> + Send + 'static) {
        let (core, tx, ctx) = (self.core.clone(), self.notices.0.clone(), ctx.clone());
        std::thread::spawn(move || {
            if let Err(e) = job(&core) {
                let _ = tx.send(format!("Couldn't {what}: {e}"));
                ctx.request_repaint();
            }
        });
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some(Ok(e)) = self.editors_rx.as_ref().map(Receiver::try_recv) {
            self.editors = Some(e);
            self.editors_rx = None;
        }
        while let Ok(n) = self.notices.1.try_recv() {
            self.notice = Some(n);
        }
        self.guard_close(&ctx);
        let (quick, files) = ui.input_mut(|i| {
            (i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::P)), i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)))
        });
        if quick {
            self.quick = Some(QuickOpen::default());
        }
        if files {
            self.show_tree = !self.show_tree;
        }
        let c = Colors::of(ui);
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, c.panel);
        ui.visuals_mut().override_text_color = Some(c.text);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        self.header(ui, &c);
        let (trace, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        shell::trace(ui, trace);
        if let Some(n) = self.notice.clone() {
            let r = egui::Frame::new().fill(Color32::from_rgb(0x2a, 0x12, 0x10)).inner_margin(egui::Margin::symmetric(16, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(n).size(12.0).color(c.fail)).wrap().sense(Sense::click()))
            });
            if r.inner.on_hover_text("Dismiss").clicked() {
                self.notice = None;
            }
        }
        self.body(ui, &c);
        if let Some(q) = &mut self.quick {
            match q.ui(ui, &self.site.root) {
                Some(quick_open::Out::Open(path)) => {
                    self.quick = None;
                    self.open_file(&path);
                }
                Some(quick_open::Out::Close) => self.quick = None,
                None => {}
            }
        }
    }

    fn guard_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.editor.docs.iter().any(|d| d.dirty()) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.ask = Some(Ask::Close);
            } else {
                self.closed = true;
            }
        }
        let Some(ask) = self.ask else { return };
        let dirty = if ask == Ask::Quit { self.quit_files.clone() } else { self.unsaved() };
        let (title, confirm) = match ask {
            Ask::Close => ("Close Studio with unsaved changes?", "Close anyway"),
            Ask::Quit => ("Quit Zenv with unsaved changes?", "Quit anyway"),
        };
        match widgets::confirm_modal(ctx, "studio-close-window", title, confirm, true, false, |ui| {
            ui.label(format!("Not saved: {}.", dirty.join(", ")));
        }) {
            ModalChoice::Confirm => {
                self.ask = None;
                self.closed |= ask == Ask::Close;
                self.quit |= ask == Ask::Quit;
                ctx.request_repaint_of(ViewportId::ROOT);
            }
            ModalChoice::Cancel => self.ask = None,
            ModalChoice::None => {}
        }
    }

    fn header(&mut self, ui: &mut Ui, c: &Colors) {
        let ctx = ui.ctx().clone();
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        let mut h = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(0.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        h.spacing_mut().item_spacing.x = 12.0;
        h.add_space(16.0);
        h.label(widgets::display("zenv", 17.0).color(Color32::WHITE));
        h.label(RichText::new(widgets::tracked("STUDIO")).font(FontId::new(10.0, FontFamily::Monospace)).color(c.led));
        let (sep, _) = h.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
        h.painter().rect_filled(sep, 0.0, c.line);
        let right_w = 3.0 * 32.0 + 4.0 * 4.0 + 18.0 + self.editor_button_width(&h) + 32.0 + 8.0;
        let search_w = (rect.width() * 0.3).min(360.0);
        let info_w = (rect.width() - right_w - search_w - 160.0).max(80.0);
        h.vertical(|ui| {
            ui.set_width(info_w);
            ui.add(egui::Label::new(RichText::new(&self.site.domain).size(13.0).family(theme::medium()).color(Color32::WHITE)).truncate());
            let root = self.site.root.display().to_string();
            ui.add(egui::Label::new(RichText::new(&root).font(FontId::new(10.0, FontFamily::Monospace)).color(c.muted)).truncate()).on_hover_text(root);
        });
        let gap = ((rect.right() - right_w) - h.cursor().left() - search_w) / 2.0;
        h.add_space(gap.max(8.0));
        if go_to_file(&mut h, search_w, c).clicked() {
            self.quick = Some(QuickOpen::default());
        }
        h.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(8.0);
            let browser = ui.add_enabled_ui(self.site.enabled, |ui| panel_icon_button(ui, lucide::EXTERNAL_LINK, "Open in browser", false, 32.0, 16.0)).inner;
            if browser.clicked() {
                ctx.open_url(OpenUrl::new_tab(self.site.url.clone()));
            }
            if panel_icon_button(ui, lucide::FOLDER_OPEN, "Open project folder", false, 32.0, 16.0).clicked() {
                let root = self.site.root.clone();
                self.run(&ctx, "open the project folder", move |_| crate::os::reveal(&root, false));
            }
            if let Some(editor) = self.preferred_editor() {
                if self.editor_button(ui, &editor, c).clicked() {
                    let (id, ed) = (self.site.id.clone(), editor.0);
                    self.run(&ctx, "open the editor", move |core| core.open_in_editor(&id, Some(ed)).map(|_| ()));
                }
            }
            let (sep, _) = ui.allocate_exact_size(vec2(9.0, 16.0), Sense::hover());
            ui.painter().rect_filled(Rect::from_center_size(sep.center(), vec2(1.0, 16.0)), 0.0, c.line);
            if panel_icon_button(ui, lucide::PANEL_LEFT, "Files (Ctrl+B)", self.show_tree, 32.0, 16.0).clicked() {
                self.show_tree = !self.show_tree;
            }
        });
    }

    fn preferred_editor(&self) -> Option<(EditorId, String, Option<String>)> {
        let e = self.editors.as_ref()?;
        let pick = e.editors.iter().find(|x| Some(x.id) == e.preferred).or(e.editors.first())?;
        Some((pick.id, pick.name.to_owned(), pick.icon.clone()))
    }

    fn editor_button_width(&self, ui: &Ui) -> f32 {
        self.preferred_editor().map_or(0.0, |(_, name, _)| ui.fonts_mut(|f| f.layout_no_wrap(name, FontId::proportional(12.0), Color32::WHITE).size().x) + 10.0 + 16.0 + 8.0 + 10.0)
    }

    fn editor_button(&self, ui: &mut Ui, (_, name, icon): &(EditorId, String, Option<String>), c: &Colors) -> egui::Response {
        let w = self.editor_button_width(ui);
        let (rect, r) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
        if r.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), c.raised);
        }
        let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 10.0 + 8.0, rect.center().y), vec2(16.0, 16.0));
        match icon.as_deref().and_then(widgets::data_url_bytes) {
            Some(bytes) => egui::Image::from_bytes(format!("bytes://editor-{name}"), bytes).paint_at(ui, icon_rect),
            None => {
                ui.painter().text(icon_rect.center(), egui::Align2::CENTER_CENTER, lucide::CODE, FontId::new(16.0, theme::icons()), c.muted);
            }
        }
        let color = if r.hovered() { c.text } else { c.muted };
        ui.painter().text(egui::pos2(icon_rect.right() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, name, FontId::proportional(12.0), color);
        r.on_hover_text(format!("Open {} in {name}", self.site.root.display()))
    }

    fn body(&mut self, ui: &mut Ui, c: &Colors) {
        let body = ui.available_rect_before_wrap();
        let tree_w = ui.ctx().data_mut(|d| d.get_persisted::<f32>(Id::unique(TREE_KEY))).unwrap_or(TREE_DEFAULT).clamp(TREE_RANGE.0, TREE_RANGE.1);
        let mut x = body.left();
        if self.show_tree {
            let rect = Rect::from_min_size(body.min, vec2(tree_w, body.height()));
            let mut tree_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
            tree_ui.set_clip_rect(rect);
            let root_name = self.site.root.file_name().map_or_else(|| self.site.root.display().to_string(), |n| n.to_string_lossy().into_owned());
            if let Some(path) = self.tree.ui(&mut tree_ui, &self.site.root, &root_name, self.editor.active.as_deref()) {
                self.open_file(&path);
            }
            x = rect.right();
            if let Some(w) = self.splitter(ui, x, body, c) {
                ui.ctx().data_mut(|d| d.insert_persisted(Id::unique(TREE_KEY), w));
            }
            x += 1.0;
        }
        let rect = Rect::from_min_max(egui::pos2(x, body.top()), body.max);
        let mut editor_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        editor_ui.set_clip_rect(rect);
        let env = Env { root: &self.site.root, core: &self.core, site_id: &self.site.id };
        self.editor.ui(&mut editor_ui, &env);
        ui.advance_cursor_after_rect(body);
    }

    fn splitter(&mut self, ui: &mut Ui, x: f32, body: Rect, c: &Colors) -> Option<f32> {
        let line = Rect::from_min_size(egui::pos2(x, body.top()), vec2(1.0, body.height()));
        let grip = line.expand2(vec2(5.0, 0.0));
        let r = ui.interact(grip, Id::unique(("studio-tree-splitter", &self.site.id)), Sense::click_and_drag());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Splitter, true, "Drag to resize · double-click to reset"));
        self.dragging = r.dragged();
        let lit = r.hovered() || self.dragging;
        ui.painter().rect_filled(line, 0.0, if self.dragging { c.led } else { c.line });
        if lit {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            ui.painter().rect_filled(Rect::from_center_size(line.center(), vec2(3.0, body.height())), 0.0, c.led.gamma_multiply(0.7));
        }
        if r.double_clicked() {
            return Some(TREE_DEFAULT);
        }
        let pointer = r.interact_pointer_pos().filter(|_| self.dragging)?;
        Some((pointer.x - body.left()).clamp(TREE_RANGE.0, TREE_RANGE.1))
    }
}

fn go_to_file(ui: &mut Ui, width: f32, c: &Colors) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Go to file"));
    let border = if r.hovered() { c.muted } else { c.line };
    ui.painter().rect(rect, CornerRadius::same(theme::RADIUS), c.raised, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let y = rect.center().y;
    ui.painter().text(egui::pos2(rect.left() + 10.0 + 7.0, y), egui::Align2::CENTER_CENTER, lucide::SEARCH, FontId::new(14.0, theme::icons()), c.muted);
    ui.painter().text(egui::pos2(rect.left() + 10.0 + 14.0 + 8.0, y), egui::Align2::LEFT_CENTER, "Go to file", FontId::proportional(12.0), c.muted);
    ui.painter().text(egui::pos2(rect.right() - 10.0, y), egui::Align2::RIGHT_CENTER, "Ctrl P", FontId::new(10.0, FontFamily::Monospace), c.muted);
    r
}

#[derive(Default)]
pub struct Windows {
    open: HashMap<String, Arc<Mutex<Studio>>>,
    quit_confirmed: bool,
    icon: Option<Arc<egui::IconData>>,
}

fn viewport_id(site_id: &str) -> ViewportId {
    ViewportId::from_hash_of(("studio", site_id))
}

impl Windows {
    pub fn open(&mut self, ctx: &egui::Context, core: &Arc<Core>, site: Site) -> Arc<Mutex<Studio>> {
        if let Some(s) = self.open.get(&site.id) {
            let id = viewport_id(&site.id);
            ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
            ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
            return s.clone();
        }
        let id = site.id.clone();
        let studio = Arc::new(Mutex::new(Studio::new(ctx, core.clone(), site)));
        self.open.insert(id, studio.clone());
        studio
    }

    pub fn ask_quit(&mut self, ctx: &egui::Context) -> bool {
        if self.quit_confirmed {
            return false;
        }
        let mut files = Vec::new();
        let mut first = None;
        for (site_id, studio) in &self.open {
            let s = studio.lock().unwrap_or_else(|p| p.into_inner());
            let unsaved = s.unsaved();
            if !unsaved.is_empty() {
                first.get_or_insert_with(|| (site_id.clone(), studio.clone()));
                files.extend(unsaved.into_iter().map(|f| format!("{f} ({})", s.site.domain)));
            }
        }
        let Some((site_id, studio)) = first else { return false };
        let mut s = studio.lock().unwrap_or_else(|p| p.into_inner());
        s.ask = Some(Ask::Quit);
        s.quit_files = files;
        let id = viewport_id(&site_id);
        ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
        ctx.request_repaint_of(id);
        true
    }

    pub fn take_quit(&mut self) -> bool {
        for studio in self.open.values() {
            if std::mem::take(&mut studio.lock().unwrap_or_else(|p| p.into_inner()).quit) {
                self.quit_confirmed = true;
            }
        }
        self.quit_confirmed
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.open.retain(|_, s| !s.lock().unwrap_or_else(|p| p.into_inner()).closed);
        if self.icon.is_none() && !self.open.is_empty() {
            self.icon = eframe::icon_data::from_png_bytes(crate::ICON_PNG).ok().map(Arc::new);
        }
        for (site_id, studio) in &self.open {
            let title = studio.lock().unwrap_or_else(|p| p.into_inner()).title();
            let mut builder = ViewportBuilder::default().with_title(title).with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 560.0]);
            if let Some(icon) = &self.icon {
                builder = builder.with_icon(icon.clone());
            }
            let studio = studio.clone();
            ctx.show_viewport_deferred(viewport_id(site_id), builder, move |ui, _| {
                let mut s = studio.lock().unwrap_or_else(|p| p.into_inner());
                let fill = palette(ui).panel;
                egui::CentralPanel::default().frame(egui::Frame::new().fill(fill)).show(ui, |ui| s.ui(ui));
                if s.closed {
                    ui.ctx().request_repaint_of(ViewportId::ROOT);
                }
            });
        }
    }
}
