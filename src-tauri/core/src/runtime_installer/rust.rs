//! Rust through the official `rustup-init.exe`, installed into its own
//! RUSTUP_HOME / CARGO_HOME inside the Zenv folder. The developer's existing
//! `~/.rustup` and `~/.cargo` are never touched, and PATH is not modified
//! (`--no-modify-path`). rustup verifies every toolchain component against the
//! channel manifest's hashes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{AvailableVersion, InstallProvider, Unpack};
use crate::error::{AppError, AppResult};
use crate::platform;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::probe::Located;

pub const MANIFESTS: &str = "https://static.rust-lang.org/manifests.txt";

pub struct RustProvider;

/// MSVC toolchains need the Visual Studio linker; without it, the GNU host
/// (which ships its own linker) is the one that can build programs.
pub fn host_triple() -> &'static str {
    let vswhere = Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    if vswhere.is_file() { "x86_64-pc-windows-msvc" } else { "x86_64-pc-windows-gnu" }
}

pub fn rustup_url(host: &str) -> String {
    format!("https://static.rust-lang.org/rustup/dist/{host}/rustup-init.exe")
}

/// Stable versions from `manifests.txt` lines like
/// `static.rust-lang.org/dist/2026-10-15/channel-rust-1.99.1.toml`, newest first.
pub fn parse_manifests(text: &str, min_minor: u32) -> Vec<String> {
    let mut versions: Vec<(u32, u32, u32)> = text
        .lines()
        .filter_map(|l| l.rsplit('/').next()?.strip_prefix("channel-rust-")?.strip_suffix(".toml").map(str::to_string))
        .filter_map(|v| {
            let p: Vec<u32> = v.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
            (p.len() == 3 && p[0] == 1 && p[1] >= min_minor).then(|| (p[0], p[1], p[2]))
        })
        .collect();
    versions.sort();
    versions.dedup();
    versions.into_iter().rev().map(|(a, b, c)| format!("{a}.{b}.{c}")).collect()
}

/// RUSTUP_HOME / CARGO_HOME of a Zenv-managed Rust install.
pub fn homes(root: &Path) -> (PathBuf, PathBuf) {
    (root.join("rustup"), root.join("cargo"))
}

/// The real compiler (not the rustup proxy) of the installed toolchain.
pub fn toolchain_rustc(root: &Path, version: &str) -> Option<PathBuf> {
    let toolchains = homes(root).0.join("toolchains");
    std::fs::read_dir(&toolchains)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{version}-"))))
        .map(|p| p.join("bin").join("rustc.exe"))
        .filter(|p| p.is_file())
}

impl InstallProvider for RustProvider {
    fn component(&self) -> ComponentId {
        ComponentId::Rust
    }

    fn source(&self) -> &'static str {
        "rust-lang.org"
    }

    fn allowed_hosts(&self) -> &'static [&'static str] {
        &["static.rust-lang.org"]
    }

    fn list(&self, http: &ureq::Agent) -> AppResult<Vec<AvailableVersion>> {
        let get = |url: &str| -> AppResult<String> {
            http.get(url)
                .call()
                .map_err(|e| AppError::Platform(format!("cannot reach static.rust-lang.org: {e}")))?
                .body_mut()
                .with_config()
                .limit(32 * 1024 * 1024)
                .read_to_string()
                .map_err(|e| AppError::Platform(format!("unexpected Rust release data: {e}")))
        };
        let host = host_triple();
        let url = rustup_url(host);
        let sha = get(&format!("{url}.sha256"))?.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
        if sha.len() != 64 {
            return Err(AppError::Platform("rustup-init checksum is missing".into()));
        }
        Ok(parse_manifests(&get(MANIFESTS)?, 70)
            .into_iter()
            .take(40)
            .map(|version| AvailableVersion {
                component: ComponentId::Rust,
                version,
                stable: true,
                filename: "rustup-init.exe".into(),
                url: url.clone(),
                sha256: sha.clone(),
                size: 0,
                source: "rust-lang.org",
                installed_id: None,
                verified_by: "",
            })
            .collect())
    }

    fn unpack(&self) -> Unpack {
        Unpack::Custom
    }

    fn custom_install(&self, download: &Path, target: &Path, offer: &AvailableVersion) -> AppResult<()> {
        let installer = target.join("rustup-init.exe");
        std::fs::copy(download, &installer).map_err(|e| AppError::Platform(format!("cannot stage rustup-init: {e}")))?;
        let (rustup_home, cargo_home) = homes(target);
        let args: Vec<OsString> = vec![
            "-y".into(),
            "--no-modify-path".into(),
            "--profile".into(),
            "minimal".into(),
            "--default-toolchain".into(),
            offer.version.clone().into(),
            "--default-host".into(),
            host_triple().into(),
        ];
        let mut cmd = Command::new(&installer);
        cmd.args(&args)
            .env("RUSTUP_HOME", &rustup_home)
            .env("CARGO_HOME", &cargo_home)
            .env("RUSTUP_INIT_SKIP_PATH_CHECK", "yes")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        platform::hide_console_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot run rustup-init: {e}")))?;
        let deadline = Instant::now() + Duration::from_secs(30 * 60);
        let status = loop {
            if let Some(s) = child.try_wait().map_err(|e| AppError::Platform(e.to_string()))? {
                break s;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(AppError::Platform("rustup-init did not finish within 30 minutes".into()));
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let _ = std::fs::remove_file(&installer);
        if !status.success() {
            let mut err = String::new();
            if let Some(mut s) = child.stderr.take() {
                use std::io::Read;
                let _ = s.read_to_string(&mut err);
            }
            return Err(AppError::Platform(format!("rustup-init failed: {}", err.lines().last().unwrap_or("unknown error"))));
        }
        Ok(())
    }

    fn locate(&self, target: &Path) -> AppResult<Located> {
        let version = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let rustc = toolchain_rustc(target, &version)
            .ok_or_else(|| AppError::invalid_path(target, "the installed toolchain has no rustc.exe"))?;
        Ok(Located { root: target.to_path_buf(), executable: rustc })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stable_versions_from_manifests() {
        let text = "static.rust-lang.org/dist/2026-10-15/channel-rust-1.99.1.toml\n\
                    static.rust-lang.org/dist/2026-10-01/channel-rust-1.99.0.toml\n\
                    static.rust-lang.org/dist/2026-10-01/channel-rust-beta.toml\n\
                    static.rust-lang.org/dist/2020-01-01/channel-rust-1.40.0.toml\n\
                    static.rust-lang.org/dist/2026-10-15/channel-rust-1.99.1.toml\n";
        assert_eq!(parse_manifests(text, 70), ["1.99.1", "1.99.0"]);
    }

    #[test]
    fn finds_the_real_rustc_of_a_toolchain() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("1.99.1");
        let bin = root.join("rustup").join("toolchains").join("1.99.1-x86_64-pc-windows-gnu").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("rustc.exe"), "").unwrap();
        let located = RustProvider.locate(&root).unwrap();
        assert_eq!(located.executable, bin.join("rustc.exe"));
        assert_eq!(located.root, root);
    }
}
