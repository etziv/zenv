pub mod migrations;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, Transaction};

use crate::error::{AppError, AppResult, IoContext};
use crate::util::file_timestamp;

/// SQLite metadata store. A single connection behind a mutex is plenty for a
/// desktop app; WAL keeps readers from blocking on writers.
pub struct Db {
    conn: Mutex<Connection>,
    schema_version: u32,
}

impl Db {
    pub fn open(path: &Path, backups_dir: &Path) -> AppResult<Self> {
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        let backup = |c: &Connection, from: u32| backup_before_migration(c, backups_dir, from);
        let report = migrations::migrate(&mut conn, migrations::MIGRATIONS, Some(&backup))?;
        Ok(Db { conn: Mutex::new(conn), schema_version: report.to })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn)?;
        let report = migrations::migrate(&mut conn, migrations::MIGRATIONS, None)?;
        Ok(Db { conn: Mutex::new(conn), schema_version: report.to })
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave SQLite inconsistent
        // (transactions roll back on drop), so recovering the guard is safe.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        f(&self.lock())
    }

    pub fn write<T>(&self, f: impl FnOnce(&Transaction<'_>) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    // In-memory databases report "memory"; that is fine.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    Ok(())
}

fn backup_before_migration(conn: &Connection, backups_dir: &Path, from: u32) -> AppResult<()> {
    std::fs::create_dir_all(backups_dir).ctx(|| format!("creating {}", backups_dir.display()))?;
    let target = backups_dir.join(format!("zenv-v{from}-{}.db", file_timestamp()));
    let target_str = target
        .to_str()
        .ok_or_else(|| AppError::invalid_path(&target, "backup path is not valid UTF-8"))?;
    conn.execute("VACUUM INTO ?1", [target_str])?;
    tracing::info!(path = %target.display(), "database backed up before migration");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_a_file_database_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("zenv.db");
        let backups = dir.path().join("backups");
        {
            let db = Db::open(&db_path, &backups).unwrap();
            db.write(|tx| {
                tx.execute(
                    "INSERT INTO installations VALUES ('i1','php','8.4.21','x','C:/p','C:/p/php.exe','external',0,'t','t','t',1)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        }
        let db = Db::open(&db_path, &backups).unwrap();
        let n: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM installations", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(db.schema_version(), migrations::latest_version(migrations::MIGRATIONS));
    }

    #[test]
    fn vacuum_into_backup_produces_a_readable_copy() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("a.db"), &dir.path().join("b")).unwrap();
        db.read(|c| backup_before_migration(c, &dir.path().join("b"), 1)).unwrap();
        let backup = std::fs::read_dir(dir.path().join("b")).unwrap().next().unwrap().unwrap();
        let copy = Connection::open(backup.path()).unwrap();
        assert_eq!(migrations::current_version(&copy).unwrap(), migrations::latest_version(migrations::MIGRATIONS));
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Db::open_in_memory().unwrap();
        let err = db.write(|tx| {
            tx.execute(
                "INSERT INTO services VALUES ('s1','apache','x','missing','127.0.0.1',80,0,'t','t')",
                [],
            )?;
            Ok(())
        });
        assert!(err.is_err());
    }
}
