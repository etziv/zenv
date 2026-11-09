//! Screenshots of local apps for the Apps grid.
//!
//! Microsoft Edge (part of Windows) or Chrome renders the app's URL headless
//! into a PNG under `<home>\cache\thumbnails`, with the app's domain mapped to
//! its loopback address, so it works before the hosts file is updated.
//!
//! The page runs unattended, so the browser is boxed in: a fresh profile for
//! each capture (deleted afterwards; the user's browser profile is never
//! touched), and no network but the app itself. Every other address, the
//! internet, the LAN and other local services alike (say, an AI router with
//! keys on 127.0.0.1:20128), goes to a proxy that does not exist.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::error::{AppError, AppResult, IoContext};
use crate::paths::AppPaths;
use crate::platform;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(40);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnail {
    /// `data:image/png;base64,...`, ready for an <img>.
    pub data_url: String,
    /// Seconds since the Unix epoch.
    pub captured_at: u64,
}

pub struct Thumbnails {
    dir: PathBuf,
    /// One capture at a time: each one is a full headless browser.
    lock: Mutex<()>,
}

/// Nothing listens there, so a request through it fails at once.
const DEAD_PROXY: &str = "http://127.0.0.1:9";

/// Browser switches that confine a capture of `host` to the app itself.
fn sandbox_args(host: &str) -> Vec<String> {
    vec![
        format!("--proxy-server={DEAD_PROXY}"),
        // Only the app goes direct; `<-loopback>` drops the implicit
        // exception for 127.0.0.1 and localhost.
        format!("--proxy-bypass-list={host};<-loopback>"),
        format!("--host-resolver-rules=MAP {host} 127.0.0.1, MAP * ~NOTFOUND"),
        "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".into(),
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
    ]
}

/// Edge first (always present on Windows 10/11), then Chrome.
pub fn browser() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
            candidates.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
            candidates.push(base.join(r"Google\Chrome\Application\chrome.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', ':']).next()?;
    (!host.is_empty()).then_some(host)
}

/// Asks the app for its page first (plain HTTP only): a browser renders no
/// screenshot of an empty error response, and would only time out.
fn precheck(url: &str, host: &str) -> AppResult<()> {
    use std::io::{Read, Write};
    let Some(rest) = url.strip_prefix("http://") else { return Ok(()) };
    let authority = rest.split('/').next().unwrap_or(host);
    let port = authority.rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok()).unwrap_or(80);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|_| AppError::Conflict(format!("nothing answers on port {port}; start the app's web server first")))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(15)));
    let path = format!("/{}", rest.split_once('/').map_or("", |(_, p)| p));
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: Zenv preview\r\n\r\n").ctx(|| "asking for the page".into())?;
    let mut buf = Vec::new();
    let _ = s.take(64 * 1024).read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let status: u16 = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    if status >= 400 && body.trim().is_empty() {
        return Err(AppError::Conflict(format!("{url} answers {status} with an empty page, so there is nothing to preview")));
    }
    Ok(())
}

/// Standard base64 (no external crate needed for this one use).
pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

impl Thumbnails {
    pub fn new(paths: &AppPaths) -> Thumbnails {
        Thumbnails { dir: paths.root.join("cache").join("thumbnails"), lock: Mutex::new(()) }
    }

    fn file(&self, site_id: &str) -> PathBuf {
        self.dir.join(format!("{site_id}.png"))
    }

    /// The stored screenshot, if one was taken.
    pub fn get(&self, site_id: &str) -> AppResult<Option<Thumbnail>> {
        let file = self.file(site_id);
        let Ok(bytes) = std::fs::read(&file) else { return Ok(None) };
        let captured_at = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        Ok(Some(Thumbnail { data_url: format!("data:image/png;base64,{}", base64(&bytes)), captured_at }))
    }

    pub fn remove(&self, site_id: &str) {
        let _ = std::fs::remove_file(self.file(site_id));
    }

    /// Renders `url` headless and stores it as the site's screenshot.
    pub fn capture(&self, site_id: &str, url: &str) -> AppResult<Thumbnail> {
        let browser = browser().ok_or_else(|| AppError::Unsupported("Microsoft Edge or Google Chrome is needed for app previews".into()))?;
        let host = host_of(url).ok_or_else(|| AppError::Validation(format!("cannot preview {url}")))?;
        crate::config_generator::safe_value("domain", host)?;
        precheck(url, host)?;
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        std::fs::create_dir_all(&self.dir).ctx(|| format!("creating {}", self.dir.display()))?;
        let staged = self.dir.join(format!("{site_id}.capture.png"));
        let _ = std::fs::remove_file(&staged);

        let mut screenshot = std::ffi::OsString::from("--screenshot=");
        screenshot.push(&staged);
        // Earlier versions shared one profile between captures.
        let _ = std::fs::remove_dir_all(self.dir.join("browser-profile"));
        let profile_dir = self.dir.join(format!("profile-{}", crate::util::new_id()));
        let mut profile = std::ffi::OsString::from("--user-data-dir=");
        profile.push(&profile_dir);
        let mut cmd = Command::new(&browser);
        cmd.args([
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--mute-audio",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            // The only server it can reach is the app, with Zenv's own certificate.
            "--ignore-certificate-errors",
            "--window-size=1280,800",
            "--force-device-scale-factor=0.5",
            // Screenshot after 8 s even if the page keeps requests open; a
            // virtual time budget would wait for the network to go idle.
            "--timeout=8000",
        ])
        .args(sandbox_args(host))
        .arg(profile)
        .arg(screenshot)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        platform::hide_console_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| AppError::Platform(format!("cannot start {}: {e}", browser.display())))?;
        // The whole browser (its renderer and GPU processes too) ends with the capture.
        let job = platform::Job::new()?;
        let _ = job.assign(&child);
        // Pages that keep connections open (polling, websockets, a dev server)
        // keep headless Edge running after it wrote the screenshot, so the
        // file itself is the signal: a complete PNG whose size stopped changing.
        let deadline = Instant::now() + CAPTURE_TIMEOUT;
        let mut exited_at: Option<Instant> = None;
        let mut last_size = 0;
        let done = loop {
            let size = std::fs::metadata(&staged).map_or(0, |m| m.len());
            if size > 0 && size == last_size && is_png(&staged) {
                break true;
            }
            last_size = size;
            if exited_at.is_none() && child.try_wait().map_err(|e| AppError::Platform(e.to_string()))?.is_some() {
                exited_at = Some(Instant::now());
            }
            // Exited without a file: give a handed-off browser a moment to write it.
            if exited_at.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) || Instant::now() > deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(300));
        };
        let _ = job.terminate(0);
        let _ = child.kill();
        let _ = child.wait();
        remove_profile(&profile_dir);
        if !done {
            let _ = std::fs::remove_file(&staged);
            return Err(AppError::Platform(if exited_at.is_some() {
                format!("the browser produced no screenshot of {url}")
            } else {
                format!("{url} did not finish loading within {}s", CAPTURE_TIMEOUT.as_secs())
            }));
        }
        let file = self.file(site_id);
        std::fs::rename(&staged, &file).ctx(|| format!("saving {}", file.display()))?;
        self.get(site_id)?.ok_or_else(|| AppError::Platform("screenshot vanished".into()))
    }
}

