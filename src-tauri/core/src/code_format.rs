//! Formatting Go, Rust and Python files with each language's own formatter
//! (gofmt, rustfmt, ruff or black), for Studio's editor. Prettier formats the
//! other languages inside the editor itself.
//!
//! Only these programs run, found on the app's PATH (Zenv's language
//! versions first), in the project folder so their settings files apply
//! (rustfmt.toml, pyproject.toml, ...). The text goes in on stdin and comes
//! back on stdout; nothing is written to disk.
//!
//! A missing formatter is reported as `NotFound { entity: "formatter" }` so
//! the editor can offer to install it: ruff into the Zenv home (the official
//! GitHub release, checked against its SHA-256), rustfmt as a rustup
//! component. gofmt comes with Go (Languages).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};
use crate::studio::{find, path_of};

const TIMEOUT: Duration = Duration::from_secs(20);
const RUFF_REPO: &str = "astral-sh/ruff";
const RUFF_ASSET: &str = "ruff-x86_64-pc-windows-msvc.zip";

fn missing(tool: &str) -> AppError {
    AppError::NotFound { entity: "formatter", id: tool.to_string() }
}

/// A program Zenv installed under `<bin>/<name>/<version>/`, newest version first.
fn zenv_tool(bin_dir: &Path, name: &str) -> Option<PathBuf> {
    let mut versions: Vec<PathBuf> = std::fs::read_dir(bin_dir.join(name)).into_iter().flatten().flatten().map(|e| e.path()).collect();
    let key = |p: &PathBuf| {
        p.file_name().map(|n| n.to_string_lossy().split('.').filter_map(|x| x.parse::<u32>().ok()).collect::<Vec<_>>()).unwrap_or_default()
    };
    versions.sort_by_key(|p| std::cmp::Reverse(key(p)));
    versions.into_iter().find_map(|dir| {
        crate::runtime_installer::programs_under(&dir).into_iter().find(|p| {
            p.file_stem().is_some_and(|s| s.eq_ignore_ascii_case(name)) && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Go,
    Rust,
    Python,
}

fn lang_of(path: &str) -> Option<Lang> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "go" => Some(Lang::Go),
        "rs" => Some(Lang::Rust),
        "py" | "pyi" => Some(Lang::Python),
        _ => None,
    }
}

/// The Rust edition of the crate holding `file` (its nearest Cargo.toml).
fn rust_edition(root: &Path, file: &Path) -> String {
    let re = regex::Regex::new(r#"(?m)^\s*edition\s*=\s*"(\d{4})""#).expect("valid regex");
    let mut dir = file.parent();
    while let Some(d) = dir {
        if let Ok(toml) = std::fs::read_to_string(d.join("Cargo.toml")) {
            if let Some(c) = re.captures(&toml) {
                return c[1].to_string();
            }
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    "2021".into()
}

/// Runs `program args` with `input` on stdin; its stdout, or its complaint.
fn run(program: &Path, args: &[String], cwd: &Path, env: &[(std::ffi::OsString, std::ffi::OsString)], input: &str, name: &str) -> AppResult<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    crate::platform::hide_console_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot start {name}: {e}")))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let text = input.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(text.as_bytes());
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stdout.read_to_end(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.read_to_end(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| AppError::Platform(e.to_string()))? {
            break s;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(AppError::Platform(format!("{name} took longer than {}s", TIMEOUT.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = writer.join();
    let stdout = out.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).trim().to_string();
    if !status.success() {
        let detail = if stderr.is_empty() { format!("exit code {}", status.code().unwrap_or(-1)) } else { stderr };
        return Err(AppError::Validation(format!("{name}: {detail}")));
    }
    String::from_utf8(stdout).map_err(|_| AppError::Platform(format!("{name} returned text that is not UTF-8")))
}

impl Core {
    /// `content` of the project file `path`, formatted by its language's tool.
    pub fn format_code(&self, site_id: &str, path: &str, content: &str) -> AppResult<String> {
        let lang = lang_of(path).ok_or_else(|| AppError::Unsupported(format!("no formatter for {path}")))?;
        let root = self.project_root(site_id)?;
        let file = root.join(path.replace('/', "\\"));
        let env = self.studio_env(site_id)?;
        let search = path_of(&env);
        let (program, args, name): (PathBuf, Vec<String>, &str) = match lang {
            Lang::Go => (find("gofmt", &search).ok_or_else(|| missing("gofmt"))?, vec![], "gofmt"),
            Lang::Rust => (
                find("rustfmt", &search).ok_or_else(|| missing("rustfmt"))?,
                vec!["--edition".into(), rust_edition(&root, &file), "--emit".into(), "stdout".into()],
                "rustfmt",
            ),
            Lang::Python => match zenv_tool(&self.paths.bin_dir, "ruff").or_else(|| find("ruff", &search)) {
                Some(ruff) => (ruff, vec!["format".into(), "--stdin-filename".into(), path.into(), "-".into()], "ruff"),
                None => (
                    find("black", &search).ok_or_else(|| missing("ruff"))?,
                    vec!["-q".into(), "--stdin-filename".into(), path.into(), "-".into()],
                    "black",
                ),
            },
        };
        let formatted = match run(&program, &args, &root, &env, content, name) {
            // rustup's proxy is there, the component is not.
            Err(AppError::Validation(m)) if lang == Lang::Rust && m.contains("is not installed for the toolchain") => {
                return Err(missing("rustfmt"))
            }
            r => r?,
        };
        // rustfmt reports a missing toolchain component on stderr with exit 1
        // (handled above); an empty result for non-empty input means failure.
        if formatted.trim().is_empty() && !content.trim().is_empty() {
            return Err(AppError::Validation(format!("{name} returned nothing")));
        }
        Ok(formatted)
    }
}

impl Core {
    /// Installs a missing formatter: ruff (into the Zenv home) or rustfmt
    /// (a rustup component). Returns what was installed.
    pub fn install_formatter(&self, site_id: &str, tool: &str) -> AppResult<String> {
        match tool {
            "ruff" => install_ruff(&self.paths.bin_dir, &self.paths.downloads_dir),
            "rustfmt" => {
                let env = self.studio_env(site_id)?;
                let rustup = find("rustup", &path_of(&env))
                    .ok_or_else(|| AppError::Validation("rustup isn't installed: install Rust in Zenv's Languages first.".into()))?;
                let mut cmd = Command::new(&rustup);
                cmd.args(["component", "add", "rustfmt"]);
                for (k, v) in &env {
                    cmd.env(k, v);
                }
                crate::platform::hide_console_window(&mut cmd);
                let out = cmd.output().map_err(|e| AppError::Platform(format!("cannot start rustup: {e}")))?;
                if !out.status.success() {
                    return Err(AppError::Platform(format!("rustup component add rustfmt: {}", String::from_utf8_lossy(&out.stderr).trim())));
                }
                tracing::info!("rustfmt installed with rustup");
                Ok("rustfmt".into())
            }
            "gofmt" => Err(AppError::Validation("gofmt comes with Go: install Go in Zenv's Languages.".into())),
            other => Err(AppError::Validation(format!("Zenv can't install {other}"))),
        }
    }

}

/// ruff's latest stable Windows build from its GitHub releases, into
/// `<bin_dir>/ruff/<version>`.
fn install_ruff(bin_dir: &Path, downloads_dir: &Path) -> AppResult<String> {
    {
        use crate::runtime_installer::{download, extract_zip, github, http_agent};
        let http = http_agent();
        let releases = github::releases(&http, RUFF_REPO)?;
        let (version, asset) = releases
            .iter()
            .filter(|r| !r.draft && !r.prerelease)
            .find_map(|r| {
                r.assets.iter().find(|a| a.name == RUFF_ASSET && a.digest.is_some()).map(|a| (r.tag_name.trim_start_matches('v').to_string(), a))
            })
            .ok_or_else(|| AppError::Platform(format!("no {RUFF_ASSET} with a checksum in ruff's releases")))?;
        if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Err(AppError::Validation(format!("unexpected ruff version {version}")));
        }
        let expected = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).unwrap_or_default().to_ascii_lowercase();
        let archive = downloads_dir.join(format!("{RUFF_ASSET}.{}.part", crate::util::new_id()));
        let got = download(&http, &asset.browser_download_url, github::HOSTS, &archive, |_, _| {});
        let result = (|| {
            if got? != expected {
                return Err(AppError::Validation("the ruff download does not match its published SHA-256".into()));
            }
            let target = bin_dir.join("ruff").join(&version);
            let _ = std::fs::remove_dir_all(&target);
            extract_zip(&archive, &target, None)?;
            let exe = zenv_tool(bin_dir, "ruff").ok_or_else(|| AppError::Platform("ruff.exe is not in the download".into()))?;
            let mut check = Command::new(&exe);
            check.arg("--version");
            crate::platform::hide_console_window(&mut check);
            if !check.output().is_ok_and(|o| o.status.success()) {
                return Err(AppError::Platform(format!("{} does not run", exe.display())));
            }
            tracing::info!(%version, "ruff installed");
            Ok(format!("ruff {version}"))
        })();
        let _ = std::fs::remove_file(&archive);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_language_by_extension() {
        assert_eq!(lang_of("cmd/server/main.go"), Some(Lang::Go));
        assert_eq!(lang_of("src/lib.rs"), Some(Lang::Rust));
        assert_eq!(lang_of("app/main.PY"), Some(Lang::Python));
        assert_eq!(lang_of("index.php"), None);
    }

    #[test]
    fn rust_edition_comes_from_the_nearest_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("core/src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nedition = \"2021\"\n").unwrap();
        std::fs::write(root.join("core/Cargo.toml"), "[package]\nname = \"x\"\nedition = \"2024\"\n").unwrap();
        assert_eq!(rust_edition(root, &root.join("core/src/lib.rs")), "2024");
        assert_eq!(rust_edition(root, &root.join("src/main.rs")), "2021");
    }

    #[test]
    fn finds_the_newest_tool_zenv_installed() {
        let dir = tempfile::tempdir().unwrap();
        for v in ["0.9.10", "0.14.2", "0.14.10"] {
            let p = dir.path().join("ruff").join(v);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("ruff.exe"), "").unwrap();
        }
        let found = zenv_tool(dir.path(), "ruff").unwrap();
        assert!(found.ends_with(Path::new("ruff").join("0.14.10").join("ruff.exe")), "{}", found.display());
        assert!(zenv_tool(dir.path(), "black").is_none());
    }

    /// Downloads ruff for real: `ZENV_RUFF_BIN=C:\zenv\bin cargo test ... -- --ignored`.
    #[test]
    #[ignore]
    fn installs_ruff_from_github() {
        let bin = std::env::var_os("ZENV_RUFF_BIN").map(PathBuf::from);
        let tmp = tempfile::tempdir().unwrap();
        let bin = bin.unwrap_or_else(|| tmp.path().join("bin"));
        let installed = install_ruff(&bin, &tmp.path().join("downloads")).unwrap();
        println!("installed {installed} into {}", bin.display());
        assert!(zenv_tool(&bin, "ruff").is_some());
    }

    #[cfg(windows)]
    #[test]
    fn feeds_stdin_and_reads_stdout() {
        // `sort` reads stdin and writes stdout, like a formatter.
        let sort = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join(r"System32\sort.exe");
        let out = run(&sort, &[], &std::env::temp_dir(), &[], "b\r\na\r\n", "sort").unwrap();
        assert_eq!(out.lines().collect::<Vec<_>>(), vec!["a", "b"]);
    }
}
