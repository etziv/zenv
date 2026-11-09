//! Per-component version probes. Each component has its own executable layout,
//! arguments and output format; there is no universal `--version`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::error::{AppError, AppResult};
use crate::platform;
use crate::runtime_catalog::ComponentId;

pub struct ProbeSpec {
    /// Candidate executables relative to an installation root, most specific first.
    pub executables: &'static [&'static str],
    pub args: &'static [&'static str],
    /// First capture group is the version.
    pub pattern: &'static str,
}

pub fn spec(component: ComponentId) -> ProbeSpec {
    use ComponentId::*;
    match component {
        Apache => ProbeSpec { executables: &["bin/httpd.exe", "httpd.exe"], args: &["-v"], pattern: r"Apache/(\d+\.\d+\.\d+)" },
        Nginx => ProbeSpec { executables: &["nginx.exe"], args: &["-v"], pattern: r"nginx/(\d+\.\d+\.\d+)" },
        Caddy => ProbeSpec { executables: &["caddy.exe"], args: &["version"], pattern: r"^v?(\d+\.\d+\.\d+)" },
        Frankenphp => ProbeSpec { executables: &["frankenphp.exe"], args: &["version"], pattern: r"FrankenPHP v?(\d+\.\d+\.\d+)" },
        Php => ProbeSpec { executables: &["php.exe"], args: &["-v"], pattern: r"^PHP (\d+\.\d+\.\d+)" },
        Go => ProbeSpec { executables: &["bin/go.exe", "go.exe"], args: &["version"], pattern: r"go version go(\d+\.\d+(?:\.\d+)?)" },
        Rust => ProbeSpec { executables: &["bin/rustc.exe", "rustc.exe"], args: &["-V"], pattern: r"^rustc (\d+\.\d+\.\d+)" },
        Ruby => ProbeSpec { executables: &["bin/ruby.exe", "ruby.exe"], args: &["-v"], pattern: r"^ruby (\d+\.\d+\.\d+)" },
        Python => ProbeSpec { executables: &["python.exe"], args: &["--version"], pattern: r"^Python (\d+\.\d+\.\d+)" },
        Node => ProbeSpec { executables: &["node.exe"], args: &["--version"], pattern: r"^v(\d+\.\d+\.\d+)" },
        Mysql => ProbeSpec { executables: &["bin/mysqld.exe", "mysqld.exe"], args: &["--version"], pattern: r"Ver (\d+\.\d+\.\d+)" },
        Postgresql => ProbeSpec { executables: &["bin/postgres.exe", "postgres.exe"], args: &["--version"], pattern: r"\(PostgreSQL\) (\d+\.\d+(?:\.\d+)?)" },
        Redis => ProbeSpec { executables: &["redis-server.exe"], args: &["--version"], pattern: r"v=(\d+\.\d+\.\d+)" },
        // A Node package: the version comes from its package.json (see detect_version).
        Omniroute => ProbeSpec { executables: &["node_modules/omniroute/bin/omniroute.mjs"], args: &[], pattern: r"(\d+\.\d+\.\d+)" },
        Ninerouter => ProbeSpec { executables: &["node_modules/9router/app/custom-server.js"], args: &[], pattern: r"(\d+\.\d+\.\d+)" },
    }
}

fn pattern(component: ComponentId) -> &'static Regex {
    static CACHE: OnceLock<Vec<(ComponentId, Regex)>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        ComponentId::ALL
            .into_iter()
            .map(|c| (c, Regex::new(&format!("(?m){}", spec(c).pattern)).expect("valid probe regex")))
            .collect()
    });
    &all.iter().find(|(c, _)| *c == component).expect("pattern for every component").1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub version: String,
    /// First non-empty output line, shown as version details.
    pub detail: String,
}

/// Different products that ship an executable with a component's name (XAMPP
/// ships MariaDB as `mysqld.exe`). They must never be registered as that component.
const LOOKALIKES: &[(ComponentId, &str, &str)] = &[(
    ComponentId::Mysql,
    "MariaDB",
    "this is MariaDB, not MySQL; their data directories and administration differ, and MariaDB is not supported yet",
)];

fn lookalike(component: ComponentId, output: &str) -> Option<&'static str> {
    LOOKALIKES
        .iter()
        .find(|(c, needle, _)| *c == component && output.contains(needle))
        .map(|(_, _, why)| *why)
}

