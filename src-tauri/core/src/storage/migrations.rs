use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::util::now_rfc3339;

#[derive(Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration { version: 1, name: "initial", sql: include_str!("migrations/0001_initial.sql") },
    Migration { version: 2, name: "local_sites", sql: include_str!("migrations/0002_local_sites.sql") },
    Migration { version: 3, name: "php_sites", sql: include_str!("migrations/0003_php_sites.sql") },
    Migration { version: 4, name: "any_server_sites", sql: include_str!("migrations/0004_any_server_sites.sql") },
    Migration { version: 5, name: "service_data_dir", sql: include_str!("migrations/0005_service_data_dir.sql") },
    Migration { version: 6, name: "https", sql: include_str!("migrations/0006_https.sql") },
    Migration { version: 7, name: "languages_always_on", sql: include_str!("migrations/0007_languages_always_on.sql") },
    Migration { version: 8, name: "app_commands", sql: include_str!("migrations/0008_app_commands.sql") },
    Migration { version: 9, name: "tailnet_apps", sql: include_str!("migrations/0009_tailnet_apps.sql") },
    Migration { version: 10, name: "path_choices", sql: include_str!("migrations/0010_path_choices.sql") },
    Migration { version: 11, name: "app_settings", sql: include_str!("migrations/0011_app_settings.sql") },
    Migration { version: 12, name: "docker_apps", sql: include_str!("migrations/0012_docker_apps.sql") },
    Migration { version: 13, name: "app_command_approval", sql: include_str!("migrations/0013_app_command_approval.sql") },
    Migration { version: 14, name: "tailnet_services", sql: include_str!("migrations/0014_tailnet_services.sql") },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub from: u32,
    pub to: u32,
}

pub fn latest_version(migrations: &[Migration]) -> u32 {
    migrations.last().map_or(0, |m| m.version)
}

