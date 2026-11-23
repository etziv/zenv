use std::sync::OnceLock;

use eframe::egui::{self, pos2, text::LayoutJob, vec2, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Stroke, TextFormat, Ui};
use fancy_regex::Regex;

use crate::studio::Colors;
use crate::theme;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub code: bool,
    pub bold: bool,
    pub italic: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub indent: usize,
    pub spans: Vec<Span>,
    pub continuation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Code { lang: String, body: String },
    Heading { big: bool, spans: Vec<Span> },
    Rule,
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    Quote(Vec<Block>),
    List { ordered: bool, items: Vec<ListItem> },
    Para(Vec<Span>),
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("markdown pattern"))
}

fn is(cell: &'static OnceLock<Regex>, pattern: &str, text: &str) -> bool {
    re(cell, pattern).is_match(text).unwrap_or(false)
}

static FENCE: OnceLock<Regex> = OnceLock::new();
static HEADING: OnceLock<Regex> = OnceLock::new();
static RULE: OnceLock<Regex> = OnceLock::new();
static TABLE: OnceLock<Regex> = OnceLock::new();
static SEPARATOR: OnceLock<Regex> = OnceLock::new();
static QUOTE: OnceLock<Regex> = OnceLock::new();
static ITEM: OnceLock<Regex> = OnceLock::new();
static INDENTED: OnceLock<Regex> = OnceLock::new();
static BREAKS_PARA: OnceLock<Regex> = OnceLock::new();
static INLINE: OnceLock<Regex> = OnceLock::new();

const MAX_QUOTE_DEPTH: usize = 8;
const MAX_INLINE_DEPTH: usize = 8;

const ITEM_RE: &str = r"^(\s*)([-*+]|\d+[.)])\s+(.*)$";

pub fn parse(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    blocks(&lines, 0)
}

fn blocks(lines: &[&str], depth: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let at = |n: usize| lines.get(n).copied().unwrap_or("");
    let mut i = 0;
    while i < lines.len() {
        let line = at(i);
        if let Ok(Some(f)) = re(&FENCE, r"^\s*(```|~~~)\s*([\w+-]*)").captures(line) {
            let close = f.get(1).map_or("```", |m| m.as_str());
            let lang = f.get(2).map_or("", |m| m.as_str()).to_owned();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !at(i).trim_start().starts_with(close) {
                body.push(at(i));
                i += 1;
            }
            i += 1;
            out.push(Block::Code { lang, body: body.join("\n") });
            continue;
        }
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Ok(Some(h)) = re(&HEADING, r"^(#{1,6})\s+(.*)$").captures(line) {
            let big = h.get(1).map_or(0, |m| m.as_str().len()) <= 2;
            out.push(Block::Heading { big, spans: inline(h.get(2).map_or("", |m| m.as_str())) });
            i += 1;
            continue;
        }
        if is(&RULE, r"^\s*([-*_])(\s*\1){2,}\s*$", line) {
            out.push(Block::Rule);
            i += 1;
            continue;
        }
        let table_row = |l: &str| is(&TABLE, r"^\s*\|.*\|\s*$", l);
        if table_row(line) {
            let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
            while i < lines.len() && table_row(at(i)) {
                let t = at(i).trim();
                let cells: Vec<&str> = t[1..t.len() - 1].split('|').map(str::trim).collect();
                if !cells.iter().all(|c| is(&SEPARATOR, r"^:?-+:?$", c)) {
                    rows.push(cells.into_iter().map(inline).collect());
                }
                i += 1;
            }
            let mut rows = rows.into_iter();
            let head = rows.next().unwrap_or_default();
            out.push(Block::Table { head, rows: rows.collect() });
            continue;
        }
        let quoted = |l: &str| is(&QUOTE, r"^\s*>", l);
        if quoted(line) && depth < MAX_QUOTE_DEPTH {
            let mut body = Vec::new();
            while i < lines.len() && quoted(at(i)) {
                let l = at(i).trim_start();
                let l = l.strip_prefix('>').unwrap_or(l);
                body.push(l.strip_prefix(' ').unwrap_or(l));
                i += 1;
            }
            out.push(Block::Quote(blocks(&body, depth + 1)));
            continue;
        }
        if let Ok(Some(first)) = re(&ITEM, ITEM_RE).captures(line) {
            let numbered = |m: &fancy_regex::Captures| m.get(2).is_some_and(|g| g.as_str().starts_with(|c: char| c.is_ascii_digit()));
            let ordered = numbered(&first);
            let mut items = Vec::new();
            while i < lines.len() {
                let l = at(i);
                match re(&ITEM, ITEM_RE).captures(l) {
                    Ok(Some(m)) if numbered(&m) == ordered => {
                        let indent = m.get(1).map_or(0, |g| g.as_str().len()).min(8);
                        items.push(ListItem { indent, spans: inline(m.get(3).map_or("", |g| g.as_str())), continuation: false });
                        i += 1;
                    }
                    _ if !l.trim().is_empty() && is(&INDENTED, r"^\s{2,}", l) && !items.is_empty() => {
                        items.push(ListItem { indent: 0, spans: inline(l.trim()), continuation: true });
                        i += 1;
                    }
                    _ => break,
                }
            }
            out.push(Block::List { ordered, items });
            continue;
        }
        let mut para = Vec::new();
        while i < lines.len() && !at(i).trim().is_empty() && !is(&BREAKS_PARA, r"^(\s*(```|~~~|>|\||[-*+]\s|\d+[.)]\s)|#{1,6}\s)", at(i)) {
            para.push(at(i));
            i += 1;
        }
        if para.is_empty() {
            para.push(at(i));
            i += 1;
        }
        out.push(Block::Para(inline(&para.join("\n"))));
    }
    out
}

