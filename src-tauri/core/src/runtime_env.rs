//! Per-process environments for specific versions.
//!
//! Zenv never edits the system PATH. A terminal or project process gets the
//! chosen versions' folders prepended to its own PATH (plus variables such as
//! GOROOT or PHPRC), so any number of versions coexist without conflict.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::Installation;

/// Folders to put on PATH for an installation, most specific first.
pub fn path_dirs(install: &Installation) -> Vec<PathBuf> {
    let root = &install.root_path;
    let exe_dir = install.executable_path.parent().map(PathBuf::from).unwrap_or_else(|| root.clone());
    match install.component {
        ComponentId::Python => vec![root.clone(), root.join("Scripts")],
        // Zenv-managed Rust: rustup's proxies (cargo, rustc, rustup) in its own CARGO_HOME.
        ComponentId::Rust if is_managed_rust(install) => vec![root.join("cargo").join("bin"), exe_dir],
        _ => vec![exe_dir],
    }
}

/// Extra variables an installation needs to behave as itself.
pub fn variables(install: &Installation) -> Vec<(&'static str, OsString)> {
    match install.component {
        ComponentId::Go => vec![("GOROOT", install.root_path.clone().into_os_string())],
        ComponentId::Php => vec![("PHPRC", install.root_path.clone().into_os_string())],
        // Points rustup at Zenv's own homes, so the developer's ~/.rustup and
        // ~/.cargo are neither used nor changed.
        ComponentId::Rust if is_managed_rust(install) => vec![
            ("RUSTUP_HOME", install.root_path.join("rustup").into_os_string()),
            ("CARGO_HOME", install.root_path.join("cargo").into_os_string()),
            ("RUSTUP_TOOLCHAIN", install.version.clone().into()),
        ],
        _ => vec![],
    }
}

/// A Rust installed by Zenv keeps its toolchains under `<root>\rustup`.
fn is_managed_rust(install: &Installation) -> bool {
    install.executable_path.starts_with(install.root_path.join("rustup"))
}

/// Environment for a process using `installs`: their folders first, then the
/// inherited PATH.
pub fn build(installs: &[Installation], inherited_path: Option<OsString>) -> Vec<(OsString, OsString)> {
    let mut dirs: Vec<PathBuf> = installs.iter().flat_map(path_dirs).collect();
    if let Some(p) = inherited_path {
        dirs.extend(std::env::split_paths(&p));
    }
    let mut env: Vec<(OsString, OsString)> = installs
        .iter()
        .flat_map(variables)
        .map(|(k, v)| (OsString::from(k), v))
        .collect();
    let path = std::env::join_paths(dirs).unwrap_or_default();
    env.push(("PATH".into(), path));
    env
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::web_server::tests_support::fake_install;

    #[test]
    fn chosen_versions_come_first_with_their_variables() {
        let mut go = fake_install(ComponentId::Go, "1.27.0", Path::new(r"C:\zenv\bin\go\1.27.0"));
        go.executable_path = PathBuf::from(r"C:\zenv\bin\go\1.27.0\bin\go.exe");
        let mut php = fake_install(ComponentId::Php, "8.4.21", Path::new(r"C:\zenv\bin\php\8.4.21"));
        php.executable_path = PathBuf::from(r"C:\zenv\bin\php\8.4.21\php.exe");
        let env = build(&[go, php], Some(OsString::from(r"C:\Windows;C:\Program Files\Go\bin")));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.to_string_lossy().into_owned());
        assert_eq!(get("GOROOT").unwrap(), r"C:\zenv\bin\go\1.27.0");
        assert_eq!(get("PHPRC").unwrap(), r"C:\zenv\bin\php\8.4.21");
        let path = get("PATH").unwrap();
        assert!(path.starts_with(r"C:\zenv\bin\go\1.27.0\bin;C:\zenv\bin\php\8.4.21;"), "{path}");
        assert!(path.ends_with(r"C:\Program Files\Go\bin"), "inherited PATH kept, but after");
    }

    #[test]
    fn managed_rust_uses_its_own_rustup_homes() {
        let root = Path::new(r"C:\zenv\bin\rust\1.99.1");
        let mut rust = fake_install(ComponentId::Rust, "1.99.1", root);
        rust.executable_path = root.join(r"rustup\toolchains\1.99.1-x86_64-pc-windows-gnu\bin\rustc.exe");
        let env = build(&[rust.clone()], None);
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.to_string_lossy().into_owned());
        assert_eq!(get("RUSTUP_HOME").unwrap(), r"C:\zenv\bin\rust\1.99.1\rustup");
        assert_eq!(get("CARGO_HOME").unwrap(), r"C:\zenv\bin\rust\1.99.1\cargo");
        assert_eq!(get("RUSTUP_TOOLCHAIN").unwrap(), "1.99.1");
        assert!(get("PATH").unwrap().starts_with(r"C:\zenv\bin\rust\1.99.1\cargo\bin;"));

        // A developer's own rustup install is left exactly as it is.
        rust.root_path = PathBuf::from(r"C:\Users\dev\.cargo");
        rust.executable_path = PathBuf::from(r"C:\Users\dev\.cargo\bin\rustc.exe");
        let env = build(&[rust], None);
        assert!(env.iter().all(|(k, _)| k != "RUSTUP_HOME" && k != "CARGO_HOME"));
    }
}
