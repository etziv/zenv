use std::path::Path;
use std::sync::OnceLock;

use base64::Engine;
use eframe::egui::{self, Id};
use fancy_regex::Regex;
use image::ImageEncoder;
use serde::Deserialize;
use serde_json::Value;
use zenv_core::studio_chat::{Attachment, AttachmentKind, ChatOptions, Effort, PermissionMode};

pub const EFFORTS: [(Option<Effort>, &str); 6] =
    [(None, "Default"), (Some(Effort::Low), "Low"), (Some(Effort::Medium), "Medium"), (Some(Effort::High), "High"), (Some(Effort::Xhigh), "Extra high"), (Some(Effort::Max), "Max")];

pub const LEVELS: [Effort; 5] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];

pub struct ModeInfo {
    pub value: Option<PermissionMode>,
    pub label: &'static str,
    pub hint: &'static str,
}

pub const MODES: [ModeInfo; 7] = [
    ModeInfo { value: None, label: "Settings", hint: "As in your Claude Code settings" },
    ModeInfo { value: Some(PermissionMode::Manual), label: "Ask first", hint: "Asks before edits and commands" },
    ModeInfo { value: Some(PermissionMode::AcceptEdits), label: "Accept edits", hint: "Edits files without asking; asks for commands" },
    ModeInfo { value: Some(PermissionMode::Auto), label: "Auto", hint: "Decides on its own, asks only for risky steps" },
    ModeInfo { value: Some(PermissionMode::Plan), label: "Plan", hint: "Reads and plans; changes nothing until you approve" },
    ModeInfo { value: Some(PermissionMode::DontAsk), label: "Don't ask", hint: "Never asks; refuses what isn't already allowed" },
    ModeInfo { value: Some(PermissionMode::BypassPermissions), label: "Bypass", hint: "Runs everything without asking. Careful" },
];

pub fn mode_name(m: PermissionMode) -> &'static str {
    match m {
        PermissionMode::Manual => "manual",
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::Auto => "auto",
        PermissionMode::Plan => "plan",
        PermissionMode::DontAsk => "dontAsk",
        PermissionMode::BypassPermissions => "bypassPermissions",
    }
}

pub fn mode_of(name: &str) -> Option<PermissionMode> {
    MODES.iter().filter_map(|m| m.value).find(|m| mode_name(*m) == name)
}

