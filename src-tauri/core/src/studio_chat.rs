//! Studio's Claude Code chat: `claude` in print mode speaking stream-json
//! over pipes (no terminal), which Studio draws as a conversation.
//!
//! Only the native Claude Code program runs, in the app's project folder, with
//! fixed arguments. Studio sends typed messages (a prompt, an answer to a
//! permission request, an interrupt); Zenv writes the JSON itself. Of what
//! `claude` prints, only what the chat shows reaches the window: text deltas,
//! whole messages, permission requests and the result. Tool results are cut
//! short and subagents' inner messages dropped, so long sessions stay light.
//!
//! Model, effort and permission mode are fixed when the program starts;
//! Studio changes them by starting it again on the same session. Pasted
//! images go in as images and text files as text; any other file is saved
//! under the Zenv home (a folder Claude Code may read) and named in the prompt.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::studio::{path_of, resolve, Agent};

/// Open chats across all Studio windows.
const MAX_CHATS: usize = 8;
/// A tool result longer than this is cut (the chat shows a preview only).
const MAX_RESULT: usize = 4000;
/// The end of `claude`'s error output kept for the exit message.
const STDERR_TAIL: usize = 2000;
const MAX_ATTACHMENTS: usize = 10;
/// The API's limit for one image.
const MAX_IMAGE: usize = 5 * 1024 * 1024;
const MAX_TEXT_FILE: usize = 256 * 1024;
const MAX_OTHER_FILE: usize = 20 * 1024 * 1024;
/// Saved attachments older than this are removed when a chat starts.
const ATTACHMENT_LIFETIME: Duration = Duration::from_secs(24 * 3600);

/// Claude Code's models and commands, as the composer's menus show them.
pub type Catalog = Value;

/// Each line is one JSON object for the chat.
pub type ChatSink = Arc<dyn Fn(String) + Send + Sync>;

/// Permission requests waiting for an answer: request id → (tool input,
/// what Claude Code suggests for "always allow").
pub(crate) type Pending = Arc<Mutex<HashMap<String, (Value, Value)>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    fn arg(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Manual,
    AcceptEdits,
    Auto,
    Plan,
    DontAsk,
    BypassPermissions,
}

impl PermissionMode {
    fn arg(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Auto => "auto",
            PermissionMode::Plan => "plan",
            PermissionMode::DontAsk => "dontAsk",
            PermissionMode::BypassPermissions => "bypassPermissions",
        }
    }
}

/// How Claude Code starts; None keeps the user's own Claude Code settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOptions {
    /// An alias (`opus`, `sonnet[1m]`, ...) or a full model name.
    pub model: Option<String>,
    pub effort: Option<Effort>,
    pub mode: Option<PermissionMode>,
    pub thinking: Option<bool>,
    pub fast: Option<bool>,
    /// One of the output styles Claude Code lists (Concise, Explanatory, ...).
    pub output_style: Option<String>,
    /// Switch to another model when a message is flagged.
    pub switch_on_flag: Option<bool>,
}

impl ChatOptions {
    /// The settings Claude Code takes on top of the user's own (`--settings`).
    fn settings(&self) -> AppResult<Option<Value>> {
        let mut s = Map::new();
        if let Some(v) = self.thinking {
            s.insert("alwaysThinkingEnabled".into(), v.into());
        }
        if let Some(v) = self.fast {
            s.insert("fastMode".into(), v.into());
        }
        if let Some(v) = self.switch_on_flag {
            s.insert("switchModelsOnFlag".into(), v.into());
        }
        if let Some(style) = self.output_style.as_deref() {
            let ok = (1..=60).contains(&style.len()) && style.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
            if !ok {
                return Err(AppError::Validation(format!("{style} is not an output style")));
            }
            s.insert("outputStyle".into(), style.into());
        }
        Ok((!s.is_empty()).then_some(Value::Object(s)))
    }
}

/// What Studio may ask a running Claude Code besides prompts.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatControl {
    McpStatus,
    #[serde(rename_all = "camelCase")]
    McpToggle { server: String, enabled: bool },
    #[serde(rename_all = "camelCase")]
    McpReconnect { server: String },
}

