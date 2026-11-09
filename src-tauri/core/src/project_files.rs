//! An app's project files, for Studio's editor: list folders, open, save and
//! create files, find files by name.
//!
//! Paths are relative to the app's project root (`app/Http/Kernel.php`) and
//! never leave it: no `..`, drive letters, alternate data streams, device
//! names or Windows-trimmed names, and links that point outside the folder
//! are refused. `.git` is not shown or touched. Only UTF-8 text up to 2 MB
//! opens. `.env` files open read-only (Zenv only reads them). A save fails
//! when the file changed on disk since it was opened (Claude Code edited
//! it, say), unless the user chooses to overwrite.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::app_core::Core;
use crate::error::{AppError, AppResult, IoContext};

const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 5000;
/// Folders the name search does not walk into (dependencies and build output).
const SKIP_SEARCH: &[&str] = &[".git", "node_modules", "vendor", "target", ".next", ".nuxt", "__pycache__", ".venv", "venv", "dist", "build"];
const MAX_SEARCH_WALK: usize = 60_000;
const MAX_RESULTS: usize = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// Relative to the project root, with `/`.
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// More entries exist than were listed.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub path: String,
    pub content: String,
    /// Last change, in milliseconds since 1970; passed back when saving.
    pub modified: u64,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewKind {
    File,
    Folder,
}

fn invalid(rel: &str, why: &str) -> AppError {
    AppError::invalid_path(rel, why)
}

/// Windows device names, which open devices instead of files in any folder.
fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit())
}

/// The relative path's parts, each checked.
fn parts(rel: &str) -> AppResult<Vec<&str>> {
    let mut out = Vec::new();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        if part == "." || part == ".." {
            return Err(invalid(rel, "must stay inside the project folder"));
        }
        if part.contains([':', '<', '>', '"', '|', '?', '*']) || part.chars().any(|c| c.is_control()) {
            return Err(invalid(rel, "has characters Windows does not allow in names"));
        }
        // Windows drops trailing dots and spaces, so "a.php." is "a.php".
        if part.ends_with('.') || part.ends_with(' ') {
            return Err(invalid(rel, "names may not end with a dot or a space"));
        }
        if is_device_name(part) {
            return Err(invalid(rel, "is a reserved Windows device name"));
        }
        if part.eq_ignore_ascii_case(".git") {
            return Err(invalid(rel, "the .git folder is not available in the editor"));
        }
        out.push(part);
    }
    Ok(out)
}

fn canonical_root(root: &Path) -> AppResult<PathBuf> {
    dunce::canonicalize(root).map_err(|_| AppError::invalid_path(root, "is not an existing folder"))
}

/// The real location of an existing path, which must be inside the root.
fn inside(root: &Path, path: &Path, rel: &str) -> AppResult<PathBuf> {
    let real = dunce::canonicalize(path).map_err(|_| AppError::NotFound { entity: "file", id: rel.to_string() })?;
    if !real.starts_with(root) {
        return Err(invalid(rel, "points outside the project folder"));
    }
    Ok(real)
}

/// An existing file or folder in the project.
fn existing(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let root = canonical_root(root)?;
    let mut path = root.clone();
    path.extend(parts(rel)?);
    inside(&root, &path, rel)
}

/// A path to create: its folder must exist inside the project.
fn to_create(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let root = canonical_root(root)?;
    let parts = parts(rel)?;
    let Some((name, folders)) = parts.split_last() else {
        return Err(invalid(rel, "needs a name"));
    };
    let mut parent = root.clone();
    parent.extend(folders);
    Ok(inside(&root, &parent, rel)?.join(name))
}

fn to_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn modified(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

fn is_env_file(path: &Path) -> bool {
    path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).is_some_and(|n| n == ".env" || n.starts_with(".env."))
}

pub fn list(root: &Path, rel: &str) -> AppResult<Listing> {
    let dir = existing(root, rel)?;
    if !dir.is_dir() {
        return Err(invalid(rel, "is not a folder"));
    }
    let root = canonical_root(root)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for e in std::fs::read_dir(&dir).ctx(|| format!("cannot list {rel}"))?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case(".git") {
            continue;
        }
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        // Follows links, so a linked folder lists as a folder.
        let Ok(meta) = std::fs::metadata(e.path()) else { continue };
        entries.push(Entry { path: to_rel(&root, &e.path()), name, dir: meta.is_dir(), size: if meta.is_dir() { 0 } else { meta.len() } });
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing { entries, truncated })
}