pub fn effort_label(e: Option<Effort>) -> &'static str {
    EFFORTS.iter().find(|(v, _)| *v == e).map_or("Default", |(_, l)| l)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Model {
    pub value: String,
    pub display_name: String,
    pub description: Option<String>,
    pub resolved_model: Option<String>,
    pub supported_effort_levels: Option<Vec<Effort>>,
    pub supports_fast_mode: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Command {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub email: Option<String>,
    pub organization: Option<String>,
    pub subscription_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Catalog {
    pub models: Vec<Model>,
    pub commands: Vec<Command>,
    pub account: Option<Account>,
    pub output_styles: Option<Vec<String>>,
    pub output_style: Option<String>,
    pub version: Option<String>,
}

impl Catalog {
    pub fn parse(v: &Value) -> Option<Self> {
        serde_json::from_value::<Catalog>(v.clone()).ok().filter(|c| !c.models.is_empty())
    }
}

pub fn fallback_models() -> Vec<Model> {
    [("default", "Default", "Your account's default model"), ("opus", "Opus", "Latest Opus"), ("sonnet", "Sonnet", "Latest Sonnet"), ("haiku", "Haiku", "Latest Haiku"), ("fable", "Fable", "Latest Fable")]
        .into_iter()
        .map(|(v, n, d)| Model { value: v.into(), display_name: n.into(), description: Some(d.into()), ..Model::default() })
        .collect()
}

pub fn fallback_commands() -> Vec<Command> {
    ["compact", "context", "init", "review", "security-review", "usage"].into_iter().map(|n| Command { name: n.into(), description: None }).collect()
}

const OPTIONS_KEY: &str = "zenv.studio.chat";
const CATALOG_KEY: &str = "zenv.studio.chat.catalog";
const CATALOG_AGE_MS: u64 = 24 * 3600 * 1000;

pub fn valid_model(m: &str) -> bool {
    let ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '[' | ']');
    (1..=80).contains(&m.len()) && m.chars().next().is_some_and(ok) && m.chars().all(|c| ok(c) || c == '-')
}

fn valid_style(s: &str) -> bool {
    (1..=60).contains(&s.chars().count()) && s.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
}

pub fn parse_options(json: &str) -> ChatOptions {
    let mut o: ChatOptions = serde_json::from_str(json).unwrap_or_default();
    o.model = o.model.filter(|m| valid_model(m));
    o.output_style = o.output_style.filter(|s| valid_style(s));
    o
}

pub fn read_options(ctx: &egui::Context) -> ChatOptions {
    ctx.data_mut(|d| d.get_persisted::<String>(Id::unique(OPTIONS_KEY))).map(|s| parse_options(&s)).unwrap_or_default()
}

pub fn save_options(ctx: &egui::Context, o: &ChatOptions) {
    if let Ok(s) = serde_json::to_string(o) {
        ctx.data_mut(|d| d.insert_persisted(Id::unique(OPTIONS_KEY), s));
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn read_catalog(ctx: &egui::Context) -> (Option<Catalog>, bool) {
    let Some((at, json)) = ctx.data_mut(|d| d.get_persisted::<(u64, String)>(Id::unique(CATALOG_KEY))) else { return (None, false) };
    let catalog = serde_json::from_str::<Value>(&json).ok().as_ref().and_then(Catalog::parse);
    let fresh = catalog.is_some() && now_ms().saturating_sub(at) < CATALOG_AGE_MS;
    (catalog, fresh)
}

pub fn save_catalog(ctx: &egui::Context, raw: &Value) {
    ctx.data_mut(|d| d.insert_persisted(Id::unique(CATALOG_KEY), (now_ms(), raw.to_string())));
}

fn model_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^claude-([a-z]+)-(\d+)(?:-(\d{1,2}))?(?:-\d{8})?(\[1m\])?$").expect("model pattern"))
}

pub fn pretty_model(id: Option<&str>) -> Option<String> {
    let id = id.filter(|s| !s.is_empty())?;
    let Ok(Some(m)) = model_re().captures(id) else { return Some(id.to_owned()) };
    let family = m.get(1).map_or("", |g| g.as_str());
    let mut chars = family.chars();
    let family = chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
    let minor = m.get(3).map(|g| format!(".{}", g.as_str())).unwrap_or_default();
    let wide = if m.get(4).is_some() { " (1M)" } else { "" };
    Some(format!("{family} {}{minor}{wide}", m.get(2).map_or("", |g| g.as_str())))
}

pub fn until_text(epoch_seconds: f64, now: u64) -> String {
    let mins = ((epoch_seconds * 1000.0 - now as f64) / 60000.0).round().max(0.0) as u64;
    if mins < 60 { format!("{mins}m") } else { format!("{}h {}m", mins / 60, mins % 60) }
}

fn local(ms: u64) -> time::OffsetDateTime {
    let t = time::OffsetDateTime::from_unix_timestamp((ms / 1000) as i64).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    time::UtcOffset::current_local_offset().map_or(t, |o| t.to_offset(o))
}

pub fn ago(ms: u64, now: u64) -> String {
    let mins = ((now as f64 - ms as f64) / 60000.0).round().max(0.0) as u64;
    match mins {
        0 => "just now".into(),
        1..60 => format!("{mins}m ago"),
        60..1440 => format!("{}h ago", mins / 60),
        1440..10080 => format!("{}d ago", mins / 1440),
        _ => {
            let d = local(ms);
            format!("{}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
        }
    }
}

pub fn reset_at(epoch_seconds: f64) -> String {
    let d = local((epoch_seconds.max(0.0) * 1000.0) as u64);
    format!("{} {:02}:{:02}", &d.weekday().to_string()[..3], d.hour(), d.minute())
}

pub const MAX_FILE: u64 = 20 * 1024 * 1024;
const MAX_TEXT: u64 = 256 * 1024;

fn text_ext() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\.(txt|md|markdown|json|jsonc|ya?ml|toml|ini|env|cfg|conf|log|csv|tsv|xml|svg|html?|css|scss|less|js|mjs|cjs|jsx|ts|tsx|vue|svelte|php|blade|py|go|rs|rb|java|kt|c|h|cpp|hpp|cs|swift|sh|bash|ps1|bat|cmd|sql|graphql|gql|dockerfile|gitignore|lock)$")
            .expect("text extensions")
    })
}

fn image_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

pub fn attachment_from(name: &str, bytes: Vec<u8>) -> Result<Attachment, String> {
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{name} is larger than 20 MB"));
    }
    if let Some(media) = image_type(name) {
        return Ok(Attachment { kind: AttachmentKind::Image, name: name.into(), media_type: media.into(), data: b64(&bytes) });
    }
    if text_ext().is_match(name).unwrap_or(false) && bytes.len() as u64 <= MAX_TEXT {
        if let Ok(text) = String::from_utf8(bytes.clone()) {
            return Ok(Attachment { kind: AttachmentKind::Text, name: name.into(), media_type: "text/plain".into(), data: text });
        }
    }
    Ok(Attachment { kind: AttachmentKind::File, name: name.into(), media_type: "application/octet-stream".into(), data: b64(&bytes) })
}

