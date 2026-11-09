//! Installation registry. Registering records metadata about an existing
//! installation; unregistering removes only that record. Zenv never
//! deletes installation directories it did not create.

pub mod discover;
pub mod probe;

use std::path::{Path, PathBuf};
use std::str::FromStr;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::runtime_catalog::ComponentId;
use crate::storage::Db;
use crate::util::{new_id, now_rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    /// Installed by the user or another tool; Zenv only references it.
    External,
    /// Installed by a Zenv provider (none exist yet; see M6).
    Managed,
}

impl InstallSource {
    fn as_str(self) -> &'static str {
        match self {
            InstallSource::External => "external",
            InstallSource::Managed => "managed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub component: ComponentId,
    pub version: String,
    pub version_detail: String,
    pub root_path: PathBuf,
    pub executable_path: PathBuf,
    pub source: InstallSource,
    pub is_default: bool,
    /// Switched-off installations are kept but not offered or used.
    pub enabled: bool,
    pub executable_exists: bool,
    pub detected_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredInstallation {
    pub component: ComponentId,
    pub executable_path: PathBuf,
    pub root_path: Option<PathBuf>,
    pub version: Option<String>,
    pub version_detail: Option<String>,
    pub error: Option<String>,
    pub registered_id: Option<String>,
}

const COLUMNS: &str = "id, component, version, version_detail, root_path, executable_path, source, \
                       is_default, detected_at, created_at, updated_at, enabled";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Installation> {
    let component: String = row.get(1)?;
    let source: String = row.get(6)?;
    let executable_path = PathBuf::from(row.get::<_, String>(5)?);
    Ok(Installation {
        id: row.get(0)?,
        component: ComponentId::from_str(&component).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        version: row.get(2)?,
        version_detail: row.get(3)?,
        root_path: PathBuf::from(row.get::<_, String>(4)?),
        executable_exists: executable_path.is_file(),
        executable_path,
        source: if source == "managed" { InstallSource::Managed } else { InstallSource::External },
        is_default: row.get(7)?,
        detected_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        enabled: row.get(11)?,
    })
}

fn path_str(p: &Path) -> AppResult<&str> {
    p.to_str().ok_or_else(|| AppError::invalid_path(p, "path is not valid UTF-8"))
}

/// Numeric version parts, so 8.10 sorts above 8.9.
pub fn version_key(version: &str) -> Vec<u32> {
    version.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

/// Installations grouped by component, newest version first.
pub fn list(db: &Db, component: Option<ComponentId>) -> AppResult<Vec<Installation>> {
    let mut list = db.read(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {COLUMNS} FROM installations
             WHERE ?1 IS NULL OR component = ?1
             ORDER BY component, created_at DESC"
        ))?;
        let rows = stmt.query_map([component.map(|c| c.as_str())], from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;
    list.sort_by(|a, b| {
        a.component
            .as_str()
            .cmp(b.component.as_str())
            .then_with(|| version_key(&b.version).cmp(&version_key(&a.version)))
            .then_with(|| b.is_default.cmp(&a.is_default))
    });
    Ok(list)
}

pub fn get(db: &Db, id: &str) -> AppResult<Installation> {
    db.read(|c| {
        c.query_row(&format!("SELECT {COLUMNS} FROM installations WHERE id = ?1"), [id], from_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound { entity: "installation", id: id.to_string() })
    })
}

/// Validates the path, probes the real executable, then records it. The
/// first installation of a component becomes its default.
pub fn register(db: &Db, component: ComponentId, path: &Path) -> AppResult<Installation> {
    let located = probe::locate(component, path)?;
    let version = probe::detect_version(component, &located.executable)?;
    insert(db, component, &located, &version, InstallSource::External)
}

pub fn insert(
    db: &Db,
    component: ComponentId,
    located: &probe::Located,
    version: &probe::VersionInfo,
    source: InstallSource,
) -> AppResult<Installation> {
    let id = new_id();
    let now = now_rfc3339();
    let root = path_str(&located.root)?;
    let exe = path_str(&located.executable)?;
    db.write(|tx| {
        let exists: Option<String> = tx
            .query_row(
                "SELECT id FROM installations WHERE component = ?1 AND executable_path = ?2 COLLATE NOCASE",
                params![component.as_str(), exe],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(AppError::Conflict(format!("{exe} is already registered")));
        }
        let has_default: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM installations WHERE component = ?1 AND is_default = 1)",
            [component.as_str()],
            |r| r.get(0),
        )?;
        tx.execute(
            &format!("INSERT INTO installations ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9, 1)"),
            params![
                id,
                component.as_str(),
                version.version,
                version.detail,
                root,
                exe,
                source.as_str(),
                !has_default,
                now
            ],
        )?;
        Ok(())
    })?;
    get(db, &id)
}

/// Removes the registration only. Refused while a service still uses it.
pub fn unregister(db: &Db, id: &str) -> AppResult<()> {
    db.write(|tx| {
        let users: Vec<String> = tx
            .prepare("SELECT name FROM services WHERE installation_id = ?1 ORDER BY name")?
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if !users.is_empty() {
            return Err(AppError::Conflict(format!(
                "installation is used by service(s): {}; delete those services first",
                users.join(", ")
            )));
        }
        let sites: Vec<String> = tx
            .prepare("SELECT domain FROM sites WHERE runtime_installation_id = ?1 ORDER BY domain LIMIT 5")?
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if !sites.is_empty() {
            return Err(AppError::Conflict(format!(
                "installation is used by site(s): {}; switch them to another version first",
                sites.join(", ")
            )));
        }
        let row: Option<(String, bool)> = tx
            .query_row("SELECT component, is_default FROM installations WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let Some((component, was_default)) = row else {
            return Err(AppError::NotFound { entity: "installation", id: id.to_string() });
        };
        tx.execute("DELETE FROM installations WHERE id = ?1", [id])?;
        if was_default {
            // Promote the most recently registered remaining installation.
            tx.execute(
                "UPDATE installations SET is_default = 1 WHERE id = (
                     SELECT id FROM installations WHERE component = ?1 ORDER BY created_at DESC LIMIT 1)",
                [component],
            )?;
        }
        Ok(())
    })
}

