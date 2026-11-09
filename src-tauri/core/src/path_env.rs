//! Putting one version of a language or tool on the user's PATH, so plain
//! terminals (and editors) use it.
//!
//! - Only the user PATH is written (no administrator rights needed); the
//!   computer-wide PATH is only read.
//! - One version per component: choosing another version replaces the folders
//!   Zenv added before.
//! - Before adding, both PATHs are checked for other folders that hold the
//!   same programs (php.exe from C:\xampp\php, python.exe from C:\Python312,
//!   ...). Windows uses the first match, and the computer-wide PATH comes
//!   first, so Zenv refuses and names the folders to remove instead of
//!   silently losing. Microsoft Store aliases (WindowsApps) do not count.

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::runtime_catalog::{self, Category, ComponentId};
use crate::runtime_manager::{self, Installation};
use crate::util::now_rfc3339;

/// Programs that identify a component on PATH.
pub fn programs(component: ComponentId) -> &'static [&'static str] {
    use ComponentId::*;
    match component {
        Php => &["php.exe"],
        Python => &["python.exe"],
        Node => &["node.exe"],
        Go => &["go.exe"],
        Ruby => &["ruby.exe"],
        Rust => &["cargo.exe", "rustc.exe"],
        Mysql => &["mysql.exe", "mysqld.exe"],
        Postgresql => &["psql.exe", "pg_ctl.exe"],
        Redis => &["redis-cli.exe", "redis-server.exe"],
        _ => &[],
    }
}