pub fn current_version(conn: &Connection) -> AppResult<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Applies pending migrations, each in its own transaction.
///
/// `backup` runs once before an *existing* database (version > 0) is upgraded.
/// A database whose schema is newer than this build knows is refused untouched.
pub fn migrate(
    conn: &mut Connection,
    migrations: &[Migration],
    backup: Option<&dyn Fn(&Connection, u32) -> AppResult<()>>,
) -> AppResult<MigrationReport> {
    for (i, m) in migrations.iter().enumerate() {
        if m.version != i as u32 + 1 {
            return Err(AppError::Migration(format!(
                "migration list is not contiguous at '{}' (version {})",
                m.name, m.version
            )));
        }
    }

    let latest = latest_version(migrations);
    let from = current_version(conn)?;
    if from > latest {
        return Err(AppError::Migration(format!(
            "database schema v{from} is newer than this Zenv build supports (v{latest}); \
             refusing to modify it"
        )));
    }
    if from == latest {
        return Ok(MigrationReport { from, to: from });
    }
    if from > 0 {
        if let Some(backup) = backup {
            backup(conn, from)?;
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    for m in migrations.iter().filter(|m| m.version > from) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql).map_err(|e| {
            AppError::Migration(format!("v{} '{}' failed: {e}", m.version, m.name))
        })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![m.version, m.name, now_rfc3339()],
        )?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
        tracing::info!(version = m.version, name = m.name, "applied migration");
    }

    Ok(MigrationReport { from, to: latest })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn latest() -> u32 {
        latest_version(MIGRATIONS)
    }

    /// The real migrations plus one synthetic next version.
    fn with_next(name: &'static str, sql: &'static str) -> Vec<Migration> {
        let mut all = MIGRATIONS.to_vec();
        all.push(Migration { version: latest() + 1, name, sql });
        all
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn fresh_database_reaches_latest_without_backup() {
        let mut conn = Connection::open_in_memory().unwrap();
        let called = Cell::new(false);
        let hook = |_: &Connection, _: u32| {
            called.set(true);
            Ok(())
        };
        let report = migrate(&mut conn, MIGRATIONS, Some(&hook)).unwrap();
        assert_eq!(report, MigrationReport { from: 0, to: latest() });
        assert_eq!(current_version(&conn).unwrap(), latest());
        for t in ["installations", "services", "managed_files", "service_events", "sites"] {
            assert!(table_exists(&conn, t), "{t}");
        }
        assert!(!called.get(), "fresh databases need no backup");
    }

    #[test]
    fn v1_database_upgrades_to_v2_keeping_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..1], None).unwrap();
        conn.execute(
            "INSERT INTO installations VALUES ('i1','apache','2.4.67','x','C:/a','C:/a/bin/httpd.exe','external',1,'t','t','t')",
            [],
        )
        .unwrap();
        let backed_up = Cell::new(None);
        let hook = |_: &Connection, v: u32| {
            backed_up.set(Some(v));
            Ok(())
        };
        let report = migrate(&mut conn, MIGRATIONS, Some(&hook)).unwrap();
        assert_eq!(report.from, 1);
        assert_eq!(backed_up.get(), Some(1));
        assert!(table_exists(&conn, "sites"));
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM installations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v2_sites_survive_the_v3_table_rebuild() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..2], None).unwrap();
        conn.execute_batch(
            "INSERT INTO installations VALUES ('i1','apache','2.4.67','x','C:/a','C:/a/bin/httpd.exe','external',1,'t','t','t');
             INSERT INTO services VALUES ('s1','apache','A','i1','127.0.0.1',8080,0,'t','t');
             INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                                upstream_port, enabled, config_status, created_at, updated_at)
             VALUES ('x1','Shop','shop.test','C:/p','C:/p/public','static','s1','static',NULL,1,'applied','t','t');",
        )
        .unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        let (domain, engine): (String, String) = conn
            .query_row("SELECT domain, php_engine FROM sites WHERE id = 'x1'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((domain.as_str(), engine.as_str()), ("shop.test", "auto"));
        let enabled: bool = conn.query_row("SELECT enabled FROM installations WHERE id = 'i1'", [], |r| r.get(0)).unwrap();
        assert!(enabled);
        // A PHP site without a PHP installation is refused by the schema.
        let bad = conn.execute(
            "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                                enabled, config_status, created_at, updated_at)
             VALUES ('x2','B','b.test','C:/p','C:/p','php','s1','php',1,'pending','t','t')",
            [],
        );
        assert!(bad.is_err());
    }

    #[test]
    fn v4_lets_sites_follow_any_running_server() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..3], None).unwrap();
        conn.execute_batch(
            "INSERT INTO installations VALUES ('i1','apache','2.4.67','x','C:/a','C:/a/bin/httpd.exe','external',1,'t','t','t',1);
             INSERT INTO services VALUES ('s1','apache','A','i1','127.0.0.1',8080,0,'t','t');
             INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                                enabled, config_status, created_at, updated_at)
             VALUES ('x1','Shop','shop.test','C:/p','C:/p/public','static','s1','static',1,'applied','t','t');",
        )
        .unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        let sid: Option<String> = conn.query_row("SELECT service_id FROM sites WHERE id = 'x1'", [], |r| r.get(0)).unwrap();
        assert_eq!(sid.as_deref(), Some("s1"), "existing bindings kept");
        conn.execute(
            "INSERT INTO sites (id, name, domain, project_root, document_root, project_type, service_id, strategy,
                                enabled, config_status, created_at, updated_at)
             VALUES ('x2','Any','any.test','C:/p','C:/p','static',NULL,'static',1,'pending','t','t')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn rerunning_is_a_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        let report = migrate(&mut conn, MIGRATIONS, None).unwrap();
        assert_eq!(report, MigrationReport { from: latest(), to: latest() });
        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(applied, i64::from(latest()));
    }

    #[test]
    fn upgrade_backs_up_first_and_preserves_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        conn.execute(
            "INSERT INTO installations VALUES ('i1','apache','2.4.67','x','C:/a','C:/a/bin/httpd.exe','external',1,'t','t','t',1)",
            [],
        )
        .unwrap();

        let next = with_next("add_settings", "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);");
        let backed_up_from = Cell::new(None);
        let hook = |c: &Connection, v: u32| {
            // The backup must see the pre-upgrade state.
            assert!(!table_exists(c, "settings"));
            backed_up_from.set(Some(v));
            Ok(())
        };
        let report = migrate(&mut conn, &next, Some(&hook)).unwrap();
        assert_eq!(report, MigrationReport { from: latest(), to: latest() + 1 });
        assert_eq!(backed_up_from.get(), Some(latest()));
        assert!(table_exists(&conn, "settings"));
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM installations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn failed_backup_aborts_upgrade() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        let next = with_next("add_settings", "CREATE TABLE settings (key TEXT PRIMARY KEY);");
        let hook = |_: &Connection, _: u32| Err(AppError::Migration("disk full".into()));
        assert!(migrate(&mut conn, &next, Some(&hook)).is_err());
        assert_eq!(current_version(&conn).unwrap(), latest());
        assert!(!table_exists(&conn, "settings"));
    }

    #[test]
    fn failing_migration_rolls_back_atomically() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, MIGRATIONS, None).unwrap();
        let broken = with_next("broken", "CREATE TABLE half_done (id INTEGER); THIS IS NOT SQL;");
        let err = migrate(&mut conn, &broken, None).unwrap_err();
        assert!(matches!(err, AppError::Migration(_)));
        assert_eq!(current_version(&conn).unwrap(), latest());
        assert!(!table_exists(&conn, "half_done"));
    }

    #[test]
    fn newer_schema_is_refused_untouched() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = migrate(&mut conn, MIGRATIONS, None).unwrap_err();
        assert!(err.to_string().contains("newer than this Zenv build"));
        assert_eq!(current_version(&conn).unwrap(), 99);
    }

    #[test]
    fn non_contiguous_list_is_rejected() {
        const GAP: &[Migration] = &[Migration { version: 2, name: "gap", sql: "" }];
        let mut conn = Connection::open_in_memory().unwrap();
        assert!(migrate(&mut conn, GAP, None).is_err());
    }
}
