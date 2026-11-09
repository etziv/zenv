//! Pointing the Codex CLI at a local AI router (OmniRoute or 9router).
//!
//! Codex reads `~/.codex/config.toml` (or `%CODEX_HOME%\config.toml`). Zenv
//! adds a provider and a profile there, both named after the router (see
//! [`CodexTarget`]), and leaves everything else as it is (comments and
//! formatting included):
//!
//! - `[model_providers.<id>]`: the router's `/v1` with the Responses API.
//! - `[profiles.<id>]`: that provider, for `codex --profile <id>`.
//! - Default: the top-level `profile = "<id>"`. Taking it away again gives
//!   back exactly the previous setup.
//!
//! The file is backed up next to itself before every change.

use std::path::PathBuf;

use serde::Serialize;
use toml_edit::{value, DocumentMut, Item, Table};

use crate::error::{AppError, AppResult, IoContext};

/// How one router appears in Codex's config.
#[derive(Debug, Clone, Copy)]
pub struct CodexTarget {
    /// Provider and profile name.
    pub id: &'static str,
    /// The provider's display name.
    pub label: &'static str,
    /// Environment variable holding the router's API key, when it needs one.
    pub env_key: Option<&'static str>,
    /// The profile's model when it has none yet (`None`: the user picks one,
    /// e.g. a combo of theirs).
    pub model: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub config_path: PathBuf,
    pub exists: bool,
    /// The router's provider is in the file.
    pub configured: bool,
    /// Its address, when configured.
    pub base_url: Option<String>,
    /// `profile = "<id>"`, or `model_provider = "<id>"` without a profile,
    /// makes it Codex's default.
    pub is_default: bool,
    /// The provider Codex uses without a profile (e.g. "ninerouter").
    pub top_level_provider: Option<String>,
    /// The profile Codex uses by default, if any.
    pub default_profile: Option<String>,
    /// The model the router's profile asks for, if set.
    pub profile_model: Option<String>,
    /// The model Codex asks the router for: the top-level `model` when the
    /// router is the default through `model_provider`, else the profile's.
    pub model: Option<String>,
    /// Top-level `model_context_window` / `model_max_output_tokens` (they
    /// apply to whichever provider is the default).
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
}

pub fn config_path() -> PathBuf {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"));
    home.join("config.toml")
}

fn read(path: &std::path::Path) -> AppResult<DocumentMut> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).ctx(|| format!("reading {}", path.display())),
    };
    text.parse::<DocumentMut>().map_err(|e| AppError::Validation(format!("{} is not valid TOML: {e}", path.display())))
}

fn top_str<'a>(doc: &'a DocumentMut, key: &str) -> Option<&'a str> {
    doc.get(key).and_then(|v| v.as_str())
}

fn is_default(doc: &DocumentMut, id: &str) -> bool {
    match top_str(doc, "profile") {
        Some(profile) => profile == id,
        None => top_str(doc, "model_provider") == Some(id),
    }
}

/// The router is the default through the top-level `model_provider` (no
/// `profile` line), so the top-level `model` is the one it gets.
fn default_by_provider(doc: &DocumentMut, id: &str) -> bool {
    top_str(doc, "profile").is_none() && top_str(doc, "model_provider") == Some(id)
}