impl ChatControl {
    fn request(&self) -> Value {
        match self {
            ChatControl::McpStatus => json!({ "subtype": "mcp_status" }),
            ChatControl::McpToggle { server, enabled } => json!({ "subtype": "mcp_toggle", "serverName": server, "enabled": enabled }),
            ChatControl::McpReconnect { server } => json!({ "subtype": "mcp_reconnect", "serverName": server }),
        }
    }
}

/// A model name as Claude Code takes it: letters, digits, `-._[]`.
fn valid_model(m: &str) -> bool {
    (1..=80).contains(&m.len())
        && !m.starts_with('-')
        && m.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '[' | ']'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    /// PNG, JPEG, GIF or WebP; `data` is base64.
    Image,
    /// `data` is the file's text.
    Text,
    /// Anything else; `data` is base64, saved to a file Claude Code reads.
    File,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub name: String,
    pub media_type: String,
    pub data: String,
}

/// A file name safe to create: its last component, without characters
/// Windows refuses.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).trim_start_matches('.');
    if trimmed.is_empty() { "attachment".into() } else { trimmed.chars().take(120).collect() }
}

/// The prompt as Claude Code takes it: plain text, or blocks when files come with it.
fn prompt_content(text: &str, attachments: &[Attachment], dir: &Path) -> AppResult<Value> {
    if attachments.is_empty() {
        if text.trim().is_empty() {
            return Err(AppError::Validation("the message is empty".into()));
        }
        return Ok(Value::String(text.to_string()));
    }
    if attachments.len() > MAX_ATTACHMENTS {
        return Err(AppError::Validation(format!("at most {MAX_ATTACHMENTS} files per message")));
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut blocks = Vec::new();
    let mut saved = Vec::new();
    for a in attachments {
        let name = safe_name(&a.name);
        match a.kind {
            AttachmentKind::Image => {
                let media = a.media_type.to_ascii_lowercase();
                if !matches!(media.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
                    return Err(AppError::Validation(format!("{name}: only PNG, JPEG, GIF and WebP images can be sent")));
                }
                if a.data.len() / 4 * 3 > MAX_IMAGE {
                    return Err(AppError::Validation(format!("{name} is larger than 5 MB")));
                }
                blocks.push(json!({ "type": "image", "source": { "type": "base64", "media_type": media, "data": a.data } }));
            }
            AttachmentKind::Text => {
                if a.data.len() > MAX_TEXT_FILE {
                    return Err(AppError::Validation(format!("{name} is larger than 256 KB")));
                }
                blocks.push(json!({ "type": "text", "text": format!("<file name=\"{name}\">\n{}\n</file>", a.data) }));
            }
            AttachmentKind::File => {
                if a.data.len() / 4 * 3 > MAX_OTHER_FILE {
                    return Err(AppError::Validation(format!("{name} is larger than 20 MB")));
                }
                let bytes = b64.decode(a.data.as_bytes()).map_err(|_| AppError::Validation(format!("{name}: bad file data")))?;
                let folder = dir.join(uuid::Uuid::new_v4().to_string());
                let path = folder.join(&name);
                std::fs::create_dir_all(&folder)
                    .and_then(|_| std::fs::write(&path, bytes))
                    .map_err(|e| AppError::Platform(format!("cannot save {name}: {e}")))?;
                saved.push(path);
            }
        }
    }
    let mut prompt = text.trim().to_string();
    if !saved.is_empty() {
        let list: Vec<String> = saved.iter().map(|p| format!("- {}", p.display())).collect();
        prompt = format!("{prompt}\n\nAttached files (read them from these paths):\n{}", list.join("\n")).trim().to_string();
    }
    if !prompt.is_empty() {
        blocks.push(json!({ "type": "text", "text": prompt }));
    }
    Ok(Value::Array(blocks))
}

/// Of Claude Code's `initialize` answer, what the composer's menus show.
fn catalog_of(answer: &Value) -> Value {
    let pick = |v: &Value, keys: &[&str]| -> Value {
        Value::Object(keys.iter().filter_map(|k| Some((k.to_string(), v.get(*k)?.clone()))).collect())
    };
    let list = |key: &str, keys: &[&str]| -> Value {
        Value::Array(answer.get(key).and_then(Value::as_array).map(|a| a.iter().map(|v| pick(v, keys)).collect()).unwrap_or_default())
    };
    json!({
        "models": list("models", &["value", "resolvedModel", "displayName", "description", "supportedEffortLevels", "supportsFastMode"]),
        "commands": list("commands", &["name", "description", "argumentHint"]),
        "account": answer.get("account").map(|a| pick(a, &["email", "organization", "subscriptionType"])),
        "outputStyles": answer.get("available_output_styles"),
        "outputStyle": answer.get("output_style"),
        "version": answer.get("claude_code_version"),
    })
}

/// Removes saved attachments past their lifetime.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| now.duration_since(t).unwrap_or_default() > ATTACHMENT_LIFETIME);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

struct Chat {
    /// The window that opened it; only that window may use or close it.
    owner: String,
    stdin: ChildStdin,
    child: Arc<Mutex<Child>>,
    pending: Pending,
}

#[derive(Default)]
pub struct Chats {
    chats: Arc<Mutex<HashMap<String, Chat>>>,
}

impl Chats {
    fn with<T>(&self, owner: &str, id: &str, f: impl FnOnce(&mut Chat) -> T) -> AppResult<T> {
        let mut chats = self.chats.lock().unwrap();
        match chats.get_mut(id) {
            Some(c) if c.owner == owner => Ok(f(c)),
            _ => Err(AppError::NotFound { entity: "chat", id: id.to_string() }),
        }
    }

    fn write(&self, owner: &str, id: &str, message: &Value) -> AppResult<()> {
        let mut line = message.to_string();
        line.push('\n');
        self.with(owner, id, |c| c.stdin.write_all(line.as_bytes()).and_then(|_| c.stdin.flush()))?
            .map_err(|e| AppError::Platform(format!("Claude Code input: {e}")))
    }

    /// Allows or denies the tool use Claude Code asked about; `always` also
    /// takes Claude Code's suggestion (a rule, or a mode for the session).
    pub fn answer(&self, owner: &str, id: &str, request_id: &str, allow: bool, always: bool) -> AppResult<()> {
        let (input, suggestions) = self
            .with(owner, id, |c| c.pending.lock().unwrap().remove(request_id))?
            .ok_or_else(|| AppError::NotFound { entity: "permission request", id: request_id.to_string() })?;
        let rules = (allow && always).then_some(suggestions);
        self.write(owner, id, &permission_answer(request_id, allow, input, rules))
    }

    /// Sends a control request; its answer comes back on the chat's lines as
    /// a `control_response` with the returned request id.
    pub fn control(&self, owner: &str, id: &str, control: &ChatControl) -> AppResult<String> {
        let request_id = format!("zenv-{}", uuid::Uuid::new_v4());
        self.write(owner, id, &json!({ "type": "control_request", "request_id": request_id, "request": control.request() }))?;
        Ok(request_id)
    }

    /// Stops what Claude Code is doing; the session stays open.
    pub fn interrupt(&self, owner: &str, id: &str) -> AppResult<()> {
        let request = json!({
            "type": "control_request",
            "request_id": uuid::Uuid::new_v4().to_string(),
            "request": { "subtype": "interrupt" },
        });
        self.write(owner, id, &request)
    }

    /// Ends the chat's program (the chat goes once it has exited).
    pub fn close(&self, owner: &str, id: &str) -> AppResult<()> {
        let child = self.with(owner, id, |c| c.child.clone())?;
        kill_tree(&child);
        Ok(())
    }

    /// A Studio window closed: end its chats.
    pub fn close_owned_by(&self, owner: &str) {
        let children: Vec<_> = self.chats.lock().unwrap().values().filter(|c| c.owner == owner).map(|c| c.child.clone()).collect();
        children.iter().for_each(kill_tree);
    }

    pub fn close_all(&self) {
        let children: Vec<_> = self.chats.lock().unwrap().values().map(|c| c.child.clone()).collect();
        children.iter().for_each(kill_tree);
    }
}

/// Claude Code runs tools (shells, servers) as its own children: end them too.
fn kill_tree(child: &Arc<Mutex<Child>>) {
    let mut child = child.lock().unwrap();
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &child.id().to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null());
        crate::platform::hide_console_window(&mut cmd);
        if cmd.status().is_ok_and(|s| s.success()) {
            return;
        }
    }
    let _ = child.kill();
}