/// The browser's processes may hold files for a moment after they end.
fn remove_profile(dir: &Path) {
    for _ in 0..10 {
        if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    tracing::warn!(dir = %dir.display(), "browser profile of a preview not removed");
}

fn is_png(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|b| b.starts_with(b"\x89PNG"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"\x89PNG\r\n"), "iVBORw0K");
    }

    #[test]
    fn reads_the_host_to_map() {
        assert_eq!(host_of("http://shop.test/"), Some("shop.test"));
        assert_eq!(host_of("https://shop.test:8443/x"), Some("shop.test"));
        assert_eq!(host_of("shop.test"), None);
    }

    #[test]
    #[ignore = "needs Microsoft Edge"]
    fn captures_a_page_that_keeps_a_connection_open() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut s in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut buf = [0u8; 2048];
                    let n = s.read(&mut buf).unwrap_or(0);
                    if String::from_utf8_lossy(&buf[..n]).starts_with("GET /events") {
                        // A server-sent event stream that never ends.
                        let _ = write!(s, "HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache

");
                        loop {
                            if write!(s, "data: tick

").is_err() {
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    }
                    let body = "<html><body style='background:#0b8a65'><h1>live</h1><script>new EventSource('/events')</script></body></html>";
                    let _ = write!(s, "HTTP/1.1 200 OK
Content-Type: text/html
Content-Length: {}
Connection: close

{body}", body.len());
                });
            }
        });
        let t = Thumbnails::new(&AppPaths::new(dir.path()));
        let started = Instant::now();
        let thumb = t.capture("live", &format!("http://live.test:{port}/")).unwrap();
        println!("captured in {:.1?}", started.elapsed());
        assert!(thumb.data_url.starts_with("data:image/png;base64,iVBORw0K"));
        assert!(started.elapsed() < CAPTURE_TIMEOUT, "did not wait for the browser to exit");
        assert!(!dir.path().join("cache/thumbnails/live.capture.png").exists());
    }

    #[test]
    fn the_page_reaches_only_its_own_app() {
        let args = sandbox_args("shop.test");
        assert!(args.contains(&"--proxy-bypass-list=shop.test;<-loopback>".to_string()));
        assert!(args.iter().any(|a| a == "--host-resolver-rules=MAP shop.test 127.0.0.1, MAP * ~NOTFOUND"));
    }

    #[test]
    #[ignore = "needs Microsoft Edge"]
    fn a_previewed_page_cannot_reach_other_addresses() {
        use std::io::{Read, Write};
        let dir = tempfile::tempdir().unwrap();
        // Another local service the page must not touch.
        let canary = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let canary_port = canary.local_addr().unwrap().port();
        canary.set_nonblocking(true).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let body = format!(
                    "<html><body><h1>probe</h1><script>
                    fetch('http://127.0.0.1:{canary_port}/a', {{mode: 'no-cors'}}).catch(() => {{}});
                    new Image().src = 'http://localhost:{canary_port}/b';
                    fetch('http://other.test:{canary_port}/c', {{mode: 'no-cors'}}).catch(() => {{}});
                    </script></body></html>"
                );
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        let t = Thumbnails::new(&AppPaths::new(dir.path()));
        t.capture("probe", &format!("http://probe.test:{port}/")).unwrap();
        assert!(canary.accept().is_err(), "the page reached another local service");
        let left: Vec<_> = std::fs::read_dir(dir.path().join("cache").join("thumbnails")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("probe.png")], "the profile is removed after the capture");
    }

    #[test]
    #[ignore = "needs Microsoft Edge"]
    fn captures_a_page_with_edge() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let body = "<html><body style='background:#0b8a65'><h1>preview</h1></body></html>";
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        let t = Thumbnails::new(&AppPaths::new(dir.path()));
        let thumb = t.capture("site1", &format!("http://preview.test:{port}/")).unwrap();
        assert!(thumb.data_url.starts_with("data:image/png;base64,iVBORw0K"));
        assert!(is_png(&t.file("site1")));
        assert!(t.get("missing").unwrap().is_none());
    }
}
