//! Claude Code's saved sessions for an app's project folder, so Studio's chat
//! can list them and continue one (`--resume`), whether it began in Studio,
//! a terminal or an editor.
//!
//! Claude Code keeps each session as JSON lines in
//! `~/.claude/projects/<folder with every non-alphanumeric character as ->/<id>.jsonl`.
//! Zenv only reads them. Listing reads each file's last 64 KB (where its
//! current title is) or, failing that, its first 64 KB; a transcript follows
//! the session's current branch back from its last message.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

const SCAN: u64 = 64 * 1024;
const MAX_SESSIONS: usize = 60;
/// A long session shows its latest lines only.
const MAX_LINES: usize = 600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    /// Last change, milliseconds since 1970.
    pub modified: u64,
}

fn claude_home() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude")))
}

/// The folder Claude Code keeps `project`'s sessions in, if it has any.
fn sessions_dir(projects: &Path, project: &Path) -> Option<PathBuf> {
    let name: String = project.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let name = name.trim_end_matches('-');
    // Windows paths differ in case only (d:\ vs D:\) from one tool to another.
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn read_range(path: &Path, from_end: bool) -> String {
    let Ok(mut f) = File::open(path) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if from_end { len.saturating_sub(SCAN) } else { 0 };
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::with_capacity(SCAN.min(len) as usize);
    let _ = f.take(SCAN).read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// What a user's prompt said, or None for Claude Code's own notes
/// (command output, caveats, reminders).
fn prompt_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                return None;
            }
            blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("<command-name>") {
        let name = rest.split("</command-name>").next().unwrap_or("").trim();
        let args = t.split("<command-args>").nth(1).and_then(|a| a.split("</command-args>").next()).unwrap_or("").trim();
        return Some(format!("{name} {args}").trim().to_string());
    }
    if t.is_empty() || t.starts_with("<local-command") || t.starts_with("<system-reminder>") || t.starts_with("Caveat:") {
        return None;
    }
    Some(t.to_string())
}

/// A session's title: its latest AI title, else its last or first prompt.
fn title_of(path: &Path) -> Option<String> {
    let tail = read_range(path, true);
    let mut title = None;
    let mut last_prompt = None;
    for line in tail.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        match v.get("type").and_then(Value::as_str) {
            Some("ai-title") | Some("custom-title") => {
                title = v.get("aiTitle").or_else(|| v.get("customTitle")).and_then(Value::as_str).map(str::to_string).or(title)
            }
            Some("summary") => title = v.get("summary").and_then(Value::as_str).map(str::to_string).or(title),
            Some("last-prompt") => last_prompt = v.get("lastPrompt").and_then(Value::as_str).map(str::to_string),
            _ => {}
        }
    }
    title.or(last_prompt).or_else(|| {
        read_range(path, false).lines().find_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            if v.get("type")?.as_str()? != "user" || v.get("isMeta").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            prompt_text(v.pointer("/message/content")?)
        })
    })
}

/// The project's sessions, newest first.
pub fn sessions(project: &Path) -> Vec<SessionSummary> {
    let Some(dir) = claude_home().and_then(|h| sessions_dir(&h.join("projects"), project)) else { return vec![] };
    sessions_in(&dir)
}

fn sessions_in(dir: &Path) -> Vec<SessionSummary> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<(PathBuf, u64)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some((e.path(), modified))
        })
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
        .into_iter()
        .filter_map(|(path, modified)| {
            let id = path.file_stem()?.to_str()?.to_string();
            uuid::Uuid::parse_str(&id).ok()?;
            let title: String = title_of(&path)?.chars().take(160).collect();
            Some(SessionSummary { id, title, modified })
        })
        .take(MAX_SESSIONS)
        .collect()
}