pub fn inline(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    spans(text, Span::default(), 0, &mut out);
    out
}

fn spans(text: &str, style: Span, depth: usize, out: &mut Vec<Span>) {
    let pattern = re(&INLINE, r"(`+)([^`]+?)\1|\*\*(.+?)\*\*|__(.+?)__|(?<![\w*])\*(?!\s)(.+?)\*(?!\w)|(?<!\w)_(?!\s)(.+?)_(?!\w)|\[([^\[\]\n]{1,500})\]\(([^)\s\[]{1,2048})\)");
    let plain = |t: &str, out: &mut Vec<Span>| {
        if !t.is_empty() {
            out.push(Span { text: t.to_owned(), ..style.clone() });
        }
    };
    if depth > MAX_INLINE_DEPTH {
        plain(text, out);
        return;
    }
    let mut last = 0;
    for m in pattern.captures_iter(text).flatten() {
        let whole = m.get(0).expect("match");
        plain(&text[last..whole.start()], out);
        let g = |n: usize| m.get(n).map(|x| x.as_str());
        if let Some(code) = g(2) {
            out.push(Span { text: code.to_owned(), code: true, ..style.clone() });
        } else if let Some(t) = g(3).or(g(4)) {
            spans(t, Span { bold: true, ..style.clone() }, depth + 1, out);
        } else if let Some(t) = g(5).or(g(6)) {
            spans(t, Span { italic: true, ..style.clone() }, depth + 1, out);
        } else if let (Some(label), Some(url)) = (g(7), g(8)) {
            out.push(Span { text: label.to_owned(), link: Some(url.to_owned()), ..style.clone() });
        }
        last = whole.end();
    }
    plain(&text[last..], out);
}

pub const SIZE: f32 = 13.0;

pub fn job(spans: &[Span], size: f32, base: Color32, c: &Colors, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    for s in spans {
        let mut f = TextFormat { font_id: FontId::proportional(size), color: base, line_height: Some(size * 1.55), ..Default::default() };
        if s.bold {
            f.font_id.family = theme::medium();
            f.color = Color32::WHITE;
        }
        f.italics = s.italic;
        if s.code {
            f.font_id = FontId::new(size * 0.92, FontFamily::Monospace);
            f.color = c.led;
            f.background = c.raised;
        }
        if s.link.is_some() {
            f.color = c.led;
            f.underline = Stroke::new(1.0, c.led.gamma_multiply(0.4));
        }
        job.append(&s.text, 0.0, f);
    }
    job
}

fn text(ui: &mut Ui, spans: &[Span], size: f32, base: Color32, c: &Colors) {
    let links: Vec<&str> = spans.iter().filter_map(|s| s.link.as_deref()).collect();
    let r = ui.add(egui::Label::new(job(spans, size, base, c, ui.available_width())).wrap());
    if !links.is_empty() {
        r.on_hover_text(links.join("\n"));
    }
}

