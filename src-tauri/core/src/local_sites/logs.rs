//! Logs that belong to one project (site): the web server's access and error
//! log for its domain, the PHP errors of that project only, and the app's own
//! log (Laravel's `storage/logs`). All read-only; paths are resolved here,
//! never taken from the caller.

use std::path::{Path, PathBuf};

use super::{ProjectType, Site, SiteManager, Strategy};
use crate::error::{AppError, AppResult};
use crate::log_manager::{self, LogTail};
use crate::php_runtime::{self, Backend};
use crate::runtime_manager;
use crate::service_manager::LogSource;

/// Where a project log comes from, and how to narrow it to this project.
enum Origin {
    File(PathBuf),
    /// The site's error log, PHP messages only (php-cgi reports through FastCGI).
    PhpInErrorLog(PathBuf),
    /// FrankenPHP's shared log, lines mentioning this project's folder only.
    PhpInFranken(PathBuf),
}

impl Origin {
    fn path(&self) -> &Path {
        match self {
            Origin::File(p) | Origin::PhpInErrorLog(p) | Origin::PhpInFranken(p) => p,
        }
    }
}

/// php-cgi output reaches the web server's error log through FastCGI's
/// stderr: Apache logs it as AH01071 "Got error", Nginx as "FastCGI sent in stderr".
const PHP_MARKERS: &[&str] = &[
    "AH01071",
    "FastCGI sent in stderr",
    "PHP message:",
    "PHP Fatal error",
    "PHP Parse error",
    "PHP Warning",
    "PHP Notice",
    "PHP Deprecated",
    "PHP Stack trace",
];

const APACHE_STDERR: &str = "AH01071: Got error '";
const NGINX_STDERR: &str = "FastCGI sent in stderr: \"";

/// What PHP wrote, without the web server's wrapping: the entry's time, then
/// one line per message. Apache packs messages into one entry separated by a
/// literal `\n` (and doubles backslashes); Nginx separates them with
/// `PHP message: ` inside a quoted string.
pub fn expand_fastcgi(line: &str) -> Vec<String> {
    let (stamp, body) = if let Some(i) = line.find(APACHE_STDERR) {
        let stamp = line.strip_prefix('[').and_then(|r| r.split_once(']')).map(|(t, _)| format!("[{t}]"));
        (stamp, line[i + APACHE_STDERR.len()..].trim_end_matches('\''))
    } else if let Some(i) = line.find(NGINX_STDERR) {
        let rest = &line[i + NGINX_STDERR.len()..];
        let end = rest.rfind("\" while").unwrap_or(rest.len());
        (line.get(..19).map(str::to_string), &rest[..end])
    } else {
        (None, line)
    };
    body.split(r"\n")
        .flat_map(|part| part.split("PHP message: "))
        .map(|l| l.replace(r"\\", r"\").trim_end().to_string())
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| match (&stamp, i) {
            (Some(s), 0) => format!("{s} {l}"),
            _ => l,
        })
        .collect()
}

/// FrankenPHP writes JSON lines; show `HH:MM:SS level message`.
pub fn readable_franken(line: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return line.to_string() };
    let msg = v.get("msg").and_then(|m| m.as_str()).unwrap_or(line);
    let level = v.get("level").and_then(|m| m.as_str()).unwrap_or("");
    let time = v
        .get("ts")
        .and_then(|t| t.as_f64())
        .and_then(|t| time::OffsetDateTime::from_unix_timestamp(t as i64).ok())
        .map(|t| {
            let t = t.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
            format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
        })
        .unwrap_or_default();
    format!("{time} {level} {msg}").trim().to_string()
}

pub fn is_php_message(line: &str) -> bool {
    PHP_MARKERS.iter().any(|m| line.contains(m))
}

/// Lowercase, forward slashes, JSON escapes undone: comparable across log formats.
fn normalized(s: &str) -> String {
    s.replace("\\\\", "/").replace('\\', "/").to_ascii_lowercase()
}

pub fn mentions_folder(line: &str, folder: &Path) -> bool {
    let needle = normalized(&folder.display().to_string());
    !needle.is_empty() && normalized(line).contains(needle.trim_end_matches('/'))
}