pub fn parse_version(component: ComponentId, output: &str) -> Option<VersionInfo> {
    if lookalike(component, output).is_some() {
        return None;
    }
    let caps = pattern(component).captures(output)?;
    let detail = output.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    Some(VersionInfo { version: caps[1].to_string(), detail: detail.to_string() })
}

/// Resolved location of an installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub root: PathBuf,
    pub executable: PathBuf,
}

/// Accepts either an executable or an installation directory.
pub fn locate(component: ComponentId, path: &Path) -> AppResult<Located> {
    let spec = spec(component);
    if !path.exists() {
        return Err(AppError::invalid_path(path, "does not exist"));
    }
    let path = dunce::canonicalize(path).map_err(|e| AppError::invalid_path(path, e.to_string()))?;

    if path.is_file() {
        let file_name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let expected: Vec<&str> = spec.executables.iter().map(|e| e.rsplit('/').next().unwrap_or(e)).collect();
        if !expected.iter().any(|e| e.eq_ignore_ascii_case(&file_name)) {
            return Err(AppError::invalid_path(
                &path,
                format!("expected one of {} for {component}", expected.join(", ")),
            ));
        }
        let parent = path.parent().unwrap_or(&path);
        let root = if parent.file_name().is_some_and(|n| n.eq_ignore_ascii_case("bin"))
            && spec.executables.iter().any(|e| e.starts_with("bin/"))
        {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        };
        return Ok(Located { root: root.to_path_buf(), executable: path.clone() });
    }

    for rel in spec.executables {
        let candidate = path.join(rel.replace('/', "\\"));
        if candidate.is_file() {
            return Ok(Located { root: path.clone(), executable: candidate });
        }
    }
    Err(AppError::invalid_path(
        &path,
        format!("no {component} executable found (looked for {})", spec.executables.join(", ")),
    ))
}

/// Runs the executable's version command with a timeout.
pub fn detect_version(component: ComponentId, executable: &Path) -> AppResult<VersionInfo> {
    if let Some(router) = crate::ai_router::spec(component) {
        // bin/omniroute.mjs, app/custom-server.js: the package's own package.json names the version.
        let manifest = executable.parent().and_then(Path::parent).map(|p| p.join("package.json")).unwrap_or_default();
        let text = std::fs::read_to_string(&manifest).map_err(|e| AppError::Detection(format!("{}: {e}", manifest.display())))?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| AppError::Detection(format!("{}: {e}", manifest.display())))?;
        let version = json.get("version").and_then(|v| v.as_str()).ok_or_else(|| AppError::Detection(format!("{} has no version", manifest.display())))?;
        return Ok(VersionInfo { version: version.to_string(), detail: format!("{} {version}", router.package) });
    }
    let output = run_capture(executable, spec(component).args, Duration::from_secs(10))?.text;
    if let Some(why) = lookalike(component, &output) {
        return Err(AppError::Detection(format!("{}: {why}", executable.display())));
    }
    parse_version(component, &output).ok_or_else(|| {
        let first = output.lines().next().unwrap_or("(no output)");
        AppError::Detection(format!(
            "{} did not report a recognizable {component} version: {first}",
            executable.display()
        ))
    })
}

#[derive(Debug, Clone)]
pub struct CapturedOutput {
    pub code: Option<i32>,
    /// stdout followed by stderr.
    pub text: String,
}

