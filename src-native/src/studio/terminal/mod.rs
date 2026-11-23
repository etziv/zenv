pub mod colors;
pub mod keys;

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Processor, StdSyncHandler};
use eframe::egui::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat};
use eframe::egui::{self, pos2, vec2, Color32, Event, EventFilter, FontFamily, FontId, Id, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, Sense, Stroke, Ui};

use crate::theme;

pub const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 1.15;
const PAD: egui::Vec2 = vec2(8.0, 6.0);
pub const MIN_SIZE: (u16, u16) = (20, 5);

type Sink = Box<dyn FnMut(&[u8]) + Send>;

#[derive(Default)]
struct Wire {
    sink: Option<Sink>,
    pending: Vec<u8>,
    size: Option<WindowSize>,
}

#[derive(Default)]
pub struct Outbox(Mutex<Wire>);

impl Outbox {
    fn wire(&self) -> std::sync::MutexGuard<'_, Wire> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn send(&self, bytes: &[u8]) {
        let mut w = self.wire();
        match &mut w.sink {
            Some(sink) => sink(bytes),
            None => w.pending.extend_from_slice(bytes),
        }
    }

    pub fn connect(&self, mut sink: Sink) {
        let mut w = self.wire();
        if !w.pending.is_empty() {
            sink(&std::mem::take(&mut w.pending));
        }
        w.sink = Some(sink);
    }

    pub fn disconnect(&self) {
        let mut w = self.wire();
        w.sink = None;
        w.pending.clear();
    }

    pub fn set_size(&self, cols: u16, rows: u16, cell: egui::Vec2) {
        self.wire().size = Some(WindowSize { num_lines: rows, num_cols: cols, cell_width: cell.x as u16, cell_height: cell.y as u16 });
    }
}

#[derive(Clone)]
pub struct Listener(Arc<Outbox>);

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::PtyWrite(text) => self.0.send(text.as_bytes()),
            TermEvent::ColorRequest(i, reply) => self.0.send(reply(colors::default_rgb(i)).as_bytes()),
            TermEvent::TextAreaSizeRequest(reply) => {
                let size = self.0.wire().size;
                if let Some(size) = size {
                    self.0.send(reply(size).as_bytes());
                }
            }
            _ => {}
        }
    }
}

pub struct Screen {
    pub term: Term<Listener>,
    parser: Processor<StdSyncHandler>,
}

impl Screen {
    pub fn new(cols: u16, rows: u16, out: Arc<Outbox>) -> Self {
        let term = Term::new(Config { osc52: Osc52::Disabled, ..Config::default() }, &TermSize::new(cols as usize, rows as usize), Listener(out));
        Self { term, parser: Processor::new() }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(TermSize::new(cols as usize, rows as usize));
    }

    pub fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    pub fn reset(&mut self) {
        self.feed(b"\x1bc");
        self.term.grid_mut().clear_history();
    }

    pub fn text(&self) -> String {
        let grid = self.term.grid();
        let offset = grid.display_offset() as i32;
        let lines: Vec<String> = (0..self.term.screen_lines() as i32)
            .map(|l| {
                let row = &grid[Line(l - offset)];
                let s: String = (0..self.term.columns()).filter(|&c| !row[Column(c)].flags.contains(Flags::WIDE_CHAR_SPACER)).map(|c| row[Column(c)].c).collect();
                s.trim_end().to_owned()
            })
            .collect();
        lines.join("\n").trim_end().to_owned()
    }
}

#[derive(Default)]
pub struct Frame {
    pub input: Vec<u8>,
    pub resized: Option<(u16, u16)>,
}

pub struct Terminal {
    pub screen: Arc<Mutex<Screen>>,
    pub out: Arc<Outbox>,
    id: Id,
    pub focus: bool,
    focused: bool,
    meta: bool,
    wheel: f32,
    mouse_cell: Option<(usize, usize)>,
}

fn metrics(ui: &Ui) -> (FontId, egui::Vec2) {
    let font = FontId::new(FONT_SIZE, FontFamily::Monospace);
    let w = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
    (font, vec2(w, (FONT_SIZE * LINE_HEIGHT).round()))
}