pub fn read(root: &Path, rel: &str) -> AppResult<FileContent> {
    let path = existing(root, rel)?;
    let meta = std::fs::metadata(&path).ctx(|| format!("cannot read {rel}"))?;
    if meta.is_dir() {
        return Err(invalid(rel, "is a folder"));
    }
    if meta.len() > MAX_FILE {
        return Err(AppError::Unsupported(format!("{rel} is {:.1} MB; the editor opens files up to 2 MB", meta.len() as f64 / 1048576.0)));
    }
    let bytes = std::fs::read(&path).ctx(|| format!("cannot read {rel}"))?;
    if bytes.contains(&0) {
        return Err(AppError::Unsupported(format!("{rel} is a binary file")));
    }
    let content = String::from_utf8(bytes).map_err(|_| AppError::Unsupported(format!("{rel} is not UTF-8 text")))?;
    Ok(FileContent { path: rel.to_string(), content, modified: modified(&meta), read_only: is_env_file(&path) || meta.permissions().readonly() })
}

/// Saves `content`. With `expected`, refuses when the file changed since.
pub fn write(root: &Path, rel: &str, content: &str, expected: Option<u64>) -> AppResult<u64> {
    let path = existing(root, rel)?;
    let meta = std::fs::metadata(&path).ctx(|| format!("cannot save {rel}"))?;
    if meta.is_dir() {
        return Err(invalid(rel, "is a folder"));
    }
    if is_env_file(&path) {
        return Err(AppError::Validation(format!("{rel} is read-only in Zenv's editor (Zenv only reads .env files)")));
    }
    if expected.is_some_and(|m| m != modified(&meta)) {
        return Err(AppError::Conflict(format!("{rel} changed on disk since it was opened")));
    }
    std::fs::write(&path, content).ctx(|| format!("cannot save {rel}"))?;
    Ok(modified(&std::fs::metadata(&path).ctx(|| format!("cannot save {rel}"))?))
}

pub fn create(root: &Path, rel: &str, kind: NewKind) -> AppResult<Entry> {
    let path = to_create(root, rel)?;
    if is_env_file(&path) {
        return Err(AppError::Validation("Zenv's editor does not create .env files".into()));
    }
    if path.exists() {
        return Err(AppError::Conflict(format!("{rel} already exists")));
    }
    match kind {
        NewKind::Folder => std::fs::create_dir(&path).ctx(|| format!("cannot create {rel}"))?,
        NewKind::File => drop(std::fs::OpenOptions::new().write(true).create_new(true).open(&path).ctx(|| format!("cannot create {rel}"))?),
    }
    let root = canonical_root(root)?;
    Ok(Entry { name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(), path: to_rel(&root, &path), dir: kind == NewKind::Folder, size: 0 })
}

/// Last-change times of open files (None: gone), to notice outside edits.
pub fn stat(root: &Path, rels: &[String]) -> Vec<Option<u64>> {
    rels.iter().map(|r| existing(root, r).ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| modified(&m))).collect()
}

/// How well `path` matches `query`: name contains it, path contains it, or
/// its letters appear in order. Lower is better.
fn score(path: &str, query: &str) -> Option<usize> {
    let p = path.to_lowercase();
    let name = p.rsplit('/').next().unwrap_or(&p);
    if name.starts_with(query) {
        return Some(p.len());
    }
    if name.contains(query) {
        return Some(1000 + p.len());
    }
    if p.contains(query) {
        return Some(2000 + p.len());
    }
    let mut chars = p.chars();
    query.chars().filter(|c| !c.is_whitespace()).all(|q| chars.any(|c| c == q)).then_some(3000 + p.len())
}