pub fn set_default(db: &Db, id: &str) -> AppResult<Installation> {
    let inst = get(db, id)?;
    db.write(|tx| {
        tx.execute(
            "UPDATE installations SET is_default = 0 WHERE component = ?1 AND is_default = 1",
            [inst.component.as_str()],
        )?;
        tx.execute(
            "UPDATE installations SET is_default = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_rfc3339()],
        )?;
        Ok(())
    })?;
    get(db, id)
}

/// Turns an installation on or off without forgetting it.
pub fn set_enabled(db: &Db, id: &str, enabled: bool) -> AppResult<Installation> {
    get(db, id)?;
    db.write(|tx| {
        tx.execute(
            "UPDATE installations SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, enabled, now_rfc3339()],
        )?;
        Ok(())
    })?;
    get(db, id)
}

/// Re-probes the executable (e.g. after the user upgraded it in place).
pub fn refresh(db: &Db, id: &str) -> AppResult<Installation> {
    let inst = get(db, id)?;
    let version = probe::detect_version(inst.component, &inst.executable_path)?;
    let now = now_rfc3339();
    db.write(|tx| {
        tx.execute(
            "UPDATE installations SET version = ?2, version_detail = ?3, detected_at = ?4, updated_at = ?4
             WHERE id = ?1",
            params![id, version.version, version.detail, now],
        )?;
        Ok(())
    })?;
    get(db, id)
}