/// Languages and databases can go on PATH (web servers run from Zenv).
pub fn eligible(component: ComponentId) -> bool {
    !programs(component).is_empty()
        && matches!(runtime_catalog::info(component).category, Category::Language | Category::Database | Category::Cache)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathConflict {
    /// The folder as written in PATH.
    pub entry: String,
    /// "system" (all users, needs administrator rights to change) or "user".
    pub scope: &'static str,
    /// Which program it holds, e.g. "php.exe".
    pub program: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStatus {
    pub component: ComponentId,
    pub eligible: bool,
    /// The version Zenv put on PATH (still there), if any.
    pub installation_id: Option<String>,
    /// Other folders on PATH that hold the same programs.
    pub conflicts: Vec<PathConflict>,
}

/// `%VAR%` expanded from this process's environment.
fn expand(entry: &str) -> String {
    let mut out = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        let name = &after[..end];
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn split(path: &str) -> Vec<String> {
    path.split(';').map(str::trim).filter(|e| !e.is_empty()).map(str::to_string).collect()
}

fn same_dir(a: &str, b: &Path) -> bool {
    let norm = |s: &str| s.trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    norm(&expand(a)) == norm(&b.display().to_string())
}

fn is_store_alias(dir: &Path) -> bool {
    dir.display().to_string().to_ascii_lowercase().contains(r"\microsoft\windowsapps")
}

/// Folders on PATH (outside `ours`) that hold one of `component`'s programs.
pub fn conflicts(machine: &str, user: &str, component: ComponentId, ours: &[PathBuf]) -> Vec<PathConflict> {
    let mut out = Vec::new();
    for (scope, value) in [("system", machine), ("user", user)] {
        for entry in split(value) {
            if ours.iter().any(|o| same_dir(&entry, o)) {
                continue;
            }
            let dir = PathBuf::from(expand(&entry));
            if is_store_alias(&dir) {
                continue;
            }
            if let Some(program) = programs(component).iter().find(|p| dir.join(p).is_file()) {
                out.push(PathConflict { entry, scope, program: (*program).to_string() });
            }
        }
    }
    out
}

/// `user` with `remove` taken out and `add` put first.
pub fn rewrite(user: &str, remove: &[PathBuf], add: &[PathBuf]) -> String {
    let kept: Vec<String> = split(user).into_iter().filter(|e| !remove.iter().chain(add).any(|r| same_dir(e, r))).collect();
    add.iter().map(|p| p.display().to_string()).chain(kept).collect::<Vec<_>>().join(";")
}

impl Core {
    fn path_choice(&self, component: ComponentId) -> AppResult<Option<(String, Vec<PathBuf>)>> {
        self.db.read(|c| {
            Ok(c.query_row("SELECT installation_id, dirs FROM path_choices WHERE component = ?1", [component.as_str()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?)
        })
        .map(|row| row.map(|(id, dirs)| (id, dirs.split('\n').filter(|d| !d.is_empty()).map(PathBuf::from).collect())))
    }

    pub fn path_status(&self, component: ComponentId) -> AppResult<PathStatus> {
        if !eligible(component) {
            return Ok(PathStatus { component, eligible: false, installation_id: None, conflicts: vec![] });
        }
        let machine = crate::platform::read_path_var(true)?;
        let user = crate::platform::read_path_var(false)?;
        let choice = self.path_choice(component)?;
        // Still on PATH? (the user may have removed it by hand)
        let current = choice.filter(|(_, dirs)| dirs.iter().all(|d| split(&user).iter().any(|e| same_dir(e, d))));
        let ours = current.as_ref().map(|(_, d)| d.clone()).unwrap_or_default();
        Ok(PathStatus {
            component,
            eligible: true,
            installation_id: current.map(|(id, _)| id),
            conflicts: conflicts(&machine, &user, component, &ours),
        })
    }

    /// Puts `installation_id` on the user PATH (replacing the version Zenv put
    /// there before), unless another folder on PATH already holds the same
    /// programs.
    pub fn path_add(&self, installation_id: &str) -> AppResult<PathStatus> {
        let inst: Installation = runtime_manager::get(&self.db, installation_id)?;
        if !eligible(inst.component) {
            return Err(AppError::Validation(format!("{} is run by Zenv and does not go on PATH", runtime_catalog::info(inst.component).name)));
        }
        let dirs = crate::runtime_env::path_dirs(&inst);
        let machine = crate::platform::read_path_var(true)?;
        let user = crate::platform::read_path_var(false)?;
        let previous = self.path_choice(inst.component)?.map(|(_, d)| d).unwrap_or_default();
        let mut ours = previous.clone();
        ours.extend(dirs.iter().cloned());
        let found = conflicts(&machine, &user, inst.component, &ours);
        if !found.is_empty() {
            let list: Vec<String> = found.iter().map(|c| format!("{} ({} PATH, has {})", c.entry, c.scope, c.program)).collect();
            return Err(AppError::Conflict(format!(
                "{} is already on PATH from another folder: {}. Remove {} from Environment Variables first (System Properties → Environment Variables), then try again.",
                runtime_catalog::info(inst.component).name,
                list.join("; "),
                if found.len() == 1 { "it" } else { "them" }
            )));
        }
        crate::platform::write_user_path(&rewrite(&user, &previous, &dirs))?;
        let joined: String = dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join("\n");
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO path_choices (component, installation_id, dirs, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(component) DO UPDATE SET installation_id = ?2, dirs = ?3, updated_at = ?4",
                params![inst.component.as_str(), inst.id, joined, now_rfc3339()],
            )?;
            Ok(())
        })?;
        crate::platform::broadcast_environment_change();
        tracing::info!(component = %inst.component, version = %inst.version, "added to the user PATH");
        self.path_status(inst.component)
    }

    /// Takes the version Zenv put on PATH for `component` off it again.
    pub fn path_remove(&self, component: ComponentId) -> AppResult<PathStatus> {
        if let Some((_, dirs)) = self.path_choice(component)? {
            let user = crate::platform::read_path_var(false)?;
            crate::platform::write_user_path(&rewrite(&user, &dirs, &[]))?;
            self.db.write(|tx| {
                tx.execute("DELETE FROM path_choices WHERE component = ?1", [component.as_str()])?;
                Ok(())
            })?;
            crate::platform::broadcast_environment_change();
        }
        self.path_status(component)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_same_program_in_other_folders() {
        let dir = tempfile::tempdir().unwrap();
        let (xampp, ours, apps, other) = (dir.path().join("xampp-php"), dir.path().join("zenv-php"), dir.path().join("Microsoft").join("WindowsApps"), dir.path().join("tools"));
        for d in [&xampp, &ours, &apps, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(xampp.join("php.exe"), "").unwrap();
        std::fs::write(ours.join("php.exe"), "").unwrap();
        std::fs::write(apps.join("python.exe"), "").unwrap();
        let machine = format!("C:\\Windows;{}", xampp.display());
        let user = format!("{};{};{}", ours.display(), apps.display(), other.display());
        let found = conflicts(&machine, &user, ComponentId::Php, std::slice::from_ref(&ours));
        assert_eq!(found, vec![PathConflict { entry: xampp.display().to_string(), scope: "system", program: "php.exe".into() }]);
        assert!(conflicts(&machine, &user, ComponentId::Python, &[]).is_empty(), "Store aliases do not count");
    }

    #[test]
    fn replaces_the_previous_version_and_keeps_the_rest() {
        let old = PathBuf::from(r"C:\zenv\bin\php\8.4.8");
        let new = PathBuf::from(r"C:\zenv\bin\php\8.5.6");
        let user = r"%USERPROFILE%\.cargo\bin;C:\zenv\bin\php\8.4.8\;C:\tools";
        assert_eq!(rewrite(user, std::slice::from_ref(&old), std::slice::from_ref(&new)), r"C:\zenv\bin\php\8.5.6;%USERPROFILE%\.cargo\bin;C:\tools");
        assert_eq!(rewrite(r"C:\zenv\bin\php\8.5.6;C:\tools", std::slice::from_ref(&new), &[]), r"C:\tools");
    }

    #[test]
    fn expands_variables() {
        std::env::set_var("ZENV_TEST_HOME", r"C:\Users\x");
        assert_eq!(expand(r"%ZENV_TEST_HOME%\bin;%NOPE_ZENV%"), r"C:\Users\x\bin;%NOPE_ZENV%");
    }
}
