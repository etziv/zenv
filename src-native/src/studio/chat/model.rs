use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Pending,
    Allowed,
    Denied,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Permission {
    pub request_id: String,
    pub state: Answer,
    pub always: Option<String>,
    pub always_mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub result: Option<(String, bool)>,
    pub permission: Option<Permission>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    User { text: String, files: Vec<String> },
    Text { text: String, streaming: bool },
    Tool(Tool),
    Result { error: bool, text: String, seconds: Option<f64> },
    Notice { text: String, error: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: u64,
    pub item: Item,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limit {
    pub utilization: f64,
    pub resets_at: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Limits {
    pub five_hour: Option<Limit>,
    pub seven_day: Option<Limit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatState {
    pub items: Vec<Entry>,
    pub busy: bool,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub commands: Option<Vec<String>>,
    pub limits: Limits,
    pub streaming: Option<usize>,
    next: u64,
}

impl Default for ChatState {
    fn default() -> Self {
        Self { items: Vec::new(), busy: false, session_id: None, model: None, mode: None, commands: None, limits: Limits::default(), streaming: None, next: 1 }
    }
}

fn str_of(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn blocks(message: &Value) -> &[Value] {
    message.get("content").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn suggestion(raw: &Value) -> (Option<String>, Option<String>) {
    let (label, mode) = first_suggestion(raw);
    let changes: usize = raw.as_array().into_iter().flatten().map(|s| s["rules"].as_array().map_or(1, |r| r.len().max(1))).sum();
    let label = label.map(|l| if changes > 1 { format!("{l} and {} more", changes - 1) } else { l });
    (label, mode)
}

fn first_suggestion(raw: &Value) -> (Option<String>, Option<String>) {
    let first = raw.get(0).unwrap_or(&Value::Null);
    match first.get("type").and_then(Value::as_str) {
        Some("setMode") => {
            let mode = first.get("mode").and_then(Value::as_str);
            let label = if mode == Some("acceptEdits") { "Allow all edits this session".to_owned() } else { format!("Switch to {} mode", mode.unwrap_or("that")) };
            (Some(label), mode.map(str::to_owned))
        }
        Some("addRules") if first.get("rules").is_some_and(Value::is_array) => {
            let rule = &first["rules"][0];
            let tool = rule.get("toolName").and_then(Value::as_str).unwrap_or("this");
            let content = rule.get("ruleContent").and_then(Value::as_str).map(|c| format!(" {c}")).unwrap_or_default();
            (Some(format!("Always allow {tool}{content}")), None)
        }
        _ => (None, None),
    }
}

fn limit(v: &Value) -> Option<Limit> {
    Some(Limit { utilization: v.get("utilization")?.as_f64()?, resets_at: v.get("resetsAt")?.as_f64()? })
}

impl ChatState {
    fn push(&mut self, item: Item) -> usize {
        self.items.push(Entry { key: self.next, item });
        self.next += 1;
        self.items.len() - 1
    }

    fn tool_index(&self, id: &str) -> Option<usize> {
        self.items.iter().rposition(|e| matches!(&e.item, Item::Tool(t) if t.id == id))
    }

    fn tool_mut(&mut self, i: Option<usize>) -> Option<&mut Tool> {
        match &mut self.items.get_mut(i?)?.item {
            Item::Tool(t) => Some(t),
            _ => None,
        }
    }

    fn end_stream(&mut self, text: Option<&str>) {
        let Some(i) = self.streaming.take() else { return };
        if let Some(Entry { item: Item::Text { text: t, streaming }, .. }) = self.items.get_mut(i) {
            if let Some(final_text) = text {
                *t = final_text.to_owned();
            }
            *streaming = false;
        }
    }

    pub fn apply(&mut self, lines: &[Value]) {
        for line in lines {
            self.apply_line(line);
        }
    }

    fn apply_line(&mut self, line: &Value) {
        let field = |k: &str| line.get(k).unwrap_or(&Value::Null);
        match field("type").as_str().unwrap_or("") {
            "system" => {
                if field("subtype") == "init" {
                    self.session_id = str_of(field("session_id")).or(self.session_id.take());
                    self.model = str_of(field("model")).or(self.model.take());
                    if let Some(list) = field("slash_commands").as_array() {
                        self.commands = Some(list.iter().filter_map(str_of).collect());
                    }
                }
                self.mode = str_of(field("permissionMode")).or(self.mode.take());
            }
            "rate_limit_event" => {
                let windows = &field("rate_limit_info")["unifiedWindows"];
                self.limits = Limits { five_hour: limit(&windows["five_hour"]).or(self.limits.five_hour), seven_day: limit(&windows["seven_day"]).or(self.limits.seven_day) };
            }
            "stream_event" => {
                let event = field("event");
                match event["type"].as_str() {
                    Some("content_block_start") if event["content_block"]["type"] == "text" => {
                        self.end_stream(None);
                        self.streaming = Some(self.push(Item::Text { text: String::new(), streaming: true }));
                    }
                    Some("content_block_delta") => {
                        let delta = event["delta"]["text"].as_str().unwrap_or("");
                        if let Some(Entry { item: Item::Text { text, .. }, .. }) = self.streaming.and_then(|i| self.items.get_mut(i)) {
                            text.push_str(delta);
                        }
                    }
                    _ => {}
                }
            }
            "assistant" => {
                for b in blocks(field("message")) {
                    match b["type"].as_str() {
                        Some("text") if b["text"].as_str().is_some_and(|t| !t.is_empty()) => {
                            let text = b["text"].as_str().unwrap_or("");
                            if self.streaming.is_some() {
                                self.end_stream(Some(text));
                            } else {
                                self.push(Item::Text { text: text.to_owned(), streaming: false });
                            }
                        }
                        Some("tool_use") => {
                            let Some(id) = b["id"].as_str().filter(|id| self.tool_index(id).is_none()) else { continue };
                            let tool = Tool { id: id.to_owned(), name: b["name"].as_str().unwrap_or("Tool").to_owned(), input: object(&b["input"]), result: None, permission: None };
                            self.end_stream(None);
                            self.push(Item::Tool(tool));
                        }
                        _ => {}
                    }
                }
            }
            "user" => {
                for b in blocks(field("message")) {
                    let Some(id) = b["tool_use_id"].as_str().filter(|_| b["type"] == "tool_result") else { continue };
                    let i = self.tool_index(id);
                    if let Some(t) = self.tool_mut(i) {
                        t.result = Some((b["content"].as_str().unwrap_or("").to_owned(), b["is_error"] == true));
                    }
                }
            }
            "control_request" => {
                let req = field("request");
                let Some(request_id) = field("request_id").as_str().filter(|_| req["subtype"] == "can_use_tool") else { return };
                let tool_id = req["tool_use_id"].as_str().unwrap_or(request_id).to_owned();
                let (always, always_mode) = suggestion(&req["permission_suggestions"]);
                let permission = Permission { request_id: request_id.to_owned(), state: Answer::Pending, always, always_mode };
                let i = self.tool_index(&tool_id);
                match self.tool_mut(i) {
                    Some(t) => t.permission = Some(permission),
                    None => {
                        let name = req["tool_name"].as_str().unwrap_or("Tool").to_owned();
                        self.push(Item::Tool(Tool { id: tool_id, name, input: object(&req["input"]), result: None, permission: Some(permission) }));
                    }
                }
            }
            "control_cancel_request" => {
                let rid = field("request_id").as_str();
                for e in &mut self.items {
                    if let Item::Tool(t) = &mut e.item {
                        if t.permission.as_ref().map(|p| p.request_id.as_str()) == rid {
                            t.permission = None;
                        }
                    }
                }
            }
            "result" => {
                self.end_stream(None);
                self.busy = false;
                let error = field("is_error") == true || field("subtype") != "success";
                let text = if error { str_of(field("result")).unwrap_or_else(|| field("subtype").as_str().unwrap_or("error").to_owned()) } else { String::new() };
                let seconds = field("duration_ms").as_f64().map(|ms| (ms / 100.0).round() / 10.0);
                self.push(Item::Result { error, text, seconds });
                for e in &mut self.items {
                    if let Item::Tool(t) = &mut e.item {
                        if t.permission.as_ref().is_some_and(|p| p.state == Answer::Pending) {
                            t.permission = None;
                        }
                    }
                }
            }
            "zenv_prompt" => {
                let images = field("images").as_u64().unwrap_or(0);
                let files = (1..=images).map(|i| format!("image {i}")).collect();
                self.push(Item::User { text: str_of(field("text")).unwrap_or_default(), files });
            }
            "zenv_truncated" => {
                let count = field("count");
                self.push(Item::Notice { text: format!("{count} earlier messages not shown."), error: false });
            }
            "zenv_exit" => {
                self.end_stream(None);
                self.busy = false;
                let code = field("code").as_i64();
                if code != Some(0) {
                    let code = code.map(|c| format!(" (exit code {c})")).unwrap_or_default();
                    let stderr = field("stderr").as_str().filter(|s| !s.is_empty()).map(|s| format!("\n{s}")).unwrap_or_default();
                    self.push(Item::Notice { text: format!("Claude Code stopped{code}.{stderr}"), error: true });
                }
            }
            _ => {}
        }
    }

    pub fn history(session_id: &str, lines: &[Value]) -> Self {
        let mut s = Self { session_id: Some(session_id.to_owned()), ..Self::default() };
        s.apply(lines);
        s
    }

    pub fn sent(&mut self, text: &str, files: Vec<String>) {
        self.busy = true;
        self.push(Item::User { text: text.to_owned(), files });
    }

    pub fn answered(&mut self, request_id: &str, allow: bool, always: bool) {
        for e in &mut self.items {
            let Item::Tool(t) = &mut e.item else { continue };
            let Some(p) = t.permission.as_mut().filter(|p| p.request_id == request_id) else { continue };
            if allow && always {
                if let Some(m) = p.always_mode.clone() {
                    self.mode = Some(m);
                }
            }
            *p = Permission { request_id: request_id.to_owned(), state: if allow { Answer::Allowed } else { Answer::Denied }, always: None, always_mode: None };
        }
    }

    pub fn notice(&mut self, text: impl Into<String>, error: bool) {
        self.busy = false;
        self.push(Item::Notice { text: text.into(), error });
    }

    pub fn waiting(&self) -> bool {
        matches!(self.items.last(), Some(Entry { item: Item::Tool(Tool { permission: Some(Permission { state: Answer::Pending, .. }), .. }), .. }))
    }

    pub fn working(&self) -> bool {
        self.busy && self.streaming.is_none() && !self.waiting()
    }
}

fn object(v: &Value) -> Value {
    if v.is_object() { v.clone() } else { Value::Object(Default::default()) }
}

pub fn relative(root: &str, path: &str) -> Option<String> {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_owned();
    let (r, f) = (norm(root), norm(path));
    let prefix = format!("{}/", r.to_lowercase());
    if f.to_lowercase().starts_with(&prefix) {
        return Some(f[prefix.len()..].to_owned());
    }
    let bytes = f.as_bytes();
    let absolute = f.starts_with('/') || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/');
    (!absolute).then_some(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(s: &mut ChatState, lines: Value) {
        s.apply(lines.as_array().unwrap());
    }

    fn delta(text: &str) -> Value {
        json!({ "type": "stream_event", "event": { "type": "content_block_delta", "delta": { "type": "text_delta", "text": text } } })
    }

    fn last(s: &ChatState) -> &Item {
        &s.items.last().unwrap().item
    }

    #[test]
    fn streams_text_and_settles_it_with_the_whole_message() {
        let mut s = ChatState::default();
        s.sent("hi", vec![]);
        run(&mut s, json!([{ "type": "system", "subtype": "init", "session_id": "abc", "model": "claude-opus-5-5" }]));
        run(&mut s, json!([{ "type": "stream_event", "event": { "type": "content_block_start", "content_block": { "type": "text" } } }, delta("Hel"), delta("lo")]));
        assert_eq!(last(&s), &Item::Text { text: "Hello".into(), streaming: true });
        assert!(!s.working(), "streaming text is not 'Working…'");
        run(&mut s, json!([{ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Hello!" }] } }]));
        assert_eq!(last(&s), &Item::Text { text: "Hello!".into(), streaming: false });
        assert_eq!(s.streaming, None);
        assert_eq!(s.session_id.as_deref(), Some("abc"));
        assert_eq!(s.model.as_deref(), Some("claude-opus-5-5"));
        assert!(s.working());
        run(&mut s, json!([{ "type": "result", "subtype": "success", "is_error": false, "duration_ms": 3650 }]));
        assert!(!s.busy);
        assert_eq!(last(&s), &Item::Result { error: false, text: String::new(), seconds: Some(3.7) });
    }

    #[test]
    fn asks_permission_on_the_tools_card_and_fills_in_its_result() {
        let mut s = ChatState::default();
        s.sent("write", vec![]);
        run(&mut s, json!([{ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": "t1", "name": "Write", "input": { "file_path": "a.txt" } }] } }]));
        run(&mut s, json!([{ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": "t1", "name": "Write", "input": {} }] } }]));
        run(&mut s, json!([{ "type": "control_request", "request_id": "r1", "request": { "subtype": "can_use_tool", "tool_name": "Write", "tool_use_id": "t1", "input": {} } }]));
        assert_eq!(s.items.len(), 2);
        assert!(s.waiting() && !s.working());
        let Item::Tool(t) = &s.items[1].item else { panic!() };
        assert_eq!(t.permission.as_ref().map(|p| p.state), Some(Answer::Pending));
        assert_eq!(t.input["file_path"], "a.txt");
        s.answered("r1", true, false);
        let Item::Tool(t) = &s.items[1].item else { panic!() };
        assert_eq!(t.permission.as_ref().map(|p| p.state), Some(Answer::Allowed));
        run(&mut s, json!([{ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "ok", "is_error": false }] } }]));
        let Item::Tool(t) = &s.items[1].item else { panic!() };
        assert_eq!(t.result, Some(("ok".into(), false)));
    }

    #[test]
    fn a_cancelled_or_finished_request_drops_its_question() {
        let mut s = ChatState::default();
        let ask = |rid: &str, tool: &str| json!([{ "type": "control_request", "request_id": rid, "request": { "subtype": "can_use_tool", "tool_name": "Bash", "tool_use_id": tool, "input": { "command": "ls" } } }]);
        run(&mut s, ask("r1", "t1"));
        run(&mut s, json!([{ "type": "control_cancel_request", "request_id": "r1" }]));
        let Item::Tool(t) = &s.items[0].item else { panic!() };
        assert_eq!(t.permission, None);
        run(&mut s, ask("r2", "t2"));
        run(&mut s, json!([{ "type": "result", "subtype": "error_during_execution", "is_error": true }]));
        let Item::Tool(t) = &s.items[1].item else { panic!() };
        assert_eq!(t.permission, None);
        assert_eq!(last(&s), &Item::Result { error: true, text: "error_during_execution".into(), seconds: None });
    }

    #[test]
    fn reports_an_unexpected_exit_with_claude_codes_error_output() {
        let mut s = ChatState::default();
        s.sent("x", vec![]);
        run(&mut s, json!([{ "type": "zenv_exit", "code": 1, "stderr": "not logged in" }]));
        assert!(!s.busy);
        let Item::Notice { text, error } = last(&s) else { panic!() };
        assert!(*error && text.contains("not logged in") && text.contains("(exit code 1)"));
        let n = s.items.len();
        run(&mut s, json!([{ "type": "zenv_exit", "code": 0 }]));
        assert_eq!(s.items.len(), n, "a clean exit says nothing");
    }

    #[test]
    fn keeps_commands_mode_and_usage_limits() {
        let mut s = ChatState::default();
        run(&mut s, json!([{ "type": "system", "subtype": "init", "session_id": "a", "model": "m", "permissionMode": "default", "slash_commands": ["compact", 3, "init"] }]));
        assert_eq!(s.commands, Some(vec!["compact".into(), "init".into()]));
        assert_eq!(s.mode.as_deref(), Some("default"));
        run(&mut s, json!([{ "type": "rate_limit_event", "rate_limit_info": { "unifiedWindows": { "five_hour": { "utilization": 0.2, "resetsAt": 100 }, "seven_day": { "utilization": 0.84, "resetsAt": 200 } } } }]));
        assert_eq!(s.limits.five_hour, Some(Limit { utilization: 0.2, resets_at: 100.0 }));
        run(&mut s, json!([{ "type": "rate_limit_event", "rate_limit_info": { "unifiedWindows": { "five_hour": { "utilization": 0.3, "resetsAt": 100 } } } }]));
        assert_eq!(s.limits.seven_day.map(|l| l.utilization), Some(0.84), "a window not reported keeps its last value");
    }

    #[test]
    fn offers_claude_codes_suggestion_and_takes_its_mode_when_always_allowed() {
        let mut s = ChatState::default();
        run(&mut s, json!([{ "type": "control_request", "request_id": "r1", "request": { "subtype": "can_use_tool", "tool_name": "Write", "tool_use_id": "t1", "input": {}, "permission_suggestions": [{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }] } }]));
        let Item::Tool(t) = &s.items[0].item else { panic!() };
        let p = t.permission.as_ref().unwrap();
        assert_eq!((p.always.as_deref(), p.always_mode.as_deref()), (Some("Allow all edits this session"), Some("acceptEdits")));
        s.answered("r1", true, true);
        assert_eq!(s.mode.as_deref(), Some("acceptEdits"));
        let mut r = ChatState::default();
        run(&mut r, json!([{ "type": "control_request", "request_id": "r2", "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": {}, "permission_suggestions": [{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }] }] } }]));
        let Item::Tool(t) = &r.items[0].item else { panic!() };
        assert_eq!(t.permission.as_ref().unwrap().always.as_deref(), Some("Always allow Bash npm test:*"));
        assert_eq!(t.id, "r2", "a request without a tool id gets its own card");
        let mut m = ChatState::default();
        run(&mut m, json!([{ "type": "control_request", "request_id": "r3", "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": {}, "permission_suggestions": [
            { "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "ls:*" }, { "toolName": "Bash", "ruleContent": "rm:*" }] },
            { "type": "addDirectories", "directories": ["C:/"] }] } }]));
        let Item::Tool(t) = &m.items[0].item else { panic!() };
        assert_eq!(t.permission.as_ref().unwrap().always.as_deref(), Some("Always allow Bash ls:* and 2 more"), "every change Always makes is counted");
    }

    #[test]
    fn shows_a_saved_session_and_continues_it() {
        let lines = json!([
            { "type": "zenv_truncated", "count": 4 },
            { "type": "zenv_prompt", "text": "make a button", "images": 1 },
            { "type": "assistant", "message": { "content": [{ "type": "text", "text": "Done" }] } }
        ]);
        let s = ChatState::history("s1", lines.as_array().unwrap());
        assert_eq!(s.session_id.as_deref(), Some("s1"));
        assert!(!s.busy);
        assert_eq!(s.items[0].item, Item::Notice { text: "4 earlier messages not shown.".into(), error: false });
        assert_eq!(s.items[1].item, Item::User { text: "make a button".into(), files: vec!["image 1".into()] });
        assert!(matches!(&s.items[2].item, Item::Text { text, .. } if text == "Done"));
    }

    #[test]
    fn maps_files_inside_the_project_and_refuses_others() {
        assert_eq!(relative("C:\\xampp\\htdocs\\app", "C:\\xampp\\htdocs\\app\\src\\a.php").as_deref(), Some("src/a.php"));
        assert_eq!(relative("C:\\xampp\\htdocs\\app", "c:/XAMPP/htdocs/app/b.php").as_deref(), Some("b.php"));
        assert_eq!(relative("C:\\xampp\\htdocs\\app", "D:\\other\\c.php"), None);
        assert_eq!(relative("C:\\xampp\\htdocs\\app", "src/d.php").as_deref(), Some("src/d.php"));
    }
}