/// Scans PATH, well-known install locations and `extra_roots` (the Zenv
/// home's `bin`) for installations.
pub fn discover(db: &Db, component: Option<ComponentId>, extra_roots: &[PathBuf]) -> AppResult<Vec<DiscoveredInstallation>> {
    let registered = list(db, component)?;
    let mut roots = extra_roots.to_vec();
    roots.extend(discover::default_roots());
    let candidates = discover::find_candidates(&roots, &discover::path_entries(), component);
    let mut out: Vec<DiscoveredInstallation> = discover::probe_all(candidates)
        .into_iter()
        .map(|r| {
            let registered_id = registered
                .iter()
                .find(|i| i.component == r.component && i.executable_path.as_os_str().eq_ignore_ascii_case(r.executable.as_os_str()))
                .map(|i| i.id.clone());
            let (version, version_detail, error) = match r.version {
                Ok(v) => (Some(v.version), Some(v.detail), None),
                Err(e) => (None, None, Some(e)),
            };
            DiscoveredInstallation {
                component: r.component,
                root_path: r.located.map(|l| l.root),
                executable_path: r.executable,
                version,
                version_detail,
                error,
                registered_id,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (a.component.as_str(), &a.executable_path).cmp(&(b.component.as_str(), &b.executable_path))
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(db: &Db, component: ComponentId, exe: &str, version: &str) -> Installation {
        let located = probe::Located { root: PathBuf::from(r"C:\fake"), executable: PathBuf::from(exe) };
        let v = probe::VersionInfo { version: version.into(), detail: format!("{component} {version}") };
        insert(db, component, &located, &v, InstallSource::External).unwrap()
    }

    #[test]
    fn first_install_becomes_default_and_duplicates_conflict() {
        let db = Db::open_in_memory().unwrap();
        let a = fake(&db, ComponentId::Php, r"C:\php84\php.exe", "8.4.21");
        let b = fake(&db, ComponentId::Php, r"C:\php83\php.exe", "8.3.10");
        assert!(a.is_default);
        assert!(!b.is_default);
        let dup = insert(
            &db,
            ComponentId::Php,
            &probe::Located { root: "C:/x".into(), executable: r"C:\PHP84\PHP.EXE".into() },
            &probe::VersionInfo { version: "8.4.21".into(), detail: String::new() },
            InstallSource::External,
        );
        assert!(matches!(dup, Err(AppError::Conflict(_))), "case-insensitive duplicate");
    }

    #[test]
    fn set_default_is_exclusive_per_component() {
        let db = Db::open_in_memory().unwrap();
        let a = fake(&db, ComponentId::Php, r"C:\php84\php.exe", "8.4.21");
        let b = fake(&db, ComponentId::Php, r"C:\php83\php.exe", "8.3.10");
        let go = fake(&db, ComponentId::Go, r"C:\go\bin\go.exe", "1.27.0");
        set_default(&db, &b.id).unwrap();
        assert!(!get(&db, &a.id).unwrap().is_default);
        assert!(get(&db, &b.id).unwrap().is_default);
        assert!(get(&db, &go.id).unwrap().is_default, "other components unaffected");
    }

    #[test]
    fn unregister_promotes_a_new_default_and_never_touches_files() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("php.exe");
        std::fs::write(&exe, b"x").unwrap();
        let db = Db::open_in_memory().unwrap();
        let a = fake(&db, ComponentId::Php, exe.to_str().unwrap(), "8.4.21");
        let b = fake(&db, ComponentId::Php, r"C:\php83\php.exe", "8.3.10");
        assert!(a.executable_exists);
        unregister(&db, &a.id).unwrap();
        assert!(exe.is_file(), "files remain after unregistering");
        assert!(get(&db, &b.id).unwrap().is_default);
        assert!(matches!(unregister(&db, &a.id), Err(AppError::NotFound { .. })));
    }

    #[test]
    fn unregister_is_refused_while_a_service_uses_it() {
        let db = Db::open_in_memory().unwrap();
        let a = fake(&db, ComponentId::Apache, r"C:\Apache24\bin\httpd.exe", "2.4.67");
        db.write(|tx| {
            tx.execute(
                "INSERT INTO services (id, component, name, installation_id, listen_address, http_port, autostart, created_at, updated_at) VALUES ('s1','apache','Main Apache',?1,'127.0.0.1',8080,0,'t','t')",
                [&a.id],
            )?;
            Ok(())
        })
        .unwrap();
        let err = unregister(&db, &a.id).unwrap_err();
        assert!(err.to_string().contains("Main Apache"), "{err}");
    }

    #[test]
    fn list_filters_by_component() {
        let db = Db::open_in_memory().unwrap();
        fake(&db, ComponentId::Php, r"C:\php84\php.exe", "8.4.21");
        fake(&db, ComponentId::Go, r"C:\go\bin\go.exe", "1.27.0");
        assert_eq!(list(&db, None).unwrap().len(), 2);
        assert_eq!(list(&db, Some(ComponentId::Go)).unwrap().len(), 1);
    }
}
