//! Database servers (MySQL, PostgreSQL) as managed services.
//!
//! Rules that apply to every database adapter:
//! - a data folder is initialized only by an explicit user action, and only
//!   when it is missing or empty;
//! - a folder holding anything else is never used, changed or deleted;
//! - connectivity is checked at the protocol level without client libraries.

pub mod mysql;
pub mod postgres;

use std::path::Path;

/// True when `dir` does not exist or has no entries.
pub fn missing_or_empty(dir: &Path) -> Option<bool> {
    if !dir.exists() {
        return Some(true);
    }
    std::fs::read_dir(dir).ok().map(|mut it| it.next().is_none())
}

/// `9.7.0` ≥ `8.0.16`?
pub fn version_at_least(version: &str, min: (u32, u32, u32)) -> bool {
    let v: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    let get = |i: usize| v.get(i).copied().unwrap_or(0);
    (get(0), get(1), get(2)) >= min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(version_at_least("9.7.0", (8, 0, 16)));
        assert!(version_at_least("8.0.16", (8, 0, 16)));
        assert!(!version_at_least("8.0.15", (8, 0, 16)));
        assert!(!version_at_least("5.7.44", (8, 0, 16)));
    }

    #[test]
    fn empty_folder_detection() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(missing_or_empty(&dir.path().join("nope")), Some(true));
        assert_eq!(missing_or_empty(dir.path()), Some(true));
        std::fs::write(dir.path().join("x"), "").unwrap();
        assert_eq!(missing_or_empty(dir.path()), Some(false));
    }
}