/// Status from a document (testable without files).
pub fn status_of(doc: &DocumentMut, target: &CodexTarget, path: PathBuf, exists: bool) -> CodexStatus {
    let provider = doc.get("model_providers").and_then(|p| p.get(target.id));
    let profile_model = doc
        .get("profiles")
        .and_then(|p| p.get(target.id))
        .and_then(|p| p.get("model"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let top_int = |key: &str| doc.get(key).and_then(|v| v.as_integer());
    CodexStatus {
        config_path: path,
        exists,
        configured: provider.is_some(),
        base_url: provider.and_then(|p| p.get("base_url")).and_then(|v| v.as_str()).map(str::to_string),
        is_default: is_default(doc, target.id),
        top_level_provider: top_str(doc, "model_provider").map(str::to_string),
        default_profile: top_str(doc, "profile").map(str::to_string),
        model: if default_by_provider(doc, target.id) { top_str(doc, "model").map(str::to_string) } else { profile_model.clone() },
        profile_model,
        context_window: top_int("model_context_window"),
        max_output: top_int("model_max_output_tokens"),
    }
}

/// Points the router's profile at `model`. When the router is Codex's
/// default, the top-level `model` (if that is what Codex reads) and the
/// context limits that are given follow, so Codex sizes its requests for it.
pub fn choose_model(doc: &mut DocumentMut, target: &CodexTarget, model: &str, context_window: Option<u64>, max_output: Option<u64>) -> AppResult<()> {
    let model = model.trim();
    if model.is_empty() || model.chars().any(char::is_control) {
        return Err(AppError::Validation("choose a model name".into()));
    }
    let id = target.id;
    let profile = doc
        .get_mut("profiles")
        .and_then(|p| p.get_mut(id))
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::Conflict(format!("add {} to Codex first", target.label)))?;
    profile["model"] = value(model);
    // The "add a model here" hint is done.
    profile.decor_mut().set_prefix("\n");
    if is_default(doc, id) {
        if default_by_provider(doc, id) {
            doc["model"] = value(model);
        }
        let limit = |n: u64| value(i64::try_from(n).unwrap_or(i64::MAX));
        if let Some(n) = context_window.filter(|n| *n > 0) {
            doc["model_context_window"] = limit(n);
        }
        if let Some(n) = max_output.filter(|n| *n > 0) {
            doc["model_max_output_tokens"] = limit(n);
        }
    }
    Ok(())
}

/// The API key Codex sends to the router: the provider's
/// `experimental_bearer_token`, or the environment variable its `env_key`
/// names (as stored for the user, so a key set after Zenv started counts).
pub fn api_key(target: &CodexTarget) -> AppResult<Option<String>> {
    let doc = read(&config_path())?;
    let provider = doc.get("model_providers").and_then(|p| p.get(target.id));
    if let Some(token) = provider.and_then(|p| p.get("experimental_bearer_token")).and_then(|v| v.as_str()) {
        return Ok(Some(token.to_string()));
    }
    let Some(name) = provider.and_then(|p| p.get("env_key")).and_then(|v| v.as_str()).or(target.env_key) else { return Ok(None) };
    if let Ok(v) = std::env::var(name) {
        return Ok(Some(v));
    }
    Ok(crate::platform::read_env_var(false, name)?.or(crate::platform::read_env_var(true, name)?))
}

/// Adds (or updates) the router's provider and profile; `make_default` also
/// selects the profile for plain `codex`. A key the user already put in the
/// provider (`env_key` or `experimental_bearer_token`) stays.
pub fn apply(doc: &mut DocumentMut, target: &CodexTarget, base_url: &str, make_default: bool) {
    let id = target.id;
    let providers = doc.entry("model_providers").or_insert(Item::Table(Table::new()));
    if let Some(t) = providers.as_table_mut() {
        t.set_implicit(true);
    }
    let provider = providers.as_table_mut().map(|t| t.entry(id).or_insert(Item::Table(Table::new())));
    if let Some(p) = provider.and_then(Item::as_table_mut) {
        p["name"] = value(target.label);
        p["base_url"] = value(base_url);
        let has_key = p.contains_key("env_key") || p.contains_key("experimental_bearer_token");
        if let (Some(env_key), false) = (target.env_key, has_key) {
            p["env_key"] = value(env_key);
        }
        p["wire_api"] = value("responses");
        // A local router may switch backends mid-answer.
        p["request_max_retries"] = value(4);
        p["stream_max_retries"] = value(5);
        p["stream_idle_timeout_ms"] = value(300_000);
        let key_note = match target.env_key {
            Some(env) => format!(
                "# Its API key (from the router's dashboard) is read from the environment\n# variable {env}; or put it here as experimental_bearer_token = \"<key>\".\n"
            ),
            None => "# If it asks for an API key, add env_key = \"<NAME>\" (an environment\n# variable holding it) or experimental_bearer_token = \"<key>\".\n".to_string(),
        };
        p.decor_mut().set_prefix(format!("\n# Added by Zenv: {} on this computer.\n{key_note}", target.label));
    }
    let profiles = doc.entry("profiles").or_insert(Item::Table(Table::new()));
    if let Some(t) = profiles.as_table_mut() {
        t.set_implicit(true);
    }
    let profile = profiles.as_table_mut().map(|t| t.entry(id).or_insert(Item::Table(Table::new())));
    if let Some(p) = profile.and_then(Item::as_table_mut) {
        p["model_provider"] = value(id);
        if p.get("model").is_none() {
            match target.model {
                Some(model) => p["model"] = value(model),
                None => p.decor_mut().set_prefix("\n# Add model = \"<a model or combo name from the router>\" here.\n"),
            }
        }
    }
    if make_default {
        doc["profile"] = value(id);
    }
}

/// Makes the router's profile Codex's default, or takes that back.
pub fn set_default(doc: &mut DocumentMut, target: &CodexTarget, on: bool) -> AppResult<()> {
    let id = target.id;
    if on {
        if doc.get("profiles").and_then(|p| p.get(id)).is_none() {
            return Err(AppError::Conflict(format!("add {} to Codex first", target.label)));
        }
        doc["profile"] = value(id);
    } else if top_str(doc, "profile") == Some(id) {
        doc.remove("profile");
    } else if is_default(doc, id) {
        // Zenv doesn't know which provider to go back to.
        return Err(AppError::Conflict(format!(
            "config.toml itself sets model_provider = \"{id}\"; change that line to use another provider"
        )));
    }
    Ok(())
}

fn write(path: &std::path::Path, doc: &DocumentMut) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
    }
    if path.is_file() {
        let backup = path.with_extension(format!("toml.zenv-{}.bak", crate::util::file_timestamp()));
        std::fs::copy(path, &backup).ctx(|| format!("backing up {}", path.display()))?;
        prune_backups(path, KEEP_BACKUPS);
    }
    let tmp = path.with_extension("toml.zenv-tmp");
    std::fs::write(&tmp, doc.to_string()).ctx(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).ctx(|| format!("replacing {}", path.display()))
}