pub fn code_block(ui: &mut Ui, lang: &str, body: &str, c: &Colors) {
    egui::Frame::new().fill(Color32::from_black_alpha(102)).stroke(Stroke::new(1.0, c.line)).corner_radius(CornerRadius::same(theme::RADIUS)).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !lang.is_empty() {
            ui.label(RichText::new(lang.to_uppercase()).size(10.0).color(c.muted));
            ui.add_space(4.0);
        }
        egui::ScrollArea::horizontal().id_salt(("md-code", ui.next_auto_id())).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(body).font(FontId::new(11.5, FontFamily::Monospace)).color(c.text)).extend());
        });
    });
}

pub fn show(ui: &mut Ui, blocks: &[Block], c: &Colors) {
    ui.spacing_mut().item_spacing.y = 8.0;
    for b in blocks {
        match b {
            Block::Code { lang, body } => code_block(ui, lang, body, c),
            Block::Heading { big, spans } => {
                ui.add_space(4.0);
                let bold: Vec<Span> = spans.iter().map(|s| Span { bold: true, ..s.clone() }).collect();
                text(ui, &bold, if *big { 14.0 } else { SIZE }, Color32::WHITE, c);
            }
            Block::Rule => crate::studio::hline(ui, c.line),
            Block::Para(spans) => text(ui, spans, SIZE, c.text, c),
            Block::Quote(inner) => {
                let top = ui.cursor().top();
                let r = ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        let muted = Colors { text: c.muted, ..Colors::of(ui) };
                        show(ui, inner, &muted);
                    });
                });
                let bar = Rect::from_min_max(pos2(r.response.rect.left(), top), pos2(r.response.rect.left() + 2.0, r.response.rect.bottom()));
                ui.painter().rect_filled(bar, 0.0, c.line);
            }
            Block::List { ordered, items } => {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let mut n = 0;
                    for it in items {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(it.indent as f32 * 6.0);
                            let marker = if it.continuation {
                                String::new()
                            } else if *ordered {
                                n += 1;
                                format!("{n}.")
                            } else {
                                "•".to_owned()
                            };
                            let (rect, _) = ui.allocate_exact_size(vec2(20.0, SIZE * 1.55), egui::Sense::hover());
                            ui.painter().text(pos2(rect.right() - 6.0, rect.center().y), egui::Align2::RIGHT_CENTER, marker, FontId::proportional(SIZE), c.muted);
                            text(ui, &it.spans, SIZE, c.text, c);
                        });
                    }
                });
            }
            Block::Table { head, rows } => {
                egui::ScrollArea::horizontal().id_salt(("md-table", ui.next_auto_id())).show(ui, |ui| {
                    egui::Grid::new(ui.next_auto_id()).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                        for cell in head {
                            let bold: Vec<Span> = cell.iter().map(|s| Span { bold: true, ..s.clone() }).collect();
                            ui.add(egui::Label::new(job(&bold, 12.0, Color32::WHITE, c, 320.0)).wrap());
                        }
                        ui.end_row();
                        for row in rows {
                            for cell in row {
                                ui.add(egui::Label::new(job(cell, 12.0, c.text, c, 320.0)).wrap());
                            }
                            ui.end_row();
                        }
                    });
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn fences_headings_rules_and_paragraphs() {
        let b = parse("# Title\n\nSome text\nwraps here\n\n```rust\nfn a() {}\n```\n---\n### Small");
        assert_eq!(b.len(), 5, "{b:?}");
        assert!(matches!(&b[0], Block::Heading { big: true, spans } if plain(spans) == "Title"));
        assert!(matches!(&b[1], Block::Para(s) if plain(s) == "Some text\nwraps here"));
        assert_eq!(b[2], Block::Code { lang: "rust".into(), body: "fn a() {}".into() });
        assert_eq!(b[3], Block::Rule);
        assert!(matches!(&b[4], Block::Heading { big: false, .. }));
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        assert_eq!(parse("```\nstill streaming"), vec![Block::Code { lang: String::new(), body: "still streaming".into() }]);
    }

    #[test]
    fn tables_drop_the_separator_row() {
        let b = parse("| a | b |\n|---|:-:|\n| 1 | **2** |");
        let Block::Table { head, rows } = &b[0] else { panic!("{b:?}") };
        assert_eq!(head.iter().map(|c| plain(c)).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0][1][0].bold);
    }

    #[test]
    fn quotes_nest_blocks_and_lists_keep_continuations() {
        let b = parse("> quoted\n> - item\n\n1. one\n2. two\n   more of two\n- other kind");
        let Block::Quote(inner) = &b[0] else { panic!("{b:?}") };
        assert!(matches!(&inner[1], Block::List { ordered: false, .. }));
        let Block::List { ordered: true, items } = &b[1] else { panic!("{b:?}") };
        assert_eq!(items.len(), 3);
        assert!(items[2].continuation && plain(&items[2].spans) == "more of two");
        assert!(matches!(&b[2], Block::List { ordered: false, .. }), "a bullet after numbers starts a new list");
    }

    #[test]
    fn inline_code_bold_italic_and_links() {
        let s = inline("Run `cargo test` **now** or *later*, see [docs](https://x.y/z).");
        let kinds: Vec<(&str, bool, bool, bool, bool)> = s.iter().map(|s| (s.text.as_str(), s.code, s.bold, s.italic, s.link.is_some())).collect();
        assert_eq!(
            kinds,
            [
                ("Run ", false, false, false, false),
                ("cargo test", true, false, false, false),
                (" ", false, false, false, false),
                ("now", false, true, false, false),
                (" or ", false, false, false, false),
                ("later", false, false, true, false),
                (", see ", false, false, false, false),
                ("docs", false, false, false, true),
                (".", false, false, false, false),
            ]
        );
        assert_eq!(s[7].link.as_deref(), Some("https://x.y/z"));
    }

    #[test]
    fn snake_case_and_lone_stars_stay_plain() {
        assert_eq!(inline("call my_snake_case_fn and 2 * 3 * 4"), vec![Span { text: "call my_snake_case_fn and 2 * 3 * 4".into(), ..Span::default() }]);
        let nested = inline("**bold with `code`**");
        assert!(nested.iter().all(|s| s.bold) && nested[1].code);
    }
}