/// The newest `*.log` directly inside `<project>/storage/logs` (Laravel's
/// single or daily channel), if it really lives inside the project.
pub fn app_log(project_root: &Path) -> Option<PathBuf> {
    let dir = project_root.join("storage").join("logs");
    let root = dunce::canonicalize(project_root).ok()?;
    let newest = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("log")))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?
        .path();
    dunce::canonicalize(&newest).ok().filter(|p| p.starts_with(&root))
}

impl SiteManager {
    fn site_server_logs(&self, site: &Site) -> AppResult<PathBuf> {
        let service_id = match &site.service_id {
            Some(sid) => sid.clone(),
            None => self.serving_service()?.map(|s| s.service.id).unwrap_or_default(),
        };
        Ok(self.paths.service_logs_dir(&service_id).join("sites"))
    }

    fn log_origins(&self, site: &Site) -> AppResult<Vec<(&'static str, String, Origin)>> {
        let dir = self.site_server_logs(site)?;
        let error_log = dir.join(format!("{}-error.log", site.domain));
        let mut out = vec![
            ("access", "Access".to_string(), Origin::File(dir.join(format!("{}-access.log", site.domain)))),
            ("error", "Error".to_string(), Origin::File(error_log.clone())),
        ];
        if site.strategy == Strategy::Php {
            if let Some(php_id) = site.runtime_installation_id.as_deref() {
                let php = runtime_manager::get(&self.db, php_id)?;
                let frankens = php_runtime::frankenphp_installations(&self.db)?;
                let origin = match php_runtime::select(site.php_engine, &php, &frankens) {
                    Ok(Backend::Frankenphp { franken }) => {
                        Origin::PhpInFranken(php_runtime::runtime_logs_dir(&self.paths, &franken).join("frankenphp.log"))
                    }
                    _ => Origin::PhpInErrorLog(error_log),
                };
                out.push(("php", "PHP".to_string(), origin));
            }
        }
        if site.strategy == Strategy::ReverseProxy {
            out.push(("process", "Output".to_string(), Origin::File(crate::app_process::log_file(&self.paths, &site.id))));
        }
        if let Some(app) = app_log(&site.project_root) {
            let label = if site.project_type == ProjectType::Laravel { "Laravel" } else { "App log" };
            out.push(("app", label.to_string(), Origin::File(app)));
        }
        Ok(out)
    }

    /// The logs that exist (or will exist) for one project.
    pub fn log_sources(&self, id: &str) -> AppResult<Vec<LogSource>> {
        let site = self.get(id)?;
        Ok(self
            .log_origins(&site)?
            .into_iter()
            .map(|(id, label, origin)| {
                let meta = std::fs::metadata(origin.path()).ok();
                LogSource {
                    id: id.to_string(),
                    label,
                    exists: meta.is_some(),
                    size_bytes: meta.map_or(0, |m| m.len()),
                    path: origin.path().to_path_buf(),
                }
            })
            .collect())
    }

    /// Reads one project log: `access`, `error`, `php` or `app`.
    pub fn read_log(&self, id: &str, kind: &str, lines: usize) -> AppResult<LogTail> {
        let site = self.get(id)?;
        let (_, _, origin) = self
            .log_origins(&site)?
            .into_iter()
            .find(|(k, _, _)| *k == kind)
            .ok_or_else(|| AppError::NotFound { entity: "log", id: kind.to_string() })?;
        let keep = |tail: LogTail, f: &dyn Fn(&str) -> Option<Vec<String>>| {
            let mut tail = tail;
            let matching: Vec<String> = tail.lines.drain(..).filter_map(|l| f(&l)).flatten().collect();
            let skip = matching.len().saturating_sub(lines);
            tail.truncated |= skip > 0;
            tail.lines = matching.into_iter().skip(skip).collect();
            tail
        };
        match origin {
            Origin::File(p) => log_manager::tail(&p, lines),
            Origin::PhpInErrorLog(p) => Ok(keep(log_manager::tail(&p, log_manager::MAX_LINES)?, &|l| {
                is_php_message(l).then(|| expand_fastcgi(l))
            })),
            Origin::PhpInFranken(p) => {
                let root = site.project_root.clone();
                Ok(keep(log_manager::tail(&p, log_manager::MAX_LINES)?, &|l| {
                    mentions_folder(l, &root).then(|| vec![readable_franken(l)])
                }))
            }
        }
    }