/// A session's messages on its current branch, as the chat's lines.
pub fn transcript(project: &Path, id: &str) -> AppResult<Vec<String>> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| AppError::Validation("not a Claude Code session id".into()))?;
    let dir = claude_home()
        .and_then(|h| sessions_dir(&h.join("projects"), project))
        .ok_or_else(|| AppError::NotFound { entity: "Claude Code session", id: id.to_string() })?;
    let path = dir.join(format!("{id}.jsonl"));
    let text = std::fs::read_to_string(&path).map_err(|_| AppError::NotFound { entity: "Claude Code session", id: id.to_string() })?;
    Ok(transcript_lines(&text))
}

fn transcript_lines(text: &str) -> Vec<String> {
    // Every linked entry (attachments and notes sit in the chain too), not a subagent's.
    let entries: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("uuid").is_some_and(Value::is_string))
        .filter(|v| v.get("isSidechain").and_then(Value::as_bool) != Some(true))
        .collect();
    let by_uuid: HashMap<&str, usize> = entries.iter().enumerate().filter_map(|(i, v)| Some((v.get("uuid")?.as_str()?, i))).collect();
    let is_message = |v: &Value| matches!(v.get("type").and_then(Value::as_str), Some("user" | "assistant"));

    // From the last message back along its parents: the branch Claude Code resumes.
    let mut chain = Vec::new();
    let mut at = entries.iter().rposition(is_message);
    while let Some(i) = at {
        chain.push(i);
        if chain.len() > entries.len() {
            break;
        }
        let v = &entries[i];
        let parent = v.get("parentUuid").and_then(Value::as_str).or_else(|| v.get("logicalParentUuid").and_then(Value::as_str));
        at = parent.and_then(|p| by_uuid.get(p).copied());
    }
    chain.reverse();

    let pending = Default::default();
    let mut out = Vec::new();
    for i in chain {
        let v = &entries[i];
        if !is_message(v) || v.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(content) = v.pointer("/message/content") else { continue };
        if v.get("type").and_then(Value::as_str) == Some("user") {
            if let Some(text) = prompt_text(content) {
                let images = content.as_array().map_or(0, |b| b.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("image")).count());
                out.push(json!({ "type": "zenv_prompt", "text": text, "images": images }).to_string());
                continue;
            }
        }
        let line = json!({ "type": v["type"], "message": v["message"] }).to_string();
        if let Some(shaped) = crate::studio_chat::shape(&line, &pending) {
            out.push(shaped);
        }
    }
    if out.len() > MAX_LINES {
        let cut = out.len() - MAX_LINES;
        out.drain(..cut);
        out.insert(0, json!({ "type": "zenv_truncated", "count": cut }).to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, uuid: &str, parent: Option<&str>, content: Value) -> String {
        json!({ "type": kind, "uuid": uuid, "parentUuid": parent, "isSidechain": false, "message": { "role": kind, "content": content } }).to_string()
    }

    #[test]
    fn finds_the_project_folder_whatever_the_drive_letter_case() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("d--Hanif-htdocs-app")).unwrap();
        let found = sessions_dir(home.path(), Path::new(r"D:\Hanif\htdocs\app")).unwrap();
        assert!(found.ends_with("d--Hanif-htdocs-app"));
        assert!(sessions_dir(home.path(), Path::new(r"D:\other")).is_none());
    }

    #[test]
    fn lists_sessions_by_title_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let a = "11111111-1111-1111-1111-111111111111";
        let b = "22222222-2222-2222-2222-222222222222";
        std::fs::write(
            dir.path().join(format!("{a}.jsonl")),
            [entry("user", "u1", None, json!("fix the login page")), json!({"type":"ai-title","aiTitle":"Old title"}).to_string(), json!({"type":"ai-title","aiTitle":"Login fix"}).to_string()].join("\n"),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join(format!("{b}.jsonl")), entry("user", "u1", None, json!([{"type":"text","text":"add dark mode"}]))).unwrap();
        // Not a session: no prompt at all, or not named by an id.
        std::fs::write(dir.path().join("33333333-3333-3333-3333-333333333333.jsonl"), json!({"type":"mode","mode":"normal"}).to_string()).unwrap();
        std::fs::write(dir.path().join("notes.jsonl"), entry("user", "u1", None, json!("x"))).unwrap();
        let list = sessions_in(dir.path());
        let titles: Vec<_> = list.iter().map(|s| (s.id.as_str(), s.title.as_str())).collect();
        assert_eq!(titles, [(b, "add dark mode"), (a, "Login fix")]);
    }

    #[test]
    fn follows_the_current_branch_and_shows_prompts_and_tools() {
        let lines = [
            entry("user", "u1", None, json!("<command-name>/clear</command-name>\n<command-args></command-args>")),
            entry("user", "u2", Some("u1"), json!("make a button")),
            entry("assistant", "a1", Some("u2"), json!([{"type":"text","text":"First try"}])),
            // Rewound: a second answer to the same prompt replaces the first.
            entry("assistant", "a2", Some("u2"), json!([{"type":"tool_use","id":"t1","name":"Write","input":{"file_path":"b.html"}}])),
            entry("user", "u3", Some("a2"), json!([{"type":"tool_result","tool_use_id":"t1","content":"ok"}])),
            json!({"type":"user","uuid":"m1","parentUuid":"u3","isMeta":true,"message":{"role":"user","content":"Caveat: noise"}}).to_string(),
            // Attachments and notes are links in the chain too.
            json!({"type":"attachment","uuid":"x1","parentUuid":"m1","attachment":{}}).to_string(),
            entry("assistant", "a3", Some("x1"), json!([{"type":"text","text":"Done"}])),
            json!({"type":"ai-title","aiTitle":"Button"}).to_string(),
            json!({"type":"assistant","uuid":"s1","parentUuid":"a3","isSidechain":true,"message":{"content":[]}}).to_string(),
        ]
        .join("\n");
        let out: Vec<Value> = transcript_lines(&lines).iter().map(|l| serde_json::from_str(l).unwrap()).collect();
        let kinds: Vec<&str> = out.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["zenv_prompt", "zenv_prompt", "assistant", "user", "assistant"]);
        assert_eq!(out[0]["text"], "/clear");
        assert_eq!(out[1]["text"], "make a button");
        assert_eq!(out[2].pointer("/message/content/0/name"), Some(&json!("Write")));
        assert_eq!(out[4].pointer("/message/content/0/text"), Some(&json!("Done")));
    }

    /// `ZENV_HISTORY_PROJECT=<folder> cargo test -p zenv-core reads_real_sessions -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_real_sessions() {
        let project = PathBuf::from(std::env::var("ZENV_HISTORY_PROJECT").expect("ZENV_HISTORY_PROJECT"));
        let started = std::time::Instant::now();
        let list = sessions(&project);
        println!("{} sessions listed in {:?}", list.len(), started.elapsed());
        for s in list.iter().take(5) {
            println!("  {} {}", s.id, s.title);
        }
        let newest = list.first().expect("a session");
        let started = std::time::Instant::now();
        let lines = transcript(&project, &newest.id).unwrap();
        let bytes: usize = lines.iter().map(String::len).sum();
        println!("newest: {} lines, {} KB, read in {:?}", lines.len(), bytes / 1024, started.elapsed());
        assert!(lines.iter().any(|l| l.contains("zenv_prompt")));
    }

    #[test]
    fn a_long_session_keeps_its_latest_lines() {
        let mut lines = Vec::new();
        let mut parent: Option<String> = None;
        for i in 0..(MAX_LINES + 5) {
            let id = format!("u{i}");
            lines.push(entry("user", &id, parent.as_deref(), json!(format!("prompt {i}"))));
            parent = Some(id);
        }
        let out = transcript_lines(&lines.join("\n"));
        assert_eq!(out.len(), MAX_LINES + 1);
        assert!(out[0].contains("zenv_truncated"));
        assert!(out.last().unwrap().contains(&format!("prompt {}", MAX_LINES + 4)));
    }
}