pub fn grid_size(rect: Rect, cell: egui::Vec2) -> (u16, u16) {
    let cols = ((rect.width() - 2.0 * PAD.x) / cell.x).floor().max(0.0) as u16;
    let rows = ((rect.height() - PAD.y) / cell.y).floor().max(0.0) as u16;
    (cols.clamp(MIN_SIZE.0, 1000), rows.clamp(MIN_SIZE.1, 500))
}

impl Terminal {
    pub fn new(id: Id) -> Self {
        let out = Arc::new(Outbox::default());
        Self { screen: Arc::new(Mutex::new(Screen::new(80, 24, out.clone()))), out, id, focus: true, focused: false, meta: false, wheel: 0.0, mouse_cell: None }
    }

    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(self.id))
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Screen> {
        self.screen.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn fit(&self, ui: &Ui, rect: Rect) -> (u16, u16) {
        grid_size(rect, metrics(ui).1)
    }

    pub fn ui(&mut self, ui: &mut Ui, rect: Rect) -> Frame {
        let mut frame = Frame::default();
        let (font, cell) = metrics(ui);
        let size = grid_size(rect, cell);
        let r = ui.interact(rect, self.id, Sense::click_and_drag());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Terminal, true, "Terminal"));
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if std::mem::take(&mut self.focus) || r.clicked() || r.drag_started() {
            r.request_focus();
        }
        let focused = r.has_focus();
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(self.id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
        }
        let origin = rect.min + PAD;
        let shared = self.screen.clone();
        let mut screen = shared.lock().unwrap_or_else(|p| p.into_inner());
        if screen.size() != size {
            screen.resize(size.0, size.1);
            frame.resized = Some(size);
        }
        self.out.set_size(size.0, size.1, cell);
        let mode = *screen.term.mode();
        if focused != self.focused {
            self.focused = focused;
            screen.term.is_focused = focused;
            if mode.contains(TermMode::FOCUS_IN_OUT) {
                frame.input.extend_from_slice(if focused { b"\x1b[I" } else { b"\x1b[O" });
            }
        }
        self.mouse(ui, &r, &mut screen, origin, cell, mode, &mut frame);
        if focused {
            self.keys(ui, &mut screen, mode, &mut frame);
        }
        if !frame.input.is_empty() {
            screen.term.scroll_display(Scroll::Bottom);
        }
        paint(ui, rect, origin, cell, &font, &screen, focused);
        frame
    }

    fn keys(&mut self, ui: &mut Ui, screen: &mut Screen, mode: TermMode, frame: &mut Frame) {
        let events = ui.input(|i| i.events.clone());
        for e in events {
            match e {
                Event::Text(t) => {
                    if std::mem::take(&mut self.meta) {
                        frame.input.push(0x1b);
                    }
                    frame.input.extend_from_slice(t.as_bytes());
                }
                Event::Ime(egui::ImeEvent::Commit(t)) => frame.input.extend_from_slice(t.as_bytes()),
                Event::Copy => match screen.term.selection_to_string().filter(|s| !s.is_empty()) {
                    Some(text) => {
                        ui.ctx().copy_text(text);
                        screen.term.selection = None;
                    }
                    None => frame.input.push(0x03),
                },
                Event::Cut => frame.input.push(0x18),
                Event::Paste(text) => frame.input.extend(keys::paste(&text, mode.contains(TermMode::BRACKETED_PASTE))),
                Event::Key { key: key @ (Key::PageUp | Key::PageDown), pressed: true, modifiers, .. } if modifiers.shift_only() && !mode.contains(TermMode::ALT_SCREEN) => {
                    screen.term.scroll_display(if key == Key::PageUp { Scroll::PageUp } else { Scroll::PageDown });
                }
                Event::Key { key, pressed: true, modifiers, .. } => match keys::encode(key, modifiers, mode) {
                    Some(bytes) => {
                        self.meta = false;
                        frame.input.extend(bytes);
                    }
                    None => self.meta = modifiers.alt && !modifiers.ctrl,
                },
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mouse(&mut self, ui: &Ui, r: &egui::Response, screen: &mut Screen, origin: Pos2, cell: egui::Vec2, mode: TermMode, frame: &mut Frame) {
        let (cols, rows) = (screen.term.columns(), screen.term.screen_lines());
        let at = |p: Pos2| {
            let x = ((p.x - origin.x) / cell.x).max(0.0);
            let col = (x as usize).min(cols - 1);
            let line = (((p.y - origin.y) / cell.y).max(0.0) as usize).min(rows - 1);
            (col, line, if x.fract() < 0.5 { Side::Left } else { Side::Right })
        };
        let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
        let report = mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift;
        let hovered = r.hovered() || r.dragged();
        for e in &events {
            match *e {
                Event::MouseWheel { unit, delta, .. } if hovered => {
                    self.wheel += match unit {
                        MouseWheelUnit::Point => delta.y / cell.y,
                        MouseWheelUnit::Line => delta.y,
                        MouseWheelUnit::Page => delta.y * rows as f32,
                    };
                }
                Event::PointerButton { pos, button, pressed, modifiers } if report && if pressed { r.contains_pointer() } else { self.mouse_cell.is_some() } => {
                    let code = match button {
                        PointerButton::Primary => keys::button::LEFT,
                        PointerButton::Middle => keys::button::MIDDLE,
                        PointerButton::Secondary => keys::button::RIGHT,
                        _ => continue,
                    };
                    let (col, line, _) = at(pos);
                    self.mouse_cell = pressed.then_some((col, line));
                    frame.input.extend(keys::mouse(code, col, line, pressed, modifiers, mode));
                }
                Event::PointerMoved(pos) if report && r.contains_pointer() => {
                    let (col, line, _) = at(pos);
                    let down = ui.input(|i| i.pointer.primary_down());
                    let wanted = mode.contains(TermMode::MOUSE_MOTION) || (down && mode.contains(TermMode::MOUSE_DRAG));
                    if wanted && self.mouse_cell != Some((col, line)) && (down || mode.contains(TermMode::MOUSE_MOTION)) {
                        let code = if down { keys::button::LEFT } else { keys::button::RELEASE } + keys::button::MOTION;
                        frame.input.extend(keys::mouse(code, col, line, true, modifiers, mode));
                        if down {
                            self.mouse_cell = Some((col, line));
                        }
                    }
                }
                _ => {}
            }
        }
        let lines = self.wheel.trunc() as i32;
        if lines != 0 {
            self.wheel -= lines as f32;
            if report {
                let (col, line, _) = r.hover_pos().map(at).unwrap_or((0, 0, Side::Left));
                let code = if lines > 0 { keys::button::WHEEL_UP } else { keys::button::WHEEL_DOWN };
                for _ in 0..lines.unsigned_abs() {
                    frame.input.extend(keys::mouse(code, col, line, true, modifiers, mode));
                }
            } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
                let arrow = keys::encode(if lines > 0 { Key::ArrowUp } else { Key::ArrowDown }, Modifiers::NONE, mode).unwrap_or_default();
                for _ in 0..lines.unsigned_abs() {
                    frame.input.extend_from_slice(&arrow);
                }
            } else {
                screen.term.scroll_display(Scroll::Delta(lines));
            }
        }
        if report {
            return;
        }
        let offset = screen.term.grid().display_offset() as i32;
        let point = |p: Pos2| {
            let (col, line, side) = at(p);
            (Point::new(Line(line as i32 - offset), Column(col)), side)
        };
        let Some(pos) = r.interact_pointer_pos() else { return };
        let (p, side) = point(pos);
        let pick = |ty| Some(Selection::new(ty, p, side));
        if r.triple_clicked() {
            screen.term.selection = pick(SelectionType::Lines);
        } else if r.double_clicked() {
            screen.term.selection = pick(SelectionType::Semantic);
        } else if r.drag_started() {
            screen.term.selection = pick(SelectionType::Simple);
        } else if r.dragged() {
            if let Some(s) = &mut screen.term.selection {
                s.update(p, side);
            }
        } else if r.clicked() {
            screen.term.selection = None;
        }
    }
}

struct Run {
    job: LayoutJob,
    col: usize,
    len: usize,
}

impl Run {
    fn push(&mut self, c: char, font: &FontId, color: Color32) {
        let start = ByteIndex(self.job.text.len());
        self.job.text.push(c);
        self.len += 1;
        let end = ByteIndex(self.job.text.len());
        match self.job.sections.last_mut() {
            Some(last) if last.format.color == color && last.format.font_id == *font => last.byte_range.end = end,
            _ => self.job.sections.push(LayoutSection { leading_space: 0.0, byte_range: start..end, format: TextFormat::simple(font.clone(), color) }),
        }
    }
}

fn paint(ui: &Ui, rect: Rect, origin: Pos2, cell: egui::Vec2, font: &FontId, screen: &Screen, focused: bool) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, colors::BACKGROUND);
    let bold_font = FontId::new(font.size, theme::mono_bold());
    let content = screen.term.renderable_content();
    let offset = content.display_offset as i32;
    let set = content.colors;
    let cursor = content.cursor;
    let cursor_line = cursor.point.line.0 + offset;
    let mut run: Option<(i32, Run)> = None;
    let mut cursor_glyph = None;
    let pos = |line: i32, col: usize| pos2(origin.x + col as f32 * cell.x, origin.y + line as f32 * cell.y);
    let flush = |run: &mut Option<(i32, Run)>| {
        if let Some((line, r)) = run.take() {
            let galley = ui.fonts_mut(|f| f.layout_job(r.job));
            let at = pos(line, r.col) + vec2(0.0, (cell.y - galley.size().y) / 2.0);
            painter.galley(at, galley, colors::FOREGROUND);
        }
    };
    for indexed in content.display_iter {
        let c = &*indexed;
        let line = indexed.point.line.0 + offset;
        let col = indexed.point.column.0;
        if c.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let bold = c.flags.contains(Flags::BOLD);
        let mut fg = colors::resolve(c.fg, set, bold, c.flags.contains(Flags::DIM));
        let mut bg = colors::resolve(c.bg, set, false, false);
        if c.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let width = if c.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 };
        let cell_rect = Rect::from_min_size(pos(line, col), vec2(cell.x * width, cell.y));
        if bg != colors::BACKGROUND {
            painter.rect_filled(cell_rect, 0.0, bg);
        }
        if content.selection.is_some_and(|s| s.contains_cell(&indexed, cursor.point, cursor.shape)) {
            painter.rect_filled(cell_rect, 0.0, colors::SELECTION);
        }
        if c.flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        let y = cell_rect.bottom() - 1.5;
        if c.flags.intersects(Flags::ALL_UNDERLINES) {
            painter.hline(cell_rect.x_range(), y, Stroke::new(1.0, fg));
        }
        if c.flags.contains(Flags::STRIKEOUT) {
            painter.hline(cell_rect.x_range(), cell_rect.center().y, Stroke::new(1.0, fg));
        }
        let on_cursor = line == cursor_line && col == cursor.point.column.0 && cursor.shape != CursorShape::Hidden;
        if on_cursor {
            cursor_glyph = Some((c.c, bold, bg, cell_rect));
        }
        let joins = run.as_ref().is_some_and(|(l, r)| *l == line && r.col + r.len == col);
        if c.c.is_ascii_graphic() || (c.c == ' ' && joins) {
            if !joins {
                flush(&mut run);
                run = Some((line, Run { job: LayoutJob::default(), col, len: 0 }));
            }
            if let Some((_, r)) = &mut run {
                r.push(c.c, if bold { &bold_font } else { font }, fg);
            }
            continue;
        }
        flush(&mut run);
        if c.c != ' ' && c.c != '\0' {
            painter.text(cell_rect.center(), egui::Align2::CENTER_CENTER, c.c, if bold { bold_font.clone() } else { font.clone() }, fg);
        }
    }
    flush(&mut run);
    let Some((glyph, bold, bg, r)) = cursor_glyph else { return };
    match cursor.shape {
        CursorShape::Block if focused => {
            painter.rect_filled(r, 0.0, colors::CURSOR);
            if glyph != ' ' {
                painter.text(r.center(), egui::Align2::CENTER_CENTER, glyph, if bold { bold_font } else { font.clone() }, if bg == colors::CURSOR { colors::BACKGROUND } else { bg });
            }
        }
        CursorShape::Beam => {
            painter.rect_filled(Rect::from_min_size(r.min, vec2(2.0, r.height())), 0.0, colors::CURSOR);
        }
        CursorShape::Underline => {
            painter.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0), r.max), 0.0, colors::CURSOR);
        }
        _ => {
            painter.rect_stroke(r.shrink(0.5), 0.0, Stroke::new(1.0, colors::CURSOR), egui::StrokeKind::Inside);
        }
    }
}