fn user_message(content: Value) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": content } })
}

fn permission_answer(request_id: &str, allow: bool, input: Value, rules: Option<Value>) -> Value {
    let decision = if allow {
        let mut d = json!({ "behavior": "allow", "updatedInput": input });
        if let Some(rules) = rules.filter(|r| r.as_array().is_some_and(|a| !a.is_empty())) {
            d["updatedPermissions"] = rules;
        }
        d
    } else {
        json!({ "behavior": "deny", "message": "The user denied this in Zenv Studio." })
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": decision },
    })
}

fn cut(text: &mut String) {
    if text.len() > MAX_RESULT {
        let mut end = MAX_RESULT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n…");
    }
}

/// A line from `claude` as the chat needs it, or None when the chat has no
/// use for it. Permission requests are remembered in `pending`.
pub(crate) fn shape(line: &str, pending: &Pending) -> Option<String> {
    let mut v: Value = serde_json::from_str(line).ok()?;
    let obj = v.as_object_mut()?;
    // A subagent's own steps: its parent Task call already shows it.
    if obj.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
        return None;
    }
    match obj.get("type")?.as_str()? {
        "stream_event" => {
            let event = obj.get("event")?;
            let keep = match event.get("type")?.as_str()? {
                "content_block_start" => true,
                "content_block_delta" => event.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta"),
                _ => false,
            };
            // Unchanged: no need to write it again.
            return keep.then(|| line.to_string());
        }
        "system" if obj.get("subtype").and_then(Value::as_str) == Some("init") => {
            let keep = ["type", "subtype", "session_id", "model", "cwd", "permissionMode", "slash_commands"];
            obj.retain(|k, _| keep.contains(&k.as_str()));
        }
        "assistant" => {
            if let Some(blocks) = obj.get_mut("message").and_then(|m| m.get_mut("content")).and_then(Value::as_array_mut) {
                // Thinking and its signature can be long; the chat only notes it happened.
                for b in blocks.iter_mut() {
                    if matches!(b.get("type").and_then(Value::as_str), Some("thinking" | "redacted_thinking")) {
                        *b = json!({ "type": "thinking" });
                    }
                }
            }
            if let Some(m) = obj.get_mut("message").and_then(Value::as_object_mut) {
                m.retain(|k, _| matches!(k.as_str(), "id" | "role" | "content" | "model"));
            }
        }
        "user" => {
            obj.remove("tool_use_result");
            if let Some(blocks) = obj.get_mut("message").and_then(|m| m.get_mut("content")).and_then(Value::as_array_mut) {
                for b in blocks.iter_mut().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                    shorten_result(b);
                }
            }
        }
        "control_request" => {
            let request = obj.get("request")?;
            if request.get("subtype").and_then(Value::as_str) == Some("can_use_tool") {
                let id = obj.get("request_id")?.as_str()?.to_string();
                let input = request.get("input").cloned().unwrap_or(Value::Object(Map::new()));
                let suggestions = request.get("permission_suggestions").cloned().unwrap_or(Value::Array(vec![]));
                pending.lock().unwrap().insert(id, (input, suggestions));
            }
        }
        "control_response" => {
            // MCP status: names and states only (a server's config may hold keys).
            if let Some(servers) = obj.get_mut("response").and_then(|r| r.get_mut("response")).and_then(|r| r.get_mut("mcpServers")).and_then(Value::as_array_mut) {
                for s in servers.iter_mut() {
                    let tools = s.get("tools").and_then(Value::as_array).map_or(0, Vec::len);
                    *s = json!({
                        "name": s.get("name"),
                        "status": s.get("status"),
                        "scope": s.get("scope"),
                        "error": s.get("error"),
                        "version": s.pointer("/serverInfo/version"),
                        "tools": tools,
                    });
                }
            }
        }
        "control_cancel_request" => {
            if let Some(id) = obj.get("request_id").and_then(Value::as_str) {
                pending.lock().unwrap().remove(id);
            }
        }
        _ => {}
    }
    Some(v.to_string())
}