/// Backups Zenv keeps of config.toml. Each one holds whatever the file held,
/// API keys included, so old ones do not pile up.
const KEEP_BACKUPS: usize = 3;

/// Removes all but the newest `keep` of Zenv's `config.toml.zenv-<time>.bak`
/// next to `path` (the time stamp sorts by name).
fn prune_backups(path: &std::path::Path, keep: usize) {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_name().map(|n| n.to_string_lossy().into_owned())) else { return };
    let prefix = format!("{stem}.zenv-");
    let mut backups: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".bak")))
        .map(|e| e.path())
        .collect();
    backups.sort();
    for old in backups.iter().rev().skip(keep) {
        if let Err(e) = std::fs::remove_file(old) {
            tracing::warn!(file = %old.display(), error = %e, "old config.toml backup not removed");
        }
    }
}

/// The text "Add to Codex" writes, for pasting into config.toml by hand.
pub fn snippet(target: &CodexTarget, base_url: &str) -> String {
    let mut doc = DocumentMut::new();
    apply(&mut doc, target, base_url, false);
    format!(
        "# Paste at the end of {}
{}
# Optional: to make `codex` use {} by default, put this line at the
# top of the file, before any [section]. Remove it to go back.
# profile = \"{}\"
",
        config_path().display(),
        doc.to_string().trim(),
        target.label,
        target.id
    )
}

pub fn status(target: &CodexTarget) -> AppResult<CodexStatus> {
    let path = config_path();
    let exists = path.is_file();
    Ok(status_of(&read(&path)?, target, path, exists))
}

pub fn setup(target: &CodexTarget, base_url: &str, make_default: bool) -> AppResult<CodexStatus> {
    let path = config_path();
    let mut doc = read(&path)?;
    apply(&mut doc, target, base_url, make_default);
    write(&path, &doc)?;
    status(target)
}

pub fn set_model(target: &CodexTarget, model: &str, context_window: Option<u64>, max_output: Option<u64>) -> AppResult<CodexStatus> {
    let path = config_path();
    let mut doc = read(&path)?;
    choose_model(&mut doc, target, model, context_window, max_output)?;
    write(&path, &doc)?;
    status(target)
}