    /// The file behind a project log, for "Open file".
    pub fn log_file(&self, id: &str, kind: &str) -> AppResult<PathBuf> {
        let site = self.get(id)?;
        self.log_origins(&site)?
            .into_iter()
            .find(|(k, _, _)| *k == kind)
            .map(|(_, _, o)| o.path().to_path_buf())
            .ok_or_else(|| AppError::NotFound { entity: "log", id: kind.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_php_messages_from_apache_and_nginx() {
        let apache = r#"[Sun Oct 19 10:21:03.123 2026] [proxy_fcgi:error] [pid 1:tid 2] [client 127.0.0.1:5] AH01071: Got error 'PHP message: PHP Warning:  Undefined variable $user in D:\\p\\shop\\public\\index.php on line 3'"#;
        let nginx = r#"2026/10/19 10:21:03 [error] 1#2: *3 FastCGI sent in stderr: "PHP message: PHP Fatal error:  Uncaught Error" while reading response header"#;
        assert!(is_php_message(apache) && is_php_message(nginx));
        assert!(!is_php_message("[Sun Oct 19] [core:error] AH00037: Symbolic link not allowed"));
    }

    #[test]
    fn makes_fastcgi_and_frankenphp_lines_readable() {
        let apache = r"[Mon Oct 19 09:54:19.355 2026] [proxy_fcgi:error] [pid 1:tid 2] [client 127.0.0.1:5] AH01071: Got error 'zenv-error\nPHP Warning:  oops in C:\\p\\x.php on line 1\n'";
        assert_eq!(
            expand_fastcgi(apache),
            ["[Mon Oct 19 09:54:19.355 2026] zenv-error", r"PHP Warning:  oops in C:\p\x.php on line 1"],
            "server wrapping removed, Apache's doubled backslashes undone"
        );
        let nginx = r#"2026/10/19 10:21:03 [error] 9#1: *3 FastCGI sent in stderr: "PHP message: PHP Warning:  a in x.php on line 2PHP message: PHP Fatal error:  b" while reading response header from upstream"#;
        assert_eq!(expand_fastcgi(nginx), ["2026/10/19 10:21:03 PHP Warning:  a in x.php on line 2", "PHP Fatal error:  b"]);
        assert!(is_php_message("[x] [proxy_fcgi:error] AH01071: Got error 'plain error_log text'"));
        let franken = r#"{"level":"info","ts":1792378112.24,"logger":"frankenphp","msg":"PHP Warning:  oops on line 1"}"#;
        let out = readable_franken(franken);
        assert!(out.ends_with("info PHP Warning:  oops on line 1"), "{out}");
        assert_eq!(readable_franken("not json"), "not json");
    }

    #[test]
    fn matches_project_folders_across_slash_styles_and_json_escapes() {
        let root = Path::new(r"D:\Projects\Shop");
        assert!(mentions_folder(r#"{"msg":"PHP Warning in D:\\Projects\\Shop\\public\\index.php"}"#, root));
        assert!(mentions_folder("PHP Warning in d:/projects/shop/app/x.php", root));
        assert!(!mentions_folder("PHP Warning in D:/Projects/Blog/index.php", root));
    }

    #[test]
    fn app_log_is_the_newest_log_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("storage").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        assert_eq!(app_log(dir.path()), None);
        std::fs::write(logs.join("laravel-2026-10-18.log"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(logs.join("laravel-2026-10-19.log"), "new").unwrap();
        std::fs::write(logs.join(".gitignore"), "*").unwrap();
        let found = app_log(dir.path()).unwrap();
        assert!(found.ends_with("laravel-2026-10-19.log"), "{}", found.display());
    }
}
