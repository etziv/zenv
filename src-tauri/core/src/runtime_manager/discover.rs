//! Finds existing installations without registering them. Scans are bounded
//! (depth and entry count) and skip directories that never contain binaries.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runtime_catalog::ComponentId;

use super::probe::{self, Located};

const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 50_000;

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "src", "pkg", "test", "tests", "doc", "docs", "manual", "htdocs",
    "icons", "include", "share", "lib", "site-packages", "data", "logs", "tmp", "cache",
    "modules", "ext", "locale", "locales",
];

/// Executable file name per component (the last segment of its probe paths).
fn exe_names() -> Vec<(ComponentId, &'static str)> {
    ComponentId::ALL
        .into_iter()
        .map(|c| {
            let rel = probe::spec(c).executables[0];
            (c, rel.rsplit('/').next().unwrap_or(rel))
        })
        .collect()
}

/// Directories worth scanning on this machine.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [
        r"C:\FlyEnv-Data\app",
        r"C:\laragon\bin",
        r"C:\xampp",
        r"C:\Apache24",
        r"C:\nginx",
        r"C:\Program Files\PostgreSQL",
        r"C:\Program Files\MySQL",
        r"C:\Program Files\Go",
        r"C:\Program Files\Redis",
        r"C:\tools",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    roots.push(PathBuf::from(r"C:\Program Files\nodejs"));
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(Path::new(&local).join("Programs").join("Python"));
    }
    // nvm-windows keeps one folder per Node version.
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(Path::new(&appdata).join("nvm"));
    }
    if let Some(nvm) = std::env::var_os("NVM_HOME") {
        roots.push(PathBuf::from(nvm));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        roots.push(Path::new(&profile).join(".cargo").join("bin"));
    }
    // C:\Ruby34-x64, C:\php-8.3 and similar top-level installs.
    if let Ok(entries) = std::fs::read_dir(r"C:\") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if ["ruby", "php", "nginx", "caddy", "redis", "python", "go"].iter().any(|p| name.starts_with(p)) {
                roots.push(e.path());
            }
        }
    }
    roots.retain(|r| r.is_dir());
    roots
}

/// App Execution Alias stubs (e.g. `WindowsApps\python.exe`) can open the
/// Microsoft Store when run; never probe them.
fn is_app_alias(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

pub fn path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).filter(|d| !is_app_alias(d)).collect())
        .unwrap_or_default()
}

/// Collects candidate executables keyed by canonical path.
pub fn find_candidates(
    roots: &[PathBuf],
    path_dirs: &[PathBuf],
    only: Option<ComponentId>,
) -> Vec<(ComponentId, PathBuf)> {
    let names: Vec<_> = exe_names().into_iter().filter(|(c, _)| only.is_none_or(|o| o == *c)).collect();
    let mut found: BTreeMap<String, (ComponentId, PathBuf)> = BTreeMap::new();
    let mut add = |component: ComponentId, path: PathBuf| {
        if is_app_alias(&path) {
            return;
        }
        let canonical = dunce::canonicalize(&path).unwrap_or(path);
        found.entry(canonical.to_string_lossy().to_ascii_lowercase()).or_insert((component, canonical));
    };

    for dir in path_dirs {
        for (component, exe) in &names {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                add(*component, candidate);
            }
        }
    }

    let mut budget = MAX_ENTRIES;
    for root in roots {
        walk(root, 0, &names, &mut budget, &mut add);
    }
    found.into_values().collect()
}

fn walk(
    dir: &Path,
    depth: usize,
    names: &[(ComponentId, &str)],
    budget: &mut usize,
    add: &mut impl FnMut(ComponentId, PathBuf),
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(ft) = entry.file_type() else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if ft.is_file() {
            if let Some((component, _)) = names.iter().find(|(_, exe)| exe.eq_ignore_ascii_case(&name)) {
                add(*component, entry.path());
            }
        } else if ft.is_dir()
            && depth < MAX_DEPTH
            && !SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(&name))
        {
            walk(&entry.path(), depth + 1, names, budget, add);
        }
    }
}

pub struct ProbeResult {
    pub component: ComponentId,
    pub located: Option<Located>,
    pub version: Result<probe::VersionInfo, String>,
    pub executable: PathBuf,
}

/// Probes candidates in parallel (each probe is a short external process).
pub fn probe_all(candidates: Vec<(ComponentId, PathBuf)>) -> Vec<ProbeResult> {
    std::thread::scope(|s| {
        let handles: Vec<_> = candidates
            .into_iter()
            .map(|(component, executable)| {
                s.spawn(move || {
                    let located = probe::locate(component, &executable).ok();
                    let version = probe::detect_version(component, &executable).map_err(|e| e.to_string());
                    ProbeResult { component, located, version, executable }
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn finds_nested_installs_and_skips_noise() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        touch(&root.join("apache-2.4.67").join("Apache24").join("bin").join("httpd.exe"));
        touch(&root.join("php-8.4.21").join("php.exe"));
        touch(&root.join("node_modules").join("x").join("php.exe"));
        touch(&root.join("a").join("b").join("c").join("d").join("e").join("redis-server.exe"));

        let found = find_candidates(&[root.clone()], &[], None);
        let components: Vec<_> = found.iter().map(|(c, _)| *c).collect();
        assert!(components.contains(&ComponentId::Apache));
        assert!(components.contains(&ComponentId::Php));
        assert_eq!(found.len(), 2, "node_modules and too-deep paths are skipped: {found:?}");

        let only_php = find_candidates(&[root], &[], Some(ComponentId::Php));
        assert_eq!(only_php.len(), 1);
    }

    #[test]
    fn path_and_root_duplicates_collapse() {
        let dir = tempfile::tempdir().unwrap();
        let php_dir = dir.path().join("php");
        touch(&php_dir.join("php.exe"));
        let found = find_candidates(&[dir.path().to_path_buf()], &[php_dir], Some(ComponentId::Php));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn windows_apps_aliases_are_ignored() {
        assert!(is_app_alias(Path::new(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\python.exe")));
        assert!(!is_app_alias(Path::new(r"C:\Python314\python.exe")));
    }
}