/// Files whose path matches `query`, best first (dependencies skipped).
pub fn search(root: &Path, query: &str) -> AppResult<Vec<Entry>> {
    let root = canonical_root(root)?;
    let query = query.trim().to_lowercase().replace('\\', "/");
    if query.is_empty() {
        return Ok(vec![]);
    }
    let mut found: Vec<(usize, Entry)> = Vec::new();
    let mut queue = vec![root.clone()];
    let mut walked = 0;
    while let Some(dir) = queue.pop() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            walked += 1;
            if walked > MAX_SEARCH_WALK {
                break;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if !SKIP_SEARCH.iter().any(|s| name.eq_ignore_ascii_case(s)) {
                    queue.push(e.path());
                }
                continue;
            }
            let rel = to_rel(&root, &e.path());
            if let Some(s) = score(&rel, &query) {
                found.push((s, Entry { name, path: rel, dir: false, size: e.metadata().map(|m| m.len()).unwrap_or(0) }));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.path.cmp(&b.1.path)));
    Ok(found.into_iter().take(MAX_RESULTS).map(|(_, e)| e).collect())
}

impl Core {
    pub fn project_root(&self, site_id: &str) -> AppResult<PathBuf> {
        Ok(self.sites.get(site_id)?.project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("app/Http")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("vendor/laravel")).unwrap();
        std::fs::write(root.join("app/Http/Kernel.php"), "<?php\r\n").unwrap();
        std::fs::write(root.join("vendor/laravel/Kernel.php"), "").unwrap();
        std::fs::write(root.join(".env"), "APP_KEY=x").unwrap();
        std::fs::write(root.join("logo.png"), [0x89, 0, 1]).unwrap();
        dir
    }

    #[test]
    fn lists_folders_first_without_git() {
        let p = project();
        let names: Vec<_> = list(p.path(), "").unwrap().entries.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["app", "vendor", ".env", "logo.png"]);
        let inner = list(p.path(), "app/Http").unwrap();
        assert_eq!(inner.entries[0].path, "app/Http/Kernel.php");
    }

    #[test]
    fn paths_cannot_leave_the_project() {
        let p = project();
        for bad in ["../x", "app/../../x", "C:/Windows/win.ini", "app/a.php.", "con.txt", "a:stream", ".git/config"] {
            assert!(read(p.path(), bad).is_err(), "{bad} must be refused");
        }
        assert!(create(p.path(), "../evil.php", NewKind::File).is_err());
    }

    #[test]
    fn saves_keep_line_endings_and_refuse_stale_writes() {
        let p = project();
        let f = read(p.path(), "app/Http/Kernel.php").unwrap();
        assert_eq!(f.content, "<?php\r\n");
        assert!(!f.read_only);
        let m = write(p.path(), "app/Http/Kernel.php", "<?php\r\necho 1;\r\n", Some(f.modified)).unwrap();
        assert_eq!(std::fs::read_to_string(p.path().join("app/Http/Kernel.php")).unwrap(), "<?php\r\necho 1;\r\n");
        let err = write(p.path(), "app/Http/Kernel.php", "x", Some(m.wrapping_sub(5000))).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        write(p.path(), "app/Http/Kernel.php", "forced", None).unwrap();
    }

    #[test]
    fn env_is_read_only_and_binaries_do_not_open() {
        let p = project();
        assert!(read(p.path(), ".env").unwrap().read_only);
        assert!(write(p.path(), ".env", "APP_KEY=y", None).is_err());
        assert!(matches!(read(p.path(), "logo.png").unwrap_err(), AppError::Unsupported(_)));
    }

    #[test]
    fn creates_files_and_folders_once() {
        let p = project();
        assert_eq!(create(p.path(), "app/Models", NewKind::Folder).unwrap().path, "app/Models");
        assert_eq!(create(p.path(), "app/Models/User.php", NewKind::File).unwrap().path, "app/Models/User.php");
        assert!(matches!(create(p.path(), "app/Models/User.php", NewKind::File).unwrap_err(), AppError::Conflict(_)));
        assert!(create(p.path(), "missing/User.php", NewKind::File).is_err());
    }

    #[test]
    fn name_search_skips_dependencies() {
        let p = project();
        let hits: Vec<_> = search(p.path(), "kernel").unwrap().into_iter().map(|e| e.path).collect();
        assert_eq!(hits, vec!["app/Http/Kernel.php"]);
        assert_eq!(search(p.path(), "ahk").unwrap().len(), 1, "letters in order");
    }
}
