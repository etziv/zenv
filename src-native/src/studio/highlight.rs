use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::OnceLock;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontFamily, FontId, Stroke};
use syntect::highlighting::{Color, FontStyle, HighlightIterator, HighlightState, Highlighter, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

pub const FONT_SIZE: f32 = 13.0;
pub const LINE_HEIGHT: f32 = 20.0;
pub const COLOUR_MAX: usize = 100 * 1024;
pub const COLOUR_LINE_MAX: usize = 2 * 1024;
pub const FOREGROUND: Color32 = Color32::from_rgb(0xd4, 0xd4, 0xd4);

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let rules: &[(&str, u32, FontStyle)] = &[
            ("comment, punctuation.definition.comment", 0x6a9955, FontStyle::empty()),
            ("string, punctuation.definition.string", 0xce9178, FontStyle::empty()),
            ("string.regexp, constant.character.escape", 0xd16969, FontStyle::empty()),
            ("constant.numeric", 0xb5cea8, FontStyle::empty()),
            ("constant.language, variable.language, storage, storage.type, storage.modifier, keyword, keyword.operator.word, keyword.operator.new, keyword.other", 0x569cd6, FontStyle::empty()),
            ("keyword.control, keyword.control.import", 0xc586c0, FontStyle::empty()),
            ("keyword.operator, punctuation", 0xd4d4d4, FontStyle::empty()),
            ("entity.name.function, support.function, variable.function, meta.function-call.identifier", 0xdcdcaa, FontStyle::empty()),
            ("entity.name.class, entity.name.type, entity.name.namespace, entity.other.inherited-class, support.class, support.type, storage.type.class.name", 0x4ec9b0, FontStyle::empty()),
            ("variable, variable.parameter, variable.other, meta.property-name, support.variable, entity.other.attribute-name, punctuation.definition.variable", 0x9cdcfe, FontStyle::empty()),
            ("constant.other, variable.other.constant, support.constant", 0x4fc1ff, FontStyle::empty()),
            ("entity.name.tag, punctuation.definition.tag, punctuation.section.embedded, meta.tag.sgml, entity.other.attribute-name.id", 0x569cd6, FontStyle::empty()),
            ("markup.heading, entity.name.section", 0x569cd6, FontStyle::BOLD),
            ("markup.bold", 0xd4d4d4, FontStyle::BOLD),
            ("markup.italic", 0xd4d4d4, FontStyle::ITALIC),
            ("markup.underline.link, string.other.link", 0x3794ff, FontStyle::UNDERLINE),
            ("invalid", 0xf44747, FontStyle::empty()),
        ];
        let rgb = |c: u32| Color { r: (c >> 16) as u8, g: (c >> 8) as u8, b: c as u8, a: 0xff };
        Theme {
            settings: ThemeSettings { foreground: Some(rgb(0xd4d4d4)), ..ThemeSettings::default() },
            scopes: rules
                .iter()
                .map(|&(scope, color, font_style)| ThemeItem {
                    scope: ScopeSelectors::from_str(scope).expect("valid scope selector"),
                    style: StyleModifier { foreground: Some(rgb(color)), background: None, font_style: Some(font_style) },
                })
                .collect(),
            ..Theme::default()
        }
    })
}

