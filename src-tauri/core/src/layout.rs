//! Moving a Zenv home from the earlier folder name `www` (the same as
//! Laragon's) to `dev`: the web servers' default pages.
//!
//! The folder is renamed when Zenv starts, then stored paths that pointed into
//! it are rewritten in the database. Nothing else refers to it (server
//! configurations are generated again on start). Running again changes nothing.

use std::path::{Path, PathBuf};

use rusqlite::params;

use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use crate::storage::Db;

/// (earlier name, current name).
pub const RENAMED: &[(&str, &str)] = &[("www", "dev")];

/// Renames earlier-layout folders. If one cannot be renamed (a program holds
/// a file in it), the ones already renamed are put back.
pub fn migrate_folders(root: &Path) -> AppResult<()> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (old, new) in RENAMED {
        let (from, to) = (root.join(old), root.join(new));
        if !from.is_dir() || to.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&from, &to) {
            for (f, t) in done.iter().rev() {
                let _ = std::fs::rename(t, f);
            }
            return Err(AppError::Conflict(format!(
                "Zenv could not rename {} to {} ({e}). A program is using a file in it, such as an editor or a \
                 terminal. Close it and start Zenv again.",
                from.display(),
                to.display()
            )));
        }
        tracing::info!(from = %from.display(), to = %to.display(), "folder renamed");
        done.push((from, to));
    }
    Ok(())
}

/// Folders that were renamed (the earlier one is gone, the new one exists).
fn moves(root: &Path) -> Vec<(PathBuf, PathBuf)> {
    RENAMED
        .iter()
        .map(|(o, n)| (root.join(o), root.join(n)))
        .filter(|(o, n)| !o.exists() && n.is_dir())
        .collect()
}

/// Rewrites paths into renamed folders, in either slash style and any case.
/// `C:\zenv\www` matches only as a whole folder name (not `...\wwwroot`).
pub fn rewrite(text: &str, moves: &[(PathBuf, PathBuf)]) -> String {
    let mut out = text.to_string();
    for (old, new) in moves {
        let old = old.display().to_string();
        let new = new.display().to_string();
        for (o, n) in [(old.replace('/', "\\"), new.replace('/', "\\")), (old.replace('\\', "/"), new.replace('\\', "/"))] {
            let lower_o = o.to_ascii_lowercase();
            let mut result = String::with_capacity(out.len());
            let mut rest = out.as_str();
            loop {
                let Some(i) = rest.to_ascii_lowercase().find(&lower_o) else {
                    result.push_str(rest);
                    break;
                };
                let after = rest[i + o.len()..].chars().next();
                let whole = after.is_none_or(|c| matches!(c, '\\' | '/' | '"' | '\'' | ';' | ' ' | '\r' | '\n' | '\t'));
                result.push_str(&rest[..i]);
                result.push_str(if whole { &n } else { &rest[i..i + o.len()] });
                rest = &rest[i + o.len()..];
            }
            out = result;
        }
    }
    out
}

/// Updates stored paths after the folders moved.
pub fn fix_references(db: &Db, paths: &AppPaths) -> AppResult<()> {
    let moves = moves(&paths.root);
    if moves.is_empty() {
        return Ok(());
    }
    let columns: &[(&str, &str, &str)] = &[
        ("services", "id", "data_dir"),
        ("managed_files", "path", "path"),
        ("installations", "id", "root_path"),
        ("installations", "id", "executable_path"),
        ("sites", "id", "project_root"),
        ("sites", "id", "document_root"),
    ];
    db.write(|tx| {
        for (table, key, col) in columns {
            let rows: Vec<(String, Option<String>)> = {
                let mut stmt = tx.prepare(&format!("SELECT {key}, {col} FROM {table}"))?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<_, _>>()?
            };
            for (k, value) in rows {
                let Some(value) = value else { continue };
                let new = rewrite(&value, &moves);
                if new != value {
                    tx.execute(&format!("UPDATE OR REPLACE {table} SET {col} = ?1 WHERE {key} = ?2"), params![new, k])?;
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_whole_folder_names_only() {
        let moves = vec![(PathBuf::from(r"C:\zenv\www"), PathBuf::from(r"C:\zenv\dev"))];
        let text = "a=c:\\ZENV\\www\\svc\nb=C:/zenv/www\nc=C:/zenv/etc/ssl\n";
        let once = rewrite(text, &moves);
        assert_eq!(once, "a=C:\\zenv\\dev\\svc\nb=C:/zenv/dev\nc=C:/zenv/etc/ssl\n");
        assert_eq!(rewrite(&once, &moves), once, "running again changes nothing");
        assert_eq!(rewrite(r"C:\zenv\wwwroot\x", &moves), r"C:\zenv\wwwroot\x");
    }

    #[test]
    fn the_folder_is_renamed_and_stored_paths_follow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for d in ["data", "etc/svc", "www/svc"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("www/svc/index.html"), "x").unwrap();
        let paths = AppPaths::new(root);
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let old_root = root.join("www").join("docs");
        db.write(|tx| {
            tx.execute(
                "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, strategy, enabled, config_status, created_at, updated_at)
                 VALUES ('s', 'docs', 'docs.test', ?1, ?1, 'static', 'static', 1, 'pending', 't', 't')",
                [old_root.display().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
        fix_references(&db, &paths).unwrap();

        assert!(!root.join("www").exists() && paths.dev_dir.join("svc").join("index.html").is_file());
        assert!(root.join("etc").is_dir() && root.join("data").is_dir(), "etc and data keep their names");
        let doc: String = db.read(|c| Ok(c.query_row("SELECT document_root FROM sites", [], |r| r.get(0))?)).unwrap();
        assert_eq!(PathBuf::from(doc), root.join("dev").join("docs"));
    }
}