/// Runs a short-lived command with a timeout, capturing its output.
pub fn run_capture<S: AsRef<std::ffi::OsStr>>(
    executable: &Path,
    args: &[S],
    timeout: Duration,
) -> AppResult<CapturedOutput> {
    let mut cmd = Command::new(executable);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = executable.parent() {
        cmd.current_dir(dir);
    }
    platform::hide_console_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Detection(format!("cannot run {}: {e}", executable.display())))?;

    // Drain pipes on threads so a chatty process cannot block on a full pipe.
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Detection(format!(
                    "{} did not finish within {}s",
                    executable.display(),
                    timeout.as_secs()
                )));
            }
        }
    };
    let mut combined = out_t.join().unwrap_or_default();
    let err = err_t.join().unwrap_or_default();
    if !err.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&err);
    }
    Ok(CapturedOutput { code: status.code(), text: combined })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_version_outputs() {
        let cases: &[(ComponentId, &str, &str)] = &[
            (ComponentId::Apache, "Server version: Apache/2.4.67 (Win64)\nApache Lounge VS18 Server built:   Apr 30 2026", "2.4.67"),
            (ComponentId::Nginx, "nginx version: nginx/1.29.1", "1.29.1"),
            (ComponentId::Caddy, "v2.11.2 h1:iOlpsSiSKqEW+SIXrcZsZ/NO74SzB/ycqqvAIEfIm64=", "2.11.2"),
            (ComponentId::Frankenphp, "FrankenPHP 1.12.2 PHP 8.5.6 Caddy v2.11.2 h1:abc", "1.12.2"),
            (ComponentId::Php, "PHP 8.4.21 (cli) (built: May  6 2026 09:30:47) (ZTS Visual C++ 2022 x64)", "8.4.21"),
            (ComponentId::Go, "go version go1.27.0 windows/amd64", "1.27.0"),
            (ComponentId::Go, "go version go1.22 windows/amd64", "1.22"),
            (ComponentId::Rust, "rustc 1.99.0 (b940084d7 2026-09-28)", "1.99.0"),
            (ComponentId::Ruby, "ruby 3.4.1 (2024-12-25 revision 48d4efcb85) +PRISM [x64-mingw-ucrt]", "3.4.1"),
            (ComponentId::Python, "Python 3.14.3", "3.14.3"),
            (ComponentId::Mysql, r"C:\mysql\bin\mysqld.exe  Ver 9.7.0 for Win64 on x86_64 (MySQL Community Server - GPL)", "9.7.0"),
            (ComponentId::Postgresql, "postgres (PostgreSQL) 9.2.24", "9.2.24"),
            (ComponentId::Postgresql, "postgres (PostgreSQL) 17.2", "17.2"),
            (ComponentId::Redis, "Redis server v=8.8.0 sha=00000000:0 malloc=libc bits=64 build=b36aaec09c4e1fe0", "8.8.0"),
        ];
        for (component, output, expected) in cases {
            let v = parse_version(*component, output).unwrap_or_else(|| panic!("{component}: {output}"));
            assert_eq!(v.version, *expected, "{component}");
            assert!(!v.detail.is_empty());
        }
    }

    #[test]
    fn mariadb_is_not_mistaken_for_mysql() {
        let xampp = r"C:\xampp\mysql\bin\mysqld.exe  Ver 10.4.32-MariaDB for Win64 on AMD64 (mariadb.org binary distribution)";
        assert!(parse_version(ComponentId::Mysql, xampp).is_none());
        assert!(lookalike(ComponentId::Mysql, xampp).unwrap().contains("MariaDB"));
    }

    #[test]
    fn rejects_unrelated_output() {
        assert!(parse_version(ComponentId::Apache, "nginx version: nginx/1.29.1").is_none());
        // FrankenPHP output mentions PHP and Caddy, but must not satisfy those probes' anchors.
        let franken = "FrankenPHP 1.12.2 PHP 8.5.6 Caddy v2.11.2 h1:abc";
        assert!(parse_version(ComponentId::Php, franken).is_none());
        assert!(parse_version(ComponentId::Caddy, franken).is_none());
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn locates_from_directory_or_executable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Apache24");
        touch(&root.join("bin").join("httpd.exe"));

        let from_dir = locate(ComponentId::Apache, &root).unwrap();
        assert!(from_dir.executable.ends_with("bin\\httpd.exe"));
        let from_exe = locate(ComponentId::Apache, &root.join("bin").join("httpd.exe")).unwrap();
        assert_eq!(from_exe, from_dir, "bin/ layout resolves to the installation root");

        let php = dir.path().join("php-8.4");
        touch(&php.join("php.exe"));
        assert_eq!(locate(ComponentId::Php, &php).unwrap().root, dunce::canonicalize(&php).unwrap());
    }

    #[test]
    fn locate_errors_are_specific() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(locate(ComponentId::Apache, &dir.path().join("missing")), Err(AppError::InvalidPath { .. })));
        let err = locate(ComponentId::Apache, dir.path()).unwrap_err().to_string();
        assert!(err.contains("bin/httpd.exe"), "{err}");
        touch(&dir.path().join("nginx.exe"));
        let err = locate(ComponentId::Apache, &dir.path().join("nginx.exe")).unwrap_err().to_string();
        assert!(err.contains("expected one of"), "{err}");
    }

    #[cfg(windows)]
    #[test]
    fn non_executable_file_reports_detection_failure() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("php.exe");
        std::fs::write(&fake, b"not a PE file").unwrap();
        assert!(matches!(detect_version(ComponentId::Php, &fake), Err(AppError::Detection(_))));
    }
}