pub fn syntax_for(name: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let lower = name.to_lowercase();
    if lower == "artisan" || lower.ends_with(".blade.php") {
        return set.find_syntax_by_extension("php");
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    let ext = match ext {
        "ts" | "tsx" | "mts" | "cts" | "jsx" | "mjs" | "cjs" => "js",
        "vue" | "svelte" => "html",
        other => other,
    };
    set.find_syntax_by_extension(ext).filter(|s| s.name != "Plain Text")
}

pub fn longest_line(text: &str) -> usize {
    text.split('\n').map(str::len).max().unwrap_or(0)
}

pub fn colourable(text: &str) -> bool {
    text.len() <= COLOUR_MAX && longest_line(text) <= COLOUR_LINE_MAX
}

struct Line {
    hash: u64,
    start: (ParseState, HighlightState),
    spans: Vec<(usize, usize, Color32, FontStyle)>,
}

pub struct Highlight {
    syntax: Option<&'static SyntaxReference>,
    language: Option<&'static str>,
    lines: Vec<Line>,
    pub recoloured: usize,
}

fn hash_of(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Highlight {
    pub fn new(name: &str, text: &str) -> Self {
        let syntax = syntax_for(name);
        Self { language: syntax.map(|s| s.name.as_str()), syntax: syntax.filter(|_| colourable(text)), lines: Vec::new(), recoloured: 0 }
    }

    pub fn language(&self) -> Option<&'static str> {
        self.language
    }

    pub fn coloured(&self) -> bool {
        self.syntax.is_some()
    }

    pub fn job(&mut self, text: &str) -> LayoutJob {
        let font = FontId::new(FONT_SIZE, FontFamily::Monospace);
        let mut job = LayoutJob { text: text.to_owned(), ..LayoutJob::default() };
        let plain = |color| TextFormat { font_id: font.clone(), color, line_height: Some(LINE_HEIGHT), ..TextFormat::default() };
        let Some(syntax) = self.syntax else {
            job.sections.push(eframe::egui::text::LayoutSection { leading_space: 0.0, byte_range: 0.into()..text.len().into(), format: plain(FOREGROUND) });
            return job;
        };
        self.update(syntax, text);
        let mut offset = 0;
        for (line, src) in self.lines.iter().zip(text.split_inclusive('\n')) {
            for &(a, b, color, style) in &line.spans {
                let mut format = plain(color);
                format.italics = style.contains(FontStyle::ITALIC);
                if style.contains(FontStyle::UNDERLINE) {
                    format.underline = Stroke::new(1.0, color);
                }
                job.sections.push(eframe::egui::text::LayoutSection { leading_space: 0.0, byte_range: (offset + a).into()..(offset + b).into(), format });
            }
            offset += src.len();
        }
        job
    }

    fn update(&mut self, syntax: &'static SyntaxReference, text: &str) {
        let set = syntaxes();
        let highlighter = Highlighter::new(theme());
        let new: Vec<&str> = text.split_inclusive('\n').collect();
        let hashes: Vec<u64> = new.iter().map(|l| hash_of(l)).collect();
        let old = std::mem::take(&mut self.lines);
        let prefix = old.iter().zip(&hashes).take_while(|(o, h)| o.hash == **h).count();
        let room = old.len().min(new.len()) - prefix;
        let suffix = old.iter().rev().zip(hashes.iter().rev()).take(room).take_while(|(o, h)| o.hash == **h).count();
        let mut old = old.into_iter();
        let mut lines: Vec<Line> = old.by_ref().take(prefix).collect();
        let rest: Vec<Line> = old.collect();
        let mut state = match (lines.last(), new.get(prefix.wrapping_sub(1))) {
            (Some(last), Some(src)) => colour(set, &highlighter, last.start.clone(), src).1,
            _ => (ParseState::new(syntax), HighlightState::new(&highlighter, ScopeStack::new())),
        };
        self.recoloured = 0;
        let shift = rest.len() as isize - (new.len() - prefix) as isize;
        let mut rest = rest.into_iter().enumerate().peekable();
        for j in prefix..new.len() {
            if j >= new.len() - suffix {
                let o = (j - prefix) as isize + shift;
                while rest.peek().is_some_and(|(i, _)| (*i as isize) < o) {
                    rest.next();
                }
                if rest.peek().is_some_and(|(_, l)| l.start == state) {
                    lines.extend(rest.map(|(_, l)| l));
                    break;
                }
            }
            let (spans, end) = colour(set, &highlighter, state.clone(), new[j]);
            lines.push(Line { hash: hashes[j], start: state, spans });
            self.recoloured += 1;
            state = end;
        }
        self.lines = lines;
    }
}

type States = (ParseState, HighlightState);

fn colour(set: &SyntaxSet, highlighter: &Highlighter, (mut parse, mut highlight): States, line: &str) -> (Vec<(usize, usize, Color32, FontStyle)>, States) {
    let ops = parse.parse_line(line, set).unwrap_or_default();
    let mut spans = Vec::new();
    let mut at = 0;
    for (style, piece) in HighlightIterator::new(&mut highlight, &ops, line, highlighter) {
        let c = style.foreground;
        spans.push((at, at + piece.len(), Color32::from_rgb(c.r, c.g, c.b), style.font_style));
        at += piece.len();
    }
    (spans, (parse, highlight))
}
