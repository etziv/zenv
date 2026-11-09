use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult, IoContext};

/// Default Zenv home: everything Zenv installs and manages lives here, like
/// `C:\laragon`. Projects can live anywhere.
pub const DEFAULT_HOME: &str = r"C:\zenv";

/// The Zenv home and its layout:
///
/// ```text
/// C:\zenv\
///   bin\<component>\<version>\   versions installed by Zenv
///   data\                        database and cache data folders
///   etc\                         generated server configuration
///   logs\  dev\  backups\  downloads\
///   zenv.db                      metadata
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub root: PathBuf,
    pub db_file: PathBuf,
    pub bin_dir: PathBuf,
    pub data_dir: PathBuf,
    pub generated_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// Default pages of the web servers (one folder per server).
    pub dev_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        AppPaths {
            db_file: root.join("zenv.db"),
            bin_dir: root.join("bin"),
            data_dir: root.join("data"),
            generated_dir: root.join("etc"),
            logs_dir: root.join("logs"),
            dev_dir: root.join("dev"),
            backups_dir: root.join("backups"),
            downloads_dir: root.join("downloads"),
            root,
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        // Earlier versions named the web servers' default pages folder `www`, like Laragon.
        crate::layout::migrate_folders(&self.root)?;
        for dir in [
            &self.root,
            &self.bin_dir,
            &self.data_dir,
            &self.generated_dir,
            &self.logs_dir,
            &self.dev_dir,
            &self.backups_dir,
            &self.downloads_dir,
        ] {
            std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// Where Zenv installs a version of a component.
    pub fn install_dir(&self, component: &str, version: &str) -> PathBuf {
        self.bin_dir.join(component).join(version)
    }

    pub fn service_generated_dir(&self, service_id: &str) -> PathBuf {
        self.generated_dir.join(service_id)
    }

    pub fn service_logs_dir(&self, service_id: &str) -> PathBuf {
        self.logs_dir.join("services").join(service_id)
    }

    pub fn app_log_file(&self) -> PathBuf {
        self.logs_dir.join("zenv.log")
    }
}

/// Written by the installer next to `zenv.exe`: the folder Zenv was installed
/// into is also its home (`C:\zenv` by default).
pub const INSTALL_MARKER: &str = "zenv-home.txt";

/// Chooses the Zenv home: `ZENV_HOME`, else a `home.txt` pointer in the
/// per-user app folder, else the folder the installer put Zenv in, else
/// `C:\zenv`.
pub fn resolve_home(app_local_dir: &Path) -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    resolve_home_from(std::env::var_os("ZENV_HOME"), app_local_dir, exe_dir.as_deref())
}

fn resolve_home_from(env: Option<std::ffi::OsString>, app_local_dir: &Path, exe_dir: Option<&Path>) -> PathBuf {
    if let Some(h) = env.filter(|h| !h.is_empty()) {
        return PathBuf::from(h);
    }
    if let Ok(text) = std::fs::read_to_string(app_local_dir.join("home.txt")) {
        let t = text.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    if let Some(dir) = exe_dir.filter(|d| d.join(INSTALL_MARKER).is_file()) {
        return dir.to_path_buf();
    }
    PathBuf::from(DEFAULT_HOME)
}

/// Earlier builds kept metadata in the per-user app folder. If the home has no
/// database yet, copy that one in (consistently, via `VACUUM INTO`). The old
/// file is left in place.
pub fn adopt_legacy_database(app_local_dir: &Path, home: &AppPaths) -> AppResult<Option<PathBuf>> {
    let legacy = app_local_dir.join("zenv.db");
    if home.db_file.exists() || !legacy.is_file() || legacy == home.db_file {
        return Ok(None);
    }
    let conn = rusqlite::Connection::open(&legacy)?;
    let target = home.db_file.to_str().ok_or_else(|| AppError::invalid_path(&home.db_file, "not valid UTF-8"))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    tracing::info!(from = %legacy.display(), to = %home.db_file.display(), "metadata copied into the Zenv home");
    Ok(Some(legacy))
}

/// Forward-slash form for server configs (Apache and Nginx accept `/` on Windows
/// and treat `\` as an escape in some contexts).
pub fn to_config_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_www_folder_becomes_dev() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("www").join("svc")).unwrap();
        std::fs::write(dir.path().join("www").join("svc").join("index.html"), "x").unwrap();
        let paths = AppPaths::new(dir.path());
        paths.ensure().unwrap();
        assert!(!dir.path().join("www").exists());
        assert!(paths.dev_dir.join("svc").join("index.html").is_file());
    }

    #[test]
    fn installed_folder_is_the_home() {
        let local = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        let dev = tempfile::tempdir().unwrap();
        std::fs::write(install.path().join(INSTALL_MARKER), "").unwrap();
        // Installed: the folder holding zenv.exe and the marker.
        assert_eq!(resolve_home_from(None, local.path(), Some(install.path())), install.path());
        // A development build (no marker) uses the default.
        assert_eq!(resolve_home_from(None, local.path(), Some(dev.path())), PathBuf::from(DEFAULT_HOME));
        // An explicit pointer or ZENV_HOME still wins.
        std::fs::write(local.path().join("home.txt"), r"E:\elsewhere").unwrap();
        assert_eq!(resolve_home_from(None, local.path(), Some(install.path())), PathBuf::from(r"E:\elsewhere"));
        assert_eq!(resolve_home_from(Some(r"F:\env".into()), local.path(), Some(install.path())), PathBuf::from(r"F:\env"));
    }

    #[test]
    fn layout_lives_under_one_home() {
        let p = AppPaths::new(r"C:\zenv");
        assert_eq!(p.install_dir("go", "1.27.0"), PathBuf::from(r"C:\zenv\bin\go\1.27.0"));
        assert_eq!(p.data_dir, PathBuf::from(r"C:\zenv\data"));
        assert_eq!(p.db_file, PathBuf::from(r"C:\zenv\zenv.db"));
    }

    #[test]
    fn home_pointer_file_overrides_the_default() {
        let dir = tempfile::tempdir().unwrap();
        if std::env::var_os("ZENV_HOME").is_none() {
            assert_eq!(resolve_home(dir.path()), PathBuf::from(DEFAULT_HOME));
            std::fs::write(dir.path().join("home.txt"), "D:\\dev\\zenv\n").unwrap();
            assert_eq!(resolve_home(dir.path()), PathBuf::from(r"D:\dev\zenv"));
        }
    }

    #[test]
    fn legacy_metadata_is_copied_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        crate::storage::Db::open(&legacy.join("zenv.db"), &legacy.join("b")).unwrap();
        let home = AppPaths::new(dir.path().join("home"));
        home.ensure().unwrap();
        assert!(adopt_legacy_database(&legacy, &home).unwrap().is_some());
        assert!(home.db_file.is_file() && legacy.join("zenv.db").is_file());
        assert!(adopt_legacy_database(&legacy, &home).unwrap().is_none(), "never overwrites the home database");
    }
}