/// A tool result's content as one short string (images dropped).
fn shorten_result(block: &mut Value) {
    let Some(content) = block.get_mut("content") else { return };
    let mut text = match content {
        Value::String(s) => std::mem::take(s),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    cut(&mut text);
    *content = Value::String(text);
}

impl Core {
    /// Where pasted files are saved for Claude Code to read.
    fn chat_attachments_dir(&self) -> PathBuf {
        self.paths.data_dir.join("studio").join("attachments")
    }

    /// A prompt from the user, with any files pasted into it.
    pub fn studio_chat_send(&self, owner: &str, id: &str, text: &str, attachments: &[Attachment]) -> AppResult<()> {
        let content = prompt_content(text, attachments, &self.chat_attachments_dir())?;
        self.chats.write(owner, id, &user_message(content))
    }

    /// `claude` in print mode speaking stream-json, in the app's project
    /// folder with Studio's PATH; true when it is the native program.
    fn claude_command(&self, site_id: &str) -> AppResult<(Command, bool)> {
        let site = self.sites.get(site_id)?;
        if !site.project_root.is_dir() {
            return Err(AppError::invalid_path(&site.project_root, "is not an existing folder"));
        }
        let env = self.studio_env(site_id)?;
        let launch = resolve(Agent::ClaudeCode, &path_of(&env)).ok_or_else(|| {
            AppError::Validation(format!("Claude Code is not installed. Install it with: {}", Agent::ClaudeCode.install_hint()))
        })?;
        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.args);
        cmd.args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"]);
        cmd.current_dir(&site.project_root);
        for (k, v) in &env {
            cmd.env(k, v);
        }
        // Zenv started from inside a Claude Code session must not pass that on.
        for k in ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT"] {
            cmd.env_remove(k);
        }
        crate::platform::hide_console_window(&mut cmd);
        tracing::info!(domain = %site.domain, program = %launch.program.display(), "studio chat");
        Ok((cmd, launch.direct))
    }

    /// The models (with their names and effort levels) and commands Claude
    /// Code offers: its answer to `initialize`, which costs no tokens.
    pub fn studio_chat_catalog(&self, site_id: &str) -> AppResult<Catalog> {
        let (mut cmd, _) = self.claude_command(site_id)?;
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot start Claude Code: {e}")))?;
        let mut stdin = child.stdin.take().ok_or_else(|| AppError::Platform("Claude Code: no input pipe".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Platform("Claude Code: no output pipe".into()))?;
        let child = Arc::new(Mutex::new(child));
        let request = json!({ "type": "control_request", "request_id": "zenv-catalog", "request": { "subtype": "initialize" } });
        let _ = writeln!(stdin, "{request}").and_then(|_| stdin.flush());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("chat-catalog".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    if v.pointer("/response/request_id").and_then(Value::as_str) == Some("zenv-catalog") {
                        let _ = tx.send(v);
                        break;
                    }
                }
            })
            .map_err(|e| AppError::Platform(e.to_string()))?;
        let answer = rx.recv_timeout(Duration::from_secs(30));
        drop(stdin);
        kill_tree(&child);
        let answer = answer.map_err(|_| AppError::Platform("Claude Code did not list its models".into()))?;
        Ok(catalog_of(answer.pointer("/response/response").unwrap_or(&Value::Null)))
    }

    /// Starts Claude Code for a chat in the app's project folder; `resume`
    /// continues an earlier session of it.
    pub fn studio_open_chat(&self, owner: &str, site_id: &str, resume: Option<&str>, options: &ChatOptions, sink: ChatSink) -> AppResult<String> {
        if self.chats.chats.lock().unwrap().len() >= MAX_CHATS {
            return Err(AppError::Validation(format!("at most {MAX_CHATS} chats can be open; close one first")));
        }
        let (mut cmd, direct) = self.claude_command(site_id)?;
        cmd.args([
            "--permission-prompt-tool",
            "stdio",
            // Its questions need a form Studio doesn't have yet; Claude asks in text instead.
            "--disallowed-tools",
            "AskUserQuestion",
        ]);
        if let Some(id) = resume {
            let id = uuid::Uuid::parse_str(id).map_err(|_| AppError::Validation("not a Claude Code session id".into()))?;
            cmd.args(["--resume", &id.to_string()]);
        }
        if let Some(model) = options.model.as_deref() {
            if !valid_model(model) {
                return Err(AppError::Validation(format!("{model} is not a model name")));
            }
            cmd.args(["--model", model]);
        }
        if let Some(effort) = options.effort {
            cmd.args(["--effort", effort.arg()]);
        }
        if let Some(mode) = options.mode {
            cmd.args(["--permission-mode", mode.arg()]);
        }
        if let Some(settings) = options.settings()? {
            cmd.arg("--settings").arg(settings.to_string());
        }
        let attachments = self.chat_attachments_dir();
        if std::fs::create_dir_all(&attachments).is_ok() {
            prune(&attachments);
            cmd.arg("--add-dir").arg(&attachments);
        }
        if direct {
            if let Ok(context) = self.studio_context(site_id) {
                cmd.arg("--append-system-prompt").arg(context);
            }
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot start Claude Code: {e}")))?;
        let stdin = child.stdin.take().ok_or_else(|| AppError::Platform("Claude Code: no input pipe".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::Platform("Claude Code: no output pipe".into()))?;
        let mut stderr = child.stderr.take().ok_or_else(|| AppError::Platform("Claude Code: no error pipe".into()))?;
        let child = Arc::new(Mutex::new(child));
        let pending: Pending = Arc::default();
        let id = uuid::Uuid::new_v4().to_string();
        self.chats.chats.lock().unwrap().insert(
            id.clone(),
            Chat { owner: owner.to_string(), stdin, child: child.clone(), pending: pending.clone() },
        );

        let tail = Arc::new(Mutex::new(String::new()));
        let tail_w = tail.clone();
        std::thread::Builder::new()
            .name("chat-stderr".into())
            .spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut t = tail_w.lock().unwrap();
                    t.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if t.len() > STDERR_TAIL * 2 {
                        let mut from = t.len() - STDERR_TAIL;
                        while !t.is_char_boundary(from) {
                            from += 1;
                        }
                        t.drain(..from);
                    }
                }
            })
            .map_err(|e| AppError::Platform(e.to_string()))?;

        let chats = self.chats.chats.clone();
        let ended = id.clone();
        std::thread::Builder::new()
            .name("chat-read".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Some(out) = shape(&line, &pending) {
                        sink(out);
                    }
                }
                // Output ends when the program does. Polled, so `close` can still take the child.
                let code = loop {
                    match child.lock().unwrap().try_wait() {
                        Ok(Some(status)) => break status.code(),
                        Ok(None) => {}
                        Err(_) => break None,
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                };
                chats.lock().unwrap().remove(&ended);
                std::thread::sleep(std::time::Duration::from_millis(100));
                let stderr = tail.lock().unwrap().trim().to_string();
                sink(json!({ "type": "zenv_exit", "code": code, "stderr": stderr }).to_string());
            })
            .map_err(|e| AppError::Platform(e.to_string()))?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(line: &Value) -> Option<Value> {
        shape(&line.to_string(), &Pending::default()).map(|s| serde_json::from_str(&s).unwrap())
    }

    #[test]
    fn keeps_text_deltas_and_drops_other_stream_events() {
        let text = json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}},"parent_tool_use_id":null});
        assert_eq!(shaped(&text), Some(text));
        let input = json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{"}},"parent_tool_use_id":null});
        assert_eq!(shaped(&input), None);
        assert_eq!(shaped(&json!({"type":"stream_event","event":{"type":"message_stop"}})), None);
    }

    #[test]
    fn drops_subagent_steps() {
        let inner = json!({"type":"assistant","message":{"content":[]},"parent_tool_use_id":"toolu_1"});
        assert_eq!(shaped(&inner), None);
    }

    #[test]
    fn cuts_tool_results_and_drops_their_duplicate() {
        let long = "x".repeat(MAX_RESULT * 3);
        let line = json!({"type":"user","message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":long},{"type":"image"}]}
        ]},"tool_use_result":{"stdout":"big"}});
        let out = shaped(&line).unwrap();
        assert!(out.get("tool_use_result").is_none());
        let content = out.pointer("/message/content/0/content").unwrap().as_str().unwrap();
        assert!(content.len() <= MAX_RESULT + 4 && content.ends_with('…'));
    }

    #[test]
    fn hides_thinking_text() {
        let line = json!({"type":"assistant","message":{"id":"m","usage":{},"content":[{"type":"thinking","thinking":"secret","signature":"s"},{"type":"text","text":"hi"}]}});
        let out = shaped(&line).unwrap();
        assert_eq!(out.pointer("/message/content/0"), Some(&json!({"type":"thinking"})));
        assert!(out.pointer("/message/usage").is_none());
    }

    #[test]
    fn remembers_permission_requests_and_answers_with_their_input() {
        let pending = Pending::default();
        let suggestion = json!([{"type":"setMode","mode":"acceptEdits","destination":"session"}]);
        let line = json!({"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"a.txt"},"permission_suggestions":suggestion}});
        assert!(shape(&line.to_string(), &pending).is_some());
        let (input, suggestions) = pending.lock().unwrap().remove("r1").unwrap();
        let once = permission_answer("r1", true, input.clone(), None);
        assert_eq!(once.pointer("/response/request_id"), Some(&json!("r1")));
        assert_eq!(once.pointer("/response/response/updatedInput/file_path"), Some(&json!("a.txt")));
        assert!(once.pointer("/response/response/updatedPermissions").is_none());
        let always = permission_answer("r1", true, input, Some(suggestions));
        assert_eq!(always.pointer("/response/response/updatedPermissions"), Some(&suggestion));
        assert_eq!(permission_answer("r1", false, json!({}), None).pointer("/response/response/behavior"), Some(&json!("deny")));
    }

    #[test]
    fn a_cancelled_request_is_forgotten() {
        let pending = Pending::default();
        pending.lock().unwrap().insert("r1".into(), (json!({}), json!([])));
        shape(&json!({"type":"control_cancel_request","request_id":"r1"}).to_string(), &pending);
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn plain_prompts_stay_text() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(prompt_content("hi", &[], dir.path()).unwrap(), json!("hi"));
        assert!(prompt_content("  ", &[], dir.path()).is_err());
        assert_eq!(user_message(json!("hi")), json!({"type":"user","message":{"role":"user","content":"hi"}}));
    }

    #[test]
    fn attachments_become_blocks_and_saved_files() {
        let dir = tempfile::tempdir().unwrap();
        let b64 = base64::engine::general_purpose::STANDARD;
        let files = [
            Attachment { kind: AttachmentKind::Image, name: "shot.png".into(), media_type: "image/png".into(), data: b64.encode([1u8, 2, 3]) },
            Attachment { kind: AttachmentKind::Text, name: "notes.md".into(), media_type: "text/markdown".into(), data: "# hi".into() },
            Attachment { kind: AttachmentKind::File, name: r"..\..\evil<1>.pdf".into(), media_type: "application/pdf".into(), data: b64.encode(b"%PDF") },
        ];
        let content = prompt_content("look", &files, dir.path()).unwrap();
        let blocks = content.as_array().unwrap();
        assert_eq!(blocks[0].pointer("/source/media_type"), Some(&json!("image/png")));
        assert!(blocks[1]["text"].as_str().unwrap().starts_with("<file name=\"notes.md\">"));
        let prompt = blocks[2]["text"].as_str().unwrap();
        assert!(prompt.starts_with("look"));
        // Saved inside the attachments folder, under a cleaned name.
        let saved = prompt.lines().last().unwrap().trim_start_matches("- ");
        let saved = Path::new(saved);
        assert!(saved.starts_with(dir.path()));
        assert_eq!(saved.file_name().unwrap(), "evil_1_.pdf");
        assert_eq!(std::fs::read(saved).unwrap(), b"%PDF");
    }

    #[test]
    fn refuses_unsupported_or_oversized_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let svg = Attachment { kind: AttachmentKind::Image, name: "a.svg".into(), media_type: "image/svg+xml".into(), data: "PHN2Zz4=".into() };
        assert!(prompt_content("", &[svg], dir.path()).is_err());
        let big = Attachment { kind: AttachmentKind::Text, name: "big.log".into(), media_type: "text/plain".into(), data: "x".repeat(MAX_TEXT_FILE + 1) };
        assert!(prompt_content("", &[big], dir.path()).is_err());
    }

    #[test]
    fn model_names_cannot_smuggle_arguments() {
        assert!(valid_model("opus"));
        assert!(valid_model("claude-opus-5-5[1m]"));
        assert!(!valid_model("--dangerously-skip-permissions"));
        assert!(!valid_model("opus extra"));
        assert!(!valid_model(""));
    }

    #[test]
    fn the_catalog_keeps_what_the_menus_show() {
        let answer = json!({
            "models": [{"value":"opus","resolvedModel":"claude-opus-5-5","displayName":"Opus 5.5","description":"d","supportsFastMode":true,"supportedEffortLevels":["low","max"]}],
            "commands": [{"name":"compact","description":"Clear history but keep a summary","argumentHint":"<instructions>","extra":1}],
            "account": {"email":"someone@example.com","subscriptionType":"max","apiProvider":"firstParty","token":"secret"},
            "available_output_styles": ["default","Concise"],
            "claude_code_version": "2.1.317",
        });
        let c = catalog_of(&answer);
        assert_eq!(c["models"][0], json!({"value":"opus","resolvedModel":"claude-opus-5-5","displayName":"Opus 5.5","description":"d","supportedEffortLevels":["low","max"],"supportsFastMode":true}));
        assert_eq!(c["commands"][0], json!({"name":"compact","description":"Clear history but keep a summary","argumentHint":"<instructions>"}));
        assert_eq!(c["account"], json!({"email":"someone@example.com","subscriptionType":"max"}));
        assert_eq!(c["outputStyles"], json!(["default","Concise"]));
        assert_eq!(c["version"], json!("2.1.317"));
    }

    #[test]
    fn options_become_settings_and_refuse_odd_styles() {
        let o = ChatOptions { thinking: Some(false), fast: Some(true), output_style: Some("Concise".into()), ..Default::default() };
        assert_eq!(o.settings().unwrap(), Some(json!({"alwaysThinkingEnabled":false,"fastMode":true,"outputStyle":"Concise"})));
        assert_eq!(ChatOptions::default().settings().unwrap(), None);
        let bad = ChatOptions { output_style: Some("x\"}".into()), ..Default::default() };
        assert!(bad.settings().is_err());
    }

    #[test]
    fn controls_are_typed_requests() {
        let t: ChatControl = serde_json::from_value(json!({"kind":"mcpToggle","server":"docs","enabled":false})).unwrap();
        assert_eq!(t.request(), json!({"subtype":"mcp_toggle","serverName":"docs","enabled":false}));
        assert!(serde_json::from_value::<ChatControl>(json!({"kind":"setCwd"})).is_err());
    }

    #[test]
    fn mcp_status_keeps_names_and_states_only() {
        let line = json!({"type":"control_response","response":{"subtype":"success","request_id":"zenv-1","response":{"mcpServers":[
            {"name":"db","status":"failed","error":"boom","config":{"env":{"KEY":"secret"}},"serverInfo":{"version":"1.0"},"tools":[{"name":"a"},{"name":"b"}]}
        ]}}});
        let out = shaped(&line).unwrap();
        let s = out.pointer("/response/response/mcpServers/0").unwrap();
        assert_eq!(s, &json!({"name":"db","status":"failed","scope":null,"error":"boom","version":"1.0","tools":2}));
        assert!(!out.to_string().contains("secret"));
    }

    #[test]
    fn options_come_in_as_studio_sends_them() {
        let o: ChatOptions = serde_json::from_value(json!({"model":"sonnet[1m]","effort":"xhigh","mode":"acceptEdits"})).unwrap();
        assert_eq!((o.effort.unwrap().arg(), o.mode.unwrap().arg()), ("xhigh", "acceptEdits"));
        assert!(serde_json::from_value::<ChatOptions>(json!({"mode":"yolo"})).is_err());
    }
}