#[cfg(test)]
mod hostile {
    use super::*;

    const LINEAR_BUDGET: std::time::Duration = std::time::Duration::from_secs(1);

    #[test]
    fn deep_quotes_and_markers_do_not_overflow_the_stack() {
        let quotes = format!("{}deep", ">".repeat(3_000));
        let blocks = parse(&quotes);
        let mut depth = 0;
        let mut b = &blocks;
        while let Some(Block::Quote(inner)) = b.first() {
            depth += 1;
            b = inner;
        }
        assert!(depth <= MAX_QUOTE_DEPTH, "nesting capped, got {depth}");
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let c = Colors::of(ui);
            show(ui, &blocks, &c);
        })
        .textures_delta
        .clear();
        let markers = format!("{}x{}", "**__".repeat(2_000), "__**".repeat(2_000));
        assert!(!inline(&markers).is_empty());
    }

    #[test]
    fn links_without_an_end_stay_linear() {
        for text in ["[a](".repeat(15_000), "[".repeat(60_000)] {
            let t = std::time::Instant::now();
            inline(&text);
            assert!(t.elapsed() < LINEAR_BUDGET, "{} KB took {:?}", text.len() / 1024, t.elapsed());
        }
    }

    #[test]
    #[ignore = "measurement"]
    fn unmatched_markers_parse_time() {
        for (name, text) in [
            ("stars", "**a ".repeat(15_000)),
            ("underscores", "a_b ".repeat(15_000)),
            ("single stars", "*a ".repeat(20_000)),
            ("backticks", "`a ".repeat(20_000)),
            ("brackets", "[a](".repeat(15_000)),
            ("long link", format!("[a]({})", "x".repeat(60_000))),
            ("open brackets", "[".repeat(60_000)),
            ("italic underscores", " _a".repeat(20_000)),
            ("italic stars", " *a".repeat(20_000)),
            ("bold no close", " **a".repeat(15_000)),
        ] {
            let t = std::time::Instant::now();
            let n = inline(&text).len();
            println!("{name}: {} KB -> {n} spans in {:?}", text.len() / 1024, t.elapsed());
        }
        assert_eq!(inline("[docs](https://x.y/z)")[0].link.as_deref(), Some("https://x.y/z"));
    }
}
