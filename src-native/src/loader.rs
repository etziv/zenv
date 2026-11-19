use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use zenv_core::app_core::Core;
use zenv_core::error::AppResult;

pub struct Loader<T> {
    value: Option<Result<T, String>>,
    pending: Option<Receiver<Result<T, String>>>,
    fetched: Option<Instant>,
    refetch: bool,
}

impl<T> Default for Loader<T> {
    fn default() -> Self {
        Self { value: None, pending: None, fetched: None, refetch: false }
    }
}

impl<T: Send + 'static> Loader<T> {
    pub fn ready(value: Result<T, String>) -> Self {
        Self { value: Some(value), pending: None, fetched: Some(Instant::now()), refetch: false }
    }

    pub fn get(&self) -> Option<&Result<T, String>> {
        self.value.as_ref()
    }

    pub fn ok(&self) -> Option<&T> {
        self.value.as_ref().and_then(|v| v.as_ref().ok())
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn invalidate(&mut self) {
        self.fetched = None;
        self.refetch = self.pending.is_some();
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.pending else { return };
        let value = match rx.try_recv() {
            Ok(v) => v,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("stopped unexpectedly".into()),
        };
        self.value = Some(value);
        self.pending = None;
        self.fetched = if std::mem::take(&mut self.refetch) { None } else { Some(Instant::now()) };
    }

    pub fn refresh_every(
        &mut self,
        every: Duration,
        ctx: &egui::Context,
        core: &Arc<Core>,
        fetch: impl FnOnce(&Core) -> AppResult<T> + Send + 'static,
    ) {
        self.poll();
        let stale = self.fetched.is_none_or(|t| t.elapsed() >= every);
        if stale && self.pending.is_none() {
            self.pending = Some(spawn(ctx, core, fetch));
        }
        if let Some(t) = self.fetched {
            ctx.request_repaint_after(every.saturating_sub(t.elapsed()));
        }
    }
}

fn spawn<T: Send + 'static>(
    ctx: &egui::Context,
    core: &Arc<Core>,
    fetch: impl FnOnce(&Core) -> AppResult<T> + Send + 'static,
) -> Receiver<Result<T, String>> {
    let (tx, rx) = channel();
    let (ctx, core) = (ctx.clone(), core.clone());
    std::thread::spawn(move || {
        let _ = tx.send(fetch(&core).map_err(|e| e.to_string()));
        ctx.request_repaint();
    });
    rx
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub tone: Tone,
    pub title: String,
    pub description: String,
}

impl Note {
    pub fn new(tone: Tone, title: impl Into<String>, description: impl Into<String>) -> Self {
        Self { tone, title: title.into(), description: description.into() }
    }
}

pub type Job = Box<dyn FnOnce(&Core) -> AppResult<Vec<Note>> + Send>;

pub struct Task {
    pub key: String,
    pub label: String,
    pub inline: bool,
    pub job: Job,
}

impl Task {
    pub fn new(key: impl Into<String>, label: impl Into<String>, job: impl FnOnce(&Core) -> AppResult<()> + Send + 'static) -> Self {
        Self::reporting(key, label, move |c| job(c).map(|()| Vec::new()))
    }

    pub fn reporting(key: impl Into<String>, label: impl Into<String>, job: impl FnOnce(&Core) -> AppResult<Vec<Note>> + Send + 'static) -> Self {
        Self { key: key.into(), label: label.into(), inline: false, job: Box::new(job) }
    }

    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task").field("key", &self.key).field("label", &self.label).finish()
    }
}

pub struct Done {
    pub key: String,
    pub label: String,
    pub inline: bool,
    pub result: Result<Vec<Note>, String>,
}

#[derive(Default)]
pub struct Tasks {
    running: Vec<(String, Receiver<Done>)>,
}

impl Tasks {
    pub fn busy(&self, key: &str) -> bool {
        self.running.iter().any(|(k, _)| k == key)
    }

    pub fn busy_keys(&self) -> Vec<String> {
        self.running.iter().map(|(k, _)| k.clone()).collect()
    }

    pub fn start(&mut self, ctx: &egui::Context, core: &Arc<Core>, task: Task) {
        if self.busy(&task.key) {
            return;
        }
        let (tx, rx) = channel();
        let (ctx, core) = (ctx.clone(), core.clone());
        let Task { key, label, inline, job } = task;
        self.running.push((key.clone(), rx));
        std::thread::spawn(move || {
            let result = job(&core).map_err(|e| e.to_string());
            let _ = tx.send(Done { key, label, inline, result });
            ctx.request_repaint();
        });
    }

    pub fn finished(&mut self) -> Vec<Done> {
        let mut done = Vec::new();
        self.running.retain(|(key, rx)| match rx.try_recv() {
            Ok(d) => {
                done.push(d);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push(Done { key: key.clone(), label: key.clone(), inline: false, result: Err("stopped unexpectedly".into()) });
                false
            }
        });
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fetch_that_died_becomes_an_error_instead_of_loading_forever() {
        let (tx, rx) = channel::<Result<u8, String>>();
        drop(tx);
        let mut l = Loader { pending: Some(rx), ..Loader::default() };
        l.poll();
        assert!(!l.loading());
        assert_eq!(l.get(), Some(&Err("stopped unexpectedly".to_owned())));
    }

    #[test]
    fn invalidate_during_a_fetch_keeps_the_result_stale() {
        let (tx, rx) = channel();
        let mut l = Loader { pending: Some(rx), ..Loader::default() };
        l.invalidate();
        tx.send(Ok(1u8)).unwrap();
        l.poll();
        assert_eq!(l.ok(), Some(&1));
        assert!(l.fetched.is_none(), "fetched again on the next refresh");

        let (tx, rx) = channel();
        l.pending = Some(rx);
        tx.send(Ok(2)).unwrap();
        l.poll();
        assert!(l.fetched.is_some());
    }
}
