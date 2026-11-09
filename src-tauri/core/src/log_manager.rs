use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppResult, IoContext};

const MAX_TAIL_BYTES: u64 = 512 * 1024;
pub const MAX_LINES: usize = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogTail {
    pub path: PathBuf,
    pub exists: bool,
    pub size_bytes: u64,
    pub lines: Vec<String>,
    /// True when older content exists beyond what was returned.
    pub truncated: bool,
}

/// Returns up to `max_lines` trailing lines, reading at most 512 KiB.
pub fn tail(path: &Path, max_lines: usize) -> AppResult<LogTail> {
    let max_lines = max_lines.clamp(1, MAX_LINES);
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LogTail { path: path.to_path_buf(), exists: false, size_bytes: 0, lines: vec![], truncated: false })
        }
        Err(e) => return Err(e).ctx(|| format!("opening {}", path.display())),
    };
    let size = file.metadata().ctx(|| format!("reading {}", path.display()))?.len();
    let start = size.saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ctx(|| format!("seeking {}", path.display()))?;
    let mut buf = Vec::with_capacity((size - start) as usize);
    file.read_to_end(&mut buf).ctx(|| format!("reading {}", path.display()))?;
    let text = String::from_utf8_lossy(&buf);

    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // first line is likely partial
    }
    let truncated = start > 0 || lines.len() > max_lines;
    let skip = lines.len().saturating_sub(max_lines);
    Ok(LogTail {
        path: path.to_path_buf(),
        exists: true,
        size_bytes: size,
        lines: lines[skip..].iter().map(|l| l.trim_end_matches('\r').to_string()).collect(),
        truncated,
    })
}

/// Last few lines of each existing file, for error diagnostics.
pub fn diagnostic_tail(paths: &[PathBuf], lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    for p in paths {
        if let Ok(t) = tail(p, lines) {
            if t.exists && !t.lines.is_empty() {
                out.push(format!("--- {}", p.display()));
                out.extend(t.lines);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let t = tail(&dir.path().join("none.log"), 10).unwrap();
        assert!(!t.exists && t.lines.is_empty());
    }

    #[test]
    fn returns_last_lines_without_cr() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.log");
        std::fs::write(&p, "one\r\ntwo\r\nthree\r\n").unwrap();
        let t = tail(&p, 2).unwrap();
        assert_eq!(t.lines, vec!["two", "three"]);
        assert!(t.truncated);
    }

    #[test]
    fn large_files_are_read_from_the_end_only() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.log");
        let line = "x".repeat(99) + "\n";
        let mut content = line.repeat(20_000); // ~2 MB
        content.push_str("LAST\n");
        std::fs::write(&p, content).unwrap();
        let t = tail(&p, 5).unwrap();
        assert_eq!(t.lines.last().unwrap(), "LAST");
        assert_eq!(t.lines.len(), 5);
        assert!(t.truncated);
    }
}
