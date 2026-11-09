//! The managed-file ledger. Zenv writes a file only if it created it (or
//! it does not exist yet), replaces it atomically, and keeps a backup of any
//! content it overwrites.

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::error::{AppError, AppResult, IoContext};
use crate::storage::Db;
use crate::util::{file_timestamp, new_id, now_rfc3339, sha256_hex};

#[derive(Debug, Clone, Copy)]
pub struct Owner<'a> {
    pub kind: &'a str,
    pub id: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOutcome {
    pub path: PathBuf,
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

fn key(path: &Path) -> AppResult<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::invalid_path(path, "path is not valid UTF-8"))
}

/// Writes `contents` to `path` via a temp file in the same directory followed
/// by a rename, so readers never observe a partially written file.
pub fn atomic_write(path: &Path, contents: &[u8]) -> AppResult<()> {
    let dir = path.parent().ok_or_else(|| AppError::invalid_path(path, "has no parent directory"))?;
    std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", new_id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp).ctx(|| format!("creating {}", tmp.display()))?;
        f.write_all(contents).ctx(|| format!("writing {}", tmp.display()))?;
        f.sync_all().ctx(|| format!("flushing {}", tmp.display()))?;
        drop(f);
        std::fs::rename(&tmp, path).ctx(|| format!("replacing {}", path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn write_managed(
    db: &Db,
    backups_dir: &Path,
    path: &Path,
    contents: &str,
    owner: Owner<'_>,
) -> AppResult<WriteOutcome> {
    let k = key(path)?;
    let record: Option<(String, String)> = db.read(|c| {
        Ok(c.query_row(
            "SELECT owner_kind, owner_id FROM managed_files WHERE path = ?1",
            [&k],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
    })?;
    let existing = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(AppError::Io { context: format!("reading {}", path.display()), source: e }),
    };

    match (&existing, &record) {
        (Some(_), None) => {
            return Err(AppError::Conflict(format!(
                "{} already exists and was not created by Zenv; refusing to overwrite it",
                path.display()
            )))
        }
        (_, Some((kind, id))) if kind != owner.kind || id != owner.id => {
            return Err(AppError::Conflict(format!(
                "{} is managed by {kind} '{id}', not {} '{}'",
                path.display(),
                owner.kind,
                owner.id
            )))
        }
        _ => {}
    }

    let new_bytes = contents.as_bytes();
    let unchanged = existing.as_deref() == Some(new_bytes);
    let mut backup = None;
    if !unchanged {
        if let Some(old) = &existing {
            let dir = backups_dir.join("files").join(owner.id);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let target = dir.join(format!("{name}.{}.bak", file_timestamp()));
            atomic_write(&target, old)?;
            backup = Some(target);
        }
        atomic_write(path, new_bytes)?;
    }

    db.write(|tx| {
        tx.execute(
            "INSERT INTO managed_files (path, owner_kind, owner_id, sha256, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (path) DO UPDATE SET sha256 = excluded.sha256, updated_at = excluded.updated_at",
            params![k, owner.kind, owner.id, sha256_hex(new_bytes), now_rfc3339()],
        )?;
        Ok(())
    })?;
    Ok(WriteOutcome { path: path.to_path_buf(), changed: !unchanged, backup })
}

/// Ledger paths of `kind` directly inside `dir`.
pub fn paths_under(db: &Db, kind: &str, dir: &Path) -> AppResult<Vec<PathBuf>> {
    let prefix = format!("{}\\", dir.display()).replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let rows: Vec<String> = db.read(|c| {
        let mut stmt = c.prepare(
            "SELECT path FROM managed_files WHERE owner_kind = ?1 AND path LIKE ?2 ESCAPE '\\'",
        )?;
        let rows = stmt.query_map(params![kind, format!("{prefix}%")], |r| r.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;
    Ok(rows
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.parent().is_some_and(|parent| parent.as_os_str().eq_ignore_ascii_case(dir.as_os_str())))
        .collect())
}

/// Deletes one managed file (backing it up first if someone edited it) and
/// drops its ledger entry.
pub fn remove_managed(db: &Db, backups_dir: &Path, path: &Path) -> AppResult<()> {
    let k = key(path)?;
    let record: Option<(String, String)> = db.read(|c| {
        Ok(c.query_row("SELECT owner_id, sha256 FROM managed_files WHERE path = ?1", [&k], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })?;
    let Some((owner_id, sha)) = record else {
        return Err(AppError::Conflict(format!("{} is not managed by Zenv; refusing to delete it", path.display())));
    };
    if let Ok(bytes) = std::fs::read(path) {
        if sha256_hex(&bytes) != sha {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            atomic_write(&backups_dir.join("files").join(&owner_id).join(format!("{name}.{}.bak", file_timestamp())), &bytes)?;
        }
        std::fs::remove_file(path).ctx(|| format!("removing {}", path.display()))?;
    }
    db.write(|tx| {
        tx.execute("DELETE FROM managed_files WHERE path = ?1", [&k])?;
        Ok(())
    })
}

/// Deletes an owner's files that still match what Zenv wrote, and drops
/// their ledger entries. Files edited by someone else are kept on disk.
pub fn release_owner(db: &Db, owner: Owner<'_>) -> AppResult<Vec<PathBuf>> {
    let files: Vec<(String, String)> = db.read(|c| {
        let mut stmt =
            c.prepare("SELECT path, sha256 FROM managed_files WHERE owner_kind = ?1 AND owner_id = ?2")?;
        let rows = stmt.query_map([owner.kind, owner.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;
    let mut kept = Vec::new();
    for (path, sha) in &files {
        match std::fs::read(path) {
            Ok(bytes) if sha256_hex(&bytes) == *sha => {
                std::fs::remove_file(path).ctx(|| format!("removing {path}"))?;
            }
            Ok(_) => kept.push(PathBuf::from(path)),
            Err(_) => {}
        }
    }
    db.write(|tx| {
        tx.execute(
            "DELETE FROM managed_files WHERE owner_kind = ?1 AND owner_id = ?2",
            [owner.kind, owner.id],
        )?;
        Ok(())
    })?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: Owner<'static> = Owner { kind: "service", id: "s1" };

    #[test]
    fn creates_then_no_ops_then_backs_up_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let path = dir.path().join("gen").join("httpd.conf");
        let backups = dir.path().join("backups");

        let first = write_managed(&db, &backups, &path, "A", OWNER).unwrap();
        assert!(first.changed && first.backup.is_none());

        let same = write_managed(&db, &backups, &path, "A", OWNER).unwrap();
        assert!(!same.changed && same.backup.is_none());

        let second = write_managed(&db, &backups, &path, "B", OWNER).unwrap();
        assert!(second.changed);
        let backup = second.backup.unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "B");
        // No temp files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn refuses_to_overwrite_unmanaged_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let path = dir.path().join("httpd.conf");
        std::fs::write(&path, "user content").unwrap();
        let err = write_managed(&db, dir.path(), &path, "ours", OWNER).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user content");
    }

    #[test]
    fn refuses_cross_owner_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let path = dir.path().join("x.conf");
        write_managed(&db, dir.path(), &path, "A", OWNER).unwrap();
        let other = Owner { kind: "service", id: "s2" };
        assert!(matches!(write_managed(&db, dir.path(), &path, "B", other), Err(AppError::Conflict(_))));
    }

    #[test]
    fn paths_under_lists_direct_children_and_remove_backs_up_edits() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let sites = dir.path().join("sites");
        let a = sites.join("a.conf");
        let nested = sites.join("deeper").join("b.conf");
        write_managed(&db, dir.path(), &a, "A", Owner { kind: "site", id: "a" }).unwrap();
        write_managed(&db, dir.path(), &nested, "B", Owner { kind: "site", id: "b" }).unwrap();
        assert_eq!(paths_under(&db, "site", &sites).unwrap(), vec![a.clone()]);

        std::fs::write(&a, "edited").unwrap();
        remove_managed(&db, &dir.path().join("bk"), &a).unwrap();
        assert!(!a.exists());
        assert!(paths_under(&db, "site", &sites).unwrap().is_empty());
        let backup = std::fs::read_dir(dir.path().join("bk").join("files").join("a")).unwrap().next().unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(backup.path()).unwrap(), "edited");

        let foreign = sites.join("user.conf");
        std::fs::write(&foreign, "x").unwrap();
        assert!(matches!(remove_managed(&db, dir.path(), &foreign), Err(AppError::Conflict(_))));
        assert!(foreign.exists());
    }

    #[test]
    fn release_keeps_user_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let untouched = dir.path().join("a.conf");
        let edited = dir.path().join("b.conf");
        write_managed(&db, dir.path(), &untouched, "A", OWNER).unwrap();
        write_managed(&db, dir.path(), &edited, "B", OWNER).unwrap();
        std::fs::write(&edited, "user edit").unwrap();

        let kept = release_owner(&db, OWNER).unwrap();
        assert!(!untouched.exists());
        assert_eq!(kept, vec![edited.clone()]);
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "user edit");
    }
}