pub fn use_by_default(target: &CodexTarget, on: bool) -> AppResult<CodexStatus> {
    let path = config_path();
    let mut doc = read(&path)?;
    set_default(&mut doc, target, on)?;
    write(&path, &doc)?;
    status(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OMNI: CodexTarget = CodexTarget { id: "omniroute", label: "OmniRoute (Zenv)", env_key: None, model: Some("auto") };
    const NINE: CodexTarget = CodexTarget { id: "ninerouter", label: "9router (Zenv)", env_key: Some("NINEROUTER_API_KEY"), model: None };

    const USER: &str = r#"model = "my-combo"
model_provider = "ninerouter"

# my-combo = combo
model_context_window = 1000000

[model_providers.ninerouter]
name = "9router"
base_url = "http://localhost:20128/v1"
env_key = "NINEROUTER_API_KEY"
wire_api = "responses"

[projects.'c:\code\shop']
trust_level = "trusted"
"#;

    #[test]
    fn adds_a_profile_and_keeps_the_rest() {
        let mut doc: DocumentMut = USER.parse().unwrap();
        apply(&mut doc, &OMNI, "http://127.0.0.1:20128/v1", false);
        let text = doc.to_string();
        assert!(text.starts_with("model = \"my-combo\"\nmodel_provider = \"ninerouter\"\n\n# my-combo = combo\n"), "{text}");
        assert!(text.contains("[model_providers.ninerouter]\nname = \"9router\""));
        assert!(text.contains("[projects.'c:\\code\\shop']"));
        let st = status_of(&doc, &OMNI, PathBuf::new(), true);
        assert!(st.configured && !st.is_default);
        assert_eq!(st.base_url.as_deref(), Some("http://127.0.0.1:20128/v1"));
        assert_eq!(st.top_level_provider.as_deref(), Some("ninerouter"));
        assert_eq!(st.profile_model.as_deref(), Some("auto"));

        // Default on and off again gives back the same file.
        let before = doc.to_string();
        set_default(&mut doc, &OMNI, true).unwrap();
        assert!(status_of(&doc, &OMNI, PathBuf::new(), true).is_default);
        let reparsed: DocumentMut = doc.to_string().parse().unwrap();
        assert_eq!(reparsed["profile"].as_str(), Some("omniroute"));
        set_default(&mut doc, &OMNI, false).unwrap();
        assert_eq!(doc.to_string(), before);

        // Running setup again only updates the address.
        apply(&mut doc, &OMNI, "http://127.0.0.1:20200/v1", false);
        assert_eq!(doc.to_string().matches("[model_providers.omniroute]").count(), 1);
        assert_eq!(status_of(&doc, &OMNI, PathBuf::new(), true).base_url.as_deref(), Some("http://127.0.0.1:20200/v1"));
    }

    #[test]
    fn ninerouter_keeps_the_users_key_and_model() {
        // The user's own 9router provider and default stay as they are.
        let mut doc: DocumentMut = USER.parse().unwrap();
        assert!(status_of(&doc, &NINE, PathBuf::new(), true).is_default);
        apply(&mut doc, &NINE, "http://127.0.0.1:20128/v1", false);
        assert_eq!(doc["model_providers"]["ninerouter"]["env_key"].as_str(), Some("NINEROUTER_API_KEY"));
        assert_eq!(doc["model_providers"]["ninerouter"]["base_url"].as_str(), Some("http://127.0.0.1:20128/v1"));
        assert!(doc["profiles"]["ninerouter"].get("model").is_none(), "no made-up model");

        // A key put in the file by hand is not replaced by env_key.
        let mut doc: DocumentMut = "[model_providers.ninerouter]\nexperimental_bearer_token = \"sk-x\"\n".parse().unwrap();
        apply(&mut doc, &NINE, "http://127.0.0.1:20128/v1", true);
        assert!(doc["model_providers"]["ninerouter"].get("env_key").is_none());
        let st = status_of(&doc, &NINE, PathBuf::new(), true);
        assert!(st.is_default && st.default_profile.as_deref() == Some("ninerouter"));
        // The other router is not the default any more.
        assert!(!status_of(&doc, &OMNI, PathBuf::new(), true).is_default);
    }

    #[test]
    fn snippet_is_valid_toml_with_the_provider_and_profile() {
        let text = snippet(&OMNI, "http://127.0.0.1:20128/v1");
        let doc: DocumentMut = text.parse().unwrap();
        assert_eq!(doc["model_providers"]["omniroute"]["base_url"].as_str(), Some("http://127.0.0.1:20128/v1"));
        assert_eq!(doc["profiles"]["omniroute"]["model"].as_str(), Some("auto"));
        assert!(doc.get("profile").is_none(), "the default line stays a comment");
        let doc: DocumentMut = snippet(&NINE, "http://127.0.0.1:20128/v1").parse().unwrap();
        assert_eq!(doc["model_providers"]["ninerouter"]["env_key"].as_str(), Some("NINEROUTER_API_KEY"));
    }

    #[test]
    fn a_top_level_provider_is_the_default() {
        let mut doc: DocumentMut = USER.replace("\"ninerouter\"\n", "\"omniroute\"\n").parse().unwrap();
        apply(&mut doc, &OMNI, "http://127.0.0.1:20128/v1", false);
        assert!(status_of(&doc, &OMNI, PathBuf::new(), true).is_default);
        assert!(set_default(&mut doc, &OMNI, false).is_err(), "Zenv can't pick the provider to go back to");
        // A profile wins over the top-level provider.
        doc["profile"] = value("other");
        assert!(!status_of(&doc, &OMNI, PathBuf::new(), true).is_default);
    }

    #[test]
    fn choosing_a_model_follows_how_codex_picks_the_router() {
        // Default through model_provider: the top-level model and limits change.
        let mut doc: DocumentMut = USER.parse().unwrap();
        apply(&mut doc, &NINE, "http://127.0.0.1:20128/v1", false);
        choose_model(&mut doc, &NINE, "main", Some(200_000), Some(32_000)).unwrap();
        assert_eq!(doc["model"].as_str(), Some("main"));
        assert_eq!(doc["profiles"]["ninerouter"]["model"].as_str(), Some("main"));
        assert_eq!(doc["model_context_window"].as_integer(), Some(200_000));
        assert_eq!(doc["model_max_output_tokens"].as_integer(), Some(32_000));
        let st = status_of(&doc, &NINE, PathBuf::new(), true);
        assert_eq!((st.model.as_deref(), st.context_window, st.max_output), (Some("main"), Some(200_000), Some(32_000)));
        assert!(!doc.to_string().contains("Add model ="), "the hint is gone");

        // Not the default: only its profile changes.
        apply(&mut doc, &OMNI, "http://127.0.0.1:20129/v1", false);
        choose_model(&mut doc, &OMNI, "gpx", Some(1), None).unwrap();
        assert_eq!(doc["profiles"]["omniroute"]["model"].as_str(), Some("gpx"));
        assert_eq!(doc["model"].as_str(), Some("main"));
        assert_eq!(doc["model_context_window"].as_integer(), Some(200_000));
        assert_eq!(status_of(&doc, &OMNI, PathBuf::new(), true).model.as_deref(), Some("gpx"));

        // Default through `profile`: the profile's model, plus the limits; unknown limits stay.
        set_default(&mut doc, &OMNI, true).unwrap();
        choose_model(&mut doc, &OMNI, "auto/best-coding", Some(1_000_000), None).unwrap();
        assert_eq!(doc["model"].as_str(), Some("main"), "top-level model is not what Codex reads now");
        assert_eq!(doc["model_context_window"].as_integer(), Some(1_000_000));
        assert_eq!(doc["model_max_output_tokens"].as_integer(), Some(32_000));
        let reparsed: DocumentMut = doc.to_string().parse().unwrap();
        assert_eq!(reparsed["profiles"]["omniroute"]["model"].as_str(), Some("auto/best-coding"));

        assert!(choose_model(&mut DocumentMut::new(), &NINE, "main", None, None).is_err(), "needs the profile");
        assert!(choose_model(&mut doc, &NINE, " ", None, None).is_err());
    }

    #[test]
    fn only_the_newest_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "model = \"x\"\n").unwrap();
        for t in ["20200101T000000Z", "20200102T000000Z", "20200103T000000Z", "20200104T000000Z"] {
            std::fs::write(dir.path().join(format!("config.toml.zenv-{t}.bak")), "experimental_bearer_token = \"secret\"").unwrap();
        }
        std::fs::write(dir.path().join("config.toml.mine.bak"), "kept").unwrap();
        write(&config, &"model = \"y\"\n".parse().unwrap()).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), 5, "{left:?}");
        assert!(left.contains(&"config.toml.mine.bak".to_string()), "the user's own backups stay");
        assert!(!left.iter().any(|n| n.contains("20200101") || n.contains("20200102")), "{left:?}");
    }

    #[test]
    fn works_on_an_empty_file() {
        let mut doc = DocumentMut::new();
        apply(&mut doc, &OMNI, "http://127.0.0.1:20128/v1", true);
        let st = status_of(&doc, &OMNI, PathBuf::new(), false);
        assert!(st.configured && st.is_default && st.top_level_provider.is_none());
        assert!(set_default(&mut DocumentMut::new(), &OMNI, true).is_err());
    }
}
