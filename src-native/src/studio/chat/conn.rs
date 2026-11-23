use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use zenv_core::app_core::Core;
use zenv_core::error::AppError;
use zenv_core::studio_chat::{Attachment, ChatControl, ChatOptions, ChatSink};

pub struct Live {
    serial: u64,
    pub generation: u64,
    id: String,
    pub options: ChatOptions,
}

impl Live {
    #[cfg(test)]
    pub fn running(generation: u64, options: ChatOptions) -> Self {
        Self { serial: 0, generation, id: String::new(), options }
    }
}

pub enum Event {
    Error(String),
    ControlSent { tag: u64, request_id: String },
    ControlFailed { tag: u64, error: String },
}

#[derive(Default)]
pub struct Shared {
    pub generation: u64,
    pub lines: Vec<(u64, Value)>,
    pub responses: HashMap<String, Value>,
    pub events: Vec<Event>,
    pub live: Option<Live>,
}

pub fn lock(s: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    s.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct Start {
    pub generation: u64,
    pub options: ChatOptions,
    pub resume: Option<String>,
}

pub enum Op {
    Send { start: Start, text: String, attachments: Vec<Attachment> },
    Control { start: Start, tag: u64, control: ChatControl },
    Answer { request_id: String, allow: bool, always: bool },
    Interrupt,
    End,
}

#[derive(Clone)]
pub struct Target {
    pub core: Arc<Core>,
    pub owner: String,
    pub site_id: String,
    pub repaint: Arc<dyn Fn() + Send + Sync>,
}

pub fn needs_restart(live: Option<&ChatOptions>, wanted: &ChatOptions, busy: bool) -> bool {
    live.is_some_and(|applied| !busy && applied != wanted)
}

pub fn spawn(target: Target, shared: Arc<Mutex<Shared>>) -> Sender<Op> {
    let (tx, rx) = channel();
    std::thread::Builder::new().name("studio-chat".into()).spawn(move || Worker { target, shared, serial: 0 }.run(rx)).expect("chat worker thread");
    tx
}

struct Worker {
    target: Target,
    shared: Arc<Mutex<Shared>>,
    serial: u64,
}

impl Worker {
    fn run(mut self, rx: Receiver<Op>) {
        while let Ok(op) = rx.recv() {
            if let Err(e) = self.handle(op) {
                lock(&self.shared).events.push(Event::Error(e));
                (self.target.repaint)();
            }
        }
        self.end();
    }

    fn live_id(&self, generation: Option<u64>) -> Option<String> {
        let s = lock(&self.shared);
        s.live.as_ref().filter(|l| generation.is_none_or(|g| l.generation == g)).map(|l| l.id.clone())
    }

    fn end(&mut self) {
        let live = lock(&self.shared).live.take();
        if let Some(l) = live {
            let _ = self.target.core.chats.close(&self.target.owner, &l.id);
        }
    }

    fn ensure(&mut self, start: &Start) -> Result<String, String> {
        if let Some(id) = self.live_id(Some(start.generation)) {
            return Ok(id);
        }
        self.end();
        self.serial += 1;
        let (serial, generation) = (self.serial, start.generation);
        let (shared, repaint) = (self.shared.clone(), self.target.repaint.clone());
        let sink: ChatSink = Arc::new(move |line: String| {
            let Ok(v) = serde_json::from_str::<Value>(&line) else { return };
            let mut s = lock(&shared);
            match v["type"].as_str() {
                Some("control_response") => {
                    let rid = v["response"]["request_id"].as_str().unwrap_or("").to_owned();
                    if rid.starts_with("zenv-") {
                        s.responses.insert(rid, v["response"].clone());
                    }
                }
                Some("zenv_exit") => {
                    if s.live.as_ref().is_some_and(|l| l.serial == serial) {
                        s.live = None;
                    }
                    s.lines.push((generation, v));
                }
                _ => s.lines.push((generation, v)),
            }
            drop(s);
            repaint();
        });
        let t = &self.target;
        let id = t.core.studio_open_chat(&t.owner, &t.site_id, start.resume.as_deref(), &start.options, sink).map_err(|e| e.to_string())?;
        lock(&self.shared).live = Some(Live { serial, generation, id: id.clone(), options: start.options.clone() });
        (self.target.repaint)();
        Ok(id)
    }

    fn handle(&mut self, op: Op) -> Result<(), String> {
        let t = self.target.clone();
        let stale = |start: &Start| start.generation != lock(&self.shared).generation;
        if matches!(&op, Op::Send { start, .. } | Op::Control { start, .. } if stale(start)) {
            return Ok(());
        }
        match op {
            Op::Send { start, text, attachments } => {
                let id = self.ensure(&start)?;
                match t.core.studio_chat_send(&t.owner, &id, &text, &attachments) {
                    Err(AppError::NotFound { .. }) => {
                        lock(&self.shared).live = None;
                        let id = self.ensure(&start)?;
                        t.core.studio_chat_send(&t.owner, &id, &text, &attachments).map_err(|e| e.to_string())
                    }
                    other => other.map_err(|e| e.to_string()),
                }
            }
            Op::Control { start, tag, control } => {
                let sent = self.ensure(&start).and_then(|id| t.core.chats.control(&t.owner, &id, &control).map_err(|e| e.to_string()));
                let event = match sent {
                    Ok(request_id) => Event::ControlSent { tag, request_id },
                    Err(error) => Event::ControlFailed { tag, error },
                };
                lock(&self.shared).events.push(event);
                (t.repaint)();
                Ok(())
            }
            Op::Answer { request_id, allow, always } => {
                let id = self.live_id(None).ok_or("Claude Code has stopped; send a message to start it again.")?;
                t.core.chats.answer(&t.owner, &id, &request_id, allow, always).map_err(|e| e.to_string())
            }
            Op::Interrupt => {
                let Some(id) = self.live_id(None) else { return Ok(()) };
                t.core.chats.interrupt(&t.owner, &id).map_err(|e| e.to_string())
            }
            Op::End => {
                self.end();
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenv_core::studio_chat::Effort;

    #[test]
    fn settings_changed_between_turns_restart_the_program() {
        let applied = ChatOptions::default();
        let wanted = ChatOptions { effort: Some(Effort::High), ..ChatOptions::default() };
        assert!(needs_restart(Some(&applied), &wanted, false));
        assert!(!needs_restart(Some(&applied), &wanted, true), "never while Claude is working");
        assert!(!needs_restart(Some(&applied), &applied.clone(), false));
        assert!(!needs_restart(None, &wanted, false), "nothing running: it starts with the new settings anyway");
    }
}