pub fn attachment_from_path(path: &Path) -> Result<Attachment, String> {
    let name = path.file_name().map_or_else(|| "pasted".into(), |n| n.to_string_lossy().into_owned());
    let size = std::fs::metadata(path).map_err(|e| format!("cannot read {name}: {e}"))?.len();
    if path.is_dir() {
        return Err(format!("{name} is a folder; attach files"));
    }
    if size > MAX_FILE {
        return Err(format!("{name} is larger than 20 MB"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {name}: {e}"))?;
    attachment_from(&name, bytes)
}

pub fn attachment_from_image(image: &egui::ColorImage) -> Result<Attachment, String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("cannot read the pasted image: {e}"))?;
    attachment_from("pasted.png", png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_model_names_versions() {
        assert_eq!(pretty_model(Some("claude-opus-5-5[1m]")).as_deref(), Some("Opus 5.5 (1M)"));
        assert_eq!(pretty_model(Some("claude-sonnet-4-5-20250929")).as_deref(), Some("Sonnet 4.5"));
        assert_eq!(pretty_model(Some("claude-haiku-4")).as_deref(), Some("Haiku 4"));
        assert_eq!(pretty_model(Some("my-proxy-model")).as_deref(), Some("my-proxy-model"));
        assert_eq!(pretty_model(None), None);
    }

    #[test]
    fn times_read_like_the_composer() {
        let now = 1_000_000_000_000;
        assert_eq!(until_text((now / 1000 + 57 * 60) as f64, now), "57m");
        assert_eq!(until_text((now / 1000 + 185 * 60) as f64, now), "3h 5m");
        assert_eq!(until_text(0.0, now), "0m");
        assert_eq!(ago(now - 20_000, now), "just now");
        assert_eq!(ago(now - 5 * 60_000, now), "5m ago");
        assert_eq!(ago(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(ago(now - 2 * 86_400_000, now), "2d ago");
        assert!(ago(now - 30 * 86_400_000, now).starts_with("2001-"));
    }

    #[test]
    fn model_names_are_checked_like_core() {
        assert!(valid_model("opus") && valid_model("sonnet[1m]") && valid_model("claude-opus-5-5"));
        assert!(!valid_model("-rf") && !valid_model("a b") && !valid_model("") && !valid_model(&"a".repeat(81)));
    }

    #[test]
    fn saved_options_keep_only_valid_values() {
        let o = parse_options(r#"{"model":"opus","effort":"high","mode":"plan","thinking":false,"outputStyle":"Explanatory"}"#);
        assert_eq!(o.model.as_deref(), Some("opus"));
        assert_eq!(o.effort, Some(Effort::High));
        assert_eq!(o.mode, Some(PermissionMode::Plan));
        assert_eq!(o.thinking, Some(false));
        assert_eq!(o.output_style.as_deref(), Some("Explanatory"));
        assert_eq!(parse_options(r#"{"model":"--evil","outputStyle":"<x>"}"#), ChatOptions::default());
        assert_eq!(parse_options("not json"), ChatOptions::default());
        let back = parse_options(&serde_json::to_string(&o).unwrap());
        assert_eq!(back, o, "options survive a save");
    }

    #[test]
    fn files_go_in_the_way_core_takes_them() {
        assert_eq!(attachment_from("shot.PNG", vec![1, 2, 3]).unwrap().kind, AttachmentKind::Image);
        let text = attachment_from("main.rs", b"fn main() {}".to_vec()).unwrap();
        assert_eq!((text.kind, text.data.as_str()), (AttachmentKind::Text, "fn main() {}"));
        assert_eq!(attachment_from("big.log", vec![b'a'; 300 * 1024]).unwrap().kind, AttachmentKind::File);
        assert_eq!(attachment_from("data.bin", vec![0; 10]).unwrap().kind, AttachmentKind::File);
        assert_eq!(attachment_from("bad.txt", vec![0xff, 0xfe]).unwrap().kind, AttachmentKind::File);
        assert!(attachment_from("huge.zip", vec![0; (MAX_FILE + 1) as usize]).unwrap_err().contains("larger than 20 MB"));
    }

    #[test]
    fn a_pasted_image_becomes_a_png() {
        let img = egui::ColorImage::new([2, 2], vec![egui::Color32::RED; 4]);
        let a = attachment_from_image(&img).unwrap();
        assert_eq!((a.kind, a.media_type.as_str(), a.name.as_str()), (AttachmentKind::Image, "image/png", "pasted.png"));
        let bytes = base64::engine::general_purpose::STANDARD.decode(a.data).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
    }

    #[test]
    fn the_catalog_reads_claude_codes_answer() {
        let v = serde_json::json!({
            "models": [{ "value": "default", "displayName": "Default", "resolvedModel": "claude-opus-5-5", "supportedEffortLevels": ["low", "high"] }],
            "commands": [{ "name": "compact", "description": "Compact" }],
            "account": { "email": "a@b.c" }, "outputStyles": ["default", "Explanatory"], "version": "2.1.0"
        });
        let c = Catalog::parse(&v).unwrap();
        assert_eq!(c.models[0].supported_effort_levels, Some(vec![Effort::Low, Effort::High]));
        assert_eq!(c.account.and_then(|a| a.email).as_deref(), Some("a@b.c"));
        assert_eq!(Catalog::parse(&serde_json::json!({ "models": [] })), None);
    }
}
