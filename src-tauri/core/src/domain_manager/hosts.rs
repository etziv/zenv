//! Windows hosts-file management.
//!
//! Zenv owns exactly one marked block in the hosts file and never edits lines
//! outside it. Writing needs administrator rights. The unelevated app writes a
//! small request file, then starts `zenv.exe --zenv-apply-hosts <request>`
//! through UAC. That elevated helper strictly validates the request and
//! recomputes the change against the current file itself.


use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{resolution, validate_domain, Resolution};
use crate::error::{AppError, AppResult, IoContext};
use crate::util::file_timestamp;

pub const BEGIN: &str = "# BEGIN zenv - managed by Zenv; lines inside this block are rewritten";
pub const END: &str = "# END zenv";
pub const HELPER_FLAG: &str = "--zenv-apply-hosts";
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const ALLOWED_IPS: &[&str] = &["127.0.0.1", "::1"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsEntry {
    pub ip: String,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsConflict {
    pub domain: String,
    /// Where an existing, non-Zenv line points the domain instead.
    pub mapped_to: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsPlan {
    #[serde(skip)]
    pub content: String,
    pub changed: bool,
    pub add: Vec<HostsEntry>,
    pub remove: Vec<HostsEntry>,
    /// Already mapped correctly by the user's own lines; not duplicated.
    pub already_mapped: Vec<String>,
    /// Mapped elsewhere by the user's own lines; left alone and reported.
    pub conflicts: Vec<HostsConflict>,
    /// The block exactly as it will appear in the file.
    pub block: Vec<String>,
}

pub fn system_hosts_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join("drivers").join("etc").join("hosts")
}

struct Split<'a> {
    before: Vec<&'a str>,
    block: Vec<&'a str>,
    after: Vec<&'a str>,
    had_block: bool,
}

fn split(content: &str) -> Split<'_> {
    let lines: Vec<&str> = content.lines().collect();
    // Only a complete BEGIN…END pair counts; a stray marker (e.g. from a
    // manual edit) stays an ordinary comment so no user line is ever swallowed.
    let end = lines.iter().rposition(|l| l.trim() == END);
    let begin = end.and_then(|e| lines[..e].iter().rposition(|l| l.trim_start().starts_with("# BEGIN zenv")));
    let mut split = match (begin, end) {
        (Some(b), Some(e)) => Split {
            before: lines[..b].to_vec(),
            block: lines[b + 1..e].to_vec(),
            after: lines[e + 1..].to_vec(),
            had_block: true,
        },
        _ => Split { before: lines, block: vec![], after: vec![], had_block: false },
    };
    // Lines marked `# zenv` outside the block were added for Zenv by hand (or
    // appended when the file could not be rewritten); they are Zenv's too and
    // move into the block on the next rewrite.
    let marked: Vec<&str> = split.before.iter().chain(split.after.iter()).copied().filter(|l| is_marked(l)).collect();
    if !marked.is_empty() {
        split.before.retain(|l| !is_marked(l));
        split.after.retain(|l| !is_marked(l));
        split.block.extend(marked);
    }
    split
}

/// `127.0.0.1  shop.test  # zenv`
fn is_marked(line: &str) -> bool {
    line.split_once('#').is_some_and(|(entry, comment)| !entry.trim().is_empty() && comment.trim() == "zenv")
}

/// `ip name name...` lines, comments and blanks skipped.
fn entries<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<HostsEntry> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(ip) = parts.next() else { continue };
        for name in parts {
            out.push(HostsEntry { ip: ip.to_string(), domain: name.to_ascii_lowercase() });
        }
    }
    out
}

/// Domains mapped by lines outside Zenv's block, with every IP they map to.
pub fn external_mappings(content: &str) -> BTreeMap<String, Vec<String>> {
    let s = split(content);
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in entries(s.before.iter().chain(s.after.iter()).copied()) {
        map.entry(e.domain).or_default().push(e.ip);
    }
    map
}

pub fn managed_entries(content: &str) -> Vec<HostsEntry> {
    entries(split(content).block)
}

/// Computes the new file content for the desired set of Zenv entries.
pub fn plan(current: &str, desired: &[HostsEntry]) -> HostsPlan {
    let eol = if current.contains("\r\n") { "\r\n" } else { "\n" };
    let external = external_mappings(current);
    let existing_block = managed_entries(current);

    let mut wanted: Vec<HostsEntry> = Vec::new();
    let mut already_mapped = Vec::new();
    let mut conflicts = Vec::new();
    let mut desired: Vec<HostsEntry> = desired.to_vec();
    desired.sort();
    desired.dedup();
    for e in desired {
        match external.get(&e.domain) {
            Some(ips) if ips.contains(&e.ip) => already_mapped.push(e.domain.clone()),
            Some(ips) => conflicts.push(HostsConflict { domain: e.domain.clone(), mapped_to: ips[0].clone() }),
            None => wanted.push(e),
        }
    }
    already_mapped.dedup();

    let block: Vec<String> = if wanted.is_empty() {
        vec![]
    } else {
        std::iter::once(BEGIN.to_string())
            .chain(wanted.iter().map(|e| format!("{}\t{}", e.ip, e.domain)))
            .chain(std::iter::once(END.to_string()))
            .collect()
    };

    let s = split(current);
    let mut lines: Vec<String> = s.before.iter().map(|l| l.to_string()).collect();
    if s.had_block {
        if block.is_empty() && s.after.is_empty() {
            // Drop the blank separator Zenv added when it appended the block.
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
        }
        lines.extend(block.iter().cloned());
        lines.extend(s.after.iter().map(|l| l.to_string()));
    } else if !block.is_empty() {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.extend(block.iter().cloned());
    }
    let mut content = lines.join(eol);
    if !content.is_empty() {
        content.push_str(eol);
    }

    let add = wanted.iter().filter(|e| !existing_block.contains(e)).cloned().collect();
    let remove = existing_block.iter().filter(|e| !wanted.contains(e)).cloned().collect();
    HostsPlan {
        changed: content != current,
        content,
        add,
        remove,
        already_mapped,
        conflicts,
        block,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainState {
    /// `*.localhost`: no hosts entry needed.
    NotNeeded,
    /// Present in Zenv's block.
    Managed,
    /// Mapped to the right address by the user's own line.
    External,
    /// Mapped to a different address by the user's own line.
    Conflict,
    Missing,
}

pub fn domain_state(content: &str, domain: &str, ip: &str) -> DomainState {
    if resolution(domain) == Resolution::Loopback {
        return DomainState::NotNeeded;
    }
    if let Some(ips) = external_mappings(content).get(domain) {
        return if ips.iter().any(|i| i == ip) { DomainState::External } else { DomainState::Conflict };
    }
    if managed_entries(content).iter().any(|e| e.domain == domain && e.ip == ip) {
        DomainState::Managed
    } else {
        DomainState::Missing
    }
}

pub fn read_hosts(path: &Path) -> AppResult<String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).ctx(|| format!("reading {}", path.display())),
    }
}

/// In-place rewrite keeps the hosts file's ACLs and attributes intact.
fn write_hosts(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).truncate(true).create(true).open(path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMethod {
    Unchanged,
    /// Zenv already had write access (it runs elevated).
    Direct,
    /// Written by the helper after the user approved the UAC prompt.
    Elevated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub method: ApplyMethod,
    pub plan: HostsPlan,
    pub backup: Option<PathBuf>,
}

/// Brings Zenv's block in the system hosts file to `desired`.
/// `work_dir` holds the request file and backups.
pub fn apply(desired: &[HostsEntry], work_dir: &Path) -> AppResult<ApplyOutcome> {
    for e in desired {
        validate_entry(e)?;
    }
    let path = system_hosts_path();
    let current = read_hosts(&path)?;
    let plan = plan(&current, desired);
    if !plan.changed {
        return Ok(ApplyOutcome { method: ApplyMethod::Unchanged, plan, backup: None });
    }

    std::fs::create_dir_all(work_dir).ctx(|| format!("creating {}", work_dir.display()))?;
    let backup = work_dir.join(format!("hosts-backup-{}", file_timestamp()));
    std::fs::write(&backup, &current).ctx(|| format!("writing {}", backup.display()))?;

    let method = match write_hosts(&path, &plan.content) {
        Ok(()) => ApplyMethod::Direct,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let request = work_dir.join("hosts-request.txt");
            let body: String = desired.iter().map(|e| format!("{} {}
", e.ip, e.domain)).collect();
            std::fs::write(&request, body).ctx(|| format!("writing {}", request.display()))?;
            let exe = std::env::current_exe().ctx(|| "locating zenv.exe".into())?;
            ensure_safe_to_elevate(&exe)?;
            let code = crate::platform::run_elevated(&exe, &format!("{HELPER_FLAG} \"{}\"", request.display()))?;
            match code {
                0 => ApplyMethod::Elevated,
                EXIT_DENIED => {
                    let missing: Vec<HostsEntry> =
                        desired.iter().filter(|e| domain_state(&current, &e.domain, &e.ip) == DomainState::Missing).cloned().collect();
                    return Err(AppError::Conflict(blocked_message(&missing)));
                }
                _ => {
                    return Err(AppError::Platform(format!(
                        "the hosts helper failed ({}); the hosts file was not changed. Backup: {}",
                        exit_reason(code),
                        backup.display()
                    )));
                }
            }
        }
        Err(e) => return Err(e).ctx(|| format!("writing {}", path.display())),
    };

    // Confirm the change actually landed (antivirus can silently revert it).
    let after = read_hosts(&path)?;
    if self::plan(&after, desired).changed {
        return Err(AppError::Platform(format!(
            "{} still differs from what Zenv wrote; another program (often antivirus) may be protecting it",
            path.display()
        )));
    }
    Ok(ApplyOutcome { method, plan, backup: Some(backup) })
}

fn validate_entry(e: &HostsEntry) -> AppResult<()> {
    if !ALLOWED_IPS.contains(&e.ip.as_str()) {
        return Err(AppError::Validation(format!("hosts entries may only point to loopback, not {}", e.ip)));
    }
    let domain = validate_domain(&e.domain)?;
    if domain != e.domain || resolution(&domain) != Resolution::HostsFile {
        return Err(AppError::Validation(format!("'{}' does not need a hosts entry", e.domain)));
    }
    Ok(())
}

/// Helper exit codes besides 0 (done).
const EXIT_INVALID: i32 = 10;
const EXIT_READ: i32 = 11;
const EXIT_WRITE: i32 = 12;
/// Writing was refused even with administrator rights (antivirus).
const EXIT_DENIED: u32 = 13;

fn exit_reason(code: u32) -> String {
    match code as i32 {
        EXIT_INVALID => "the request was rejected".into(),
        EXIT_READ => "the hosts file could not be read".into(),
        EXIT_WRITE => "the hosts file could not be written".into(),
        _ => format!("exit code {code}"),
    }
}

/// Approving the UAC prompt runs `exe` with administrator rights, together
/// with any DLL next to it. If another account on this PC could change that
/// folder, the prompt would hand it administrator rights; refuse instead.
fn ensure_safe_to_elevate(exe: &Path) -> AppResult<()> {
    use crate::platform::FolderAccess;
    let dir = exe.parent().unwrap_or(exe);
    for target in [dir, exe] {
        let problem = match crate::platform::folder_access(target)? {
            FolderAccess::Private => continue,
            FolderAccess::Shared(_) => "other accounts on this computer can change it",
            FolderAccess::ForeignOwner(_) => "it belongs to another account on this computer",
        };
        return Err(AppError::Conflict(format!(
            "Zenv did not ask for administrator rights because {problem}: {}. Restart Zenv so it can lock its folder, or \
             reinstall it into a folder of its own (such as C:\\zenv).",
            target.display()
        )));
    }
    Ok(())
}

/// Elevated helper body: validates a request file and rewrites `hosts_path`.
/// It writes nothing else: the outcome is its exit code.
pub fn apply_request_file(request: &Path, hosts_path: &Path) -> Result<(), (i32, String)> {
    let meta = std::fs::metadata(request).map_err(|e| (EXIT_INVALID, format!("request unreadable: {e}")))?;
    if meta.len() > MAX_REQUEST_BYTES {
        return Err((EXIT_INVALID, "request too large".into()));
    }
    let text = std::fs::read_to_string(request).map_err(|e| (EXIT_INVALID, format!("request unreadable: {e}")))?;
    let mut desired = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let mut parts = line.split_whitespace();
        let (Some(ip), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err((EXIT_INVALID, format!("malformed request line: {line}")));
        };
        let entry = HostsEntry { ip: ip.to_string(), domain: domain.to_string() };
        validate_entry(&entry).map_err(|e| (EXIT_INVALID, e.to_string()))?;
        desired.push(entry);
    }
    let current = read_hosts(hosts_path).map_err(|e| (EXIT_READ, e.to_string()))?;
    let plan = plan(&current, &desired);
    if plan.changed {
        write_hosts(hosts_path, &plan.content).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => (EXIT_DENIED as i32, e.to_string()),
            _ => (EXIT_WRITE, e.to_string()),
        })?;
    }
    Ok(())
}

/// What to tell the user when even the elevated helper may not write.
fn blocked_message(entries: &[HostsEntry]) -> String {
    let lines: Vec<String> = entries.iter().map(|e| format!("{} {}", e.ip, e.domain)).collect();
    format!(
        "Windows allowed it, but security software on this computer (for example Kaspersky) does not let Zenv change the \
         hosts file. Either mark {} as a trusted application in it and try again, use a domain ending in .localhost (it \
         needs no hosts entry), or add these lines to the hosts file yourself: {}",
        std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "zenv.exe".into()),
        lines.join(", ")
    )
}

/// Called from `main` before the GUI starts. Returns an exit code when this
/// process was launched as the elevated hosts helper.
pub fn run_helper_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some(HELPER_FLAG) {
        return None;
    }
    let Some(request) = args.get(2) else { return Some(EXIT_INVALID) };
    Some(match apply_request_file(Path::new(request), &system_hosts_path()) {
        Ok(()) => 0,
        Err((code, msg)) => {
            eprintln!("zenv hosts helper: {msg}");
            code
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(ip: &str, d: &str) -> HostsEntry {
        HostsEntry { ip: ip.into(), domain: d.into() }
    }

    const WINDOWS_DEFAULT: &str = "# Copyright (c) 1993-2009 Microsoft Corp.\r\n#\r\n# localhost name resolution is handled within DNS itself.\r\n#\t127.0.0.1       localhost\r\n#\t::1             localhost\r\n";

    #[test]
    fn lines_marked_zenv_are_zenv_entries() {
        let current = "127.0.0.1 localhost
127.0.0.1	gone.test	# zenv
127.0.0.1	keep.test # zenv
127.0.0.1 mine.test # my note
";
        assert_eq!(managed_entries(current), vec![e("127.0.0.1", "gone.test"), e("127.0.0.1", "keep.test")]);
        assert!(!external_mappings(current).contains_key("keep.test"));
        let p = plan(current, &[e("127.0.0.1", "keep.test")]);
        assert!(p.changed);
        assert_eq!(p.remove, vec![e("127.0.0.1", "gone.test")]);
        assert_eq!(
            p.content,
            format!("127.0.0.1 localhost
127.0.0.1 mine.test # my note

{BEGIN}
127.0.0.1	keep.test
{END}
")
        );
        assert!(!plan(&p.content, &[e("127.0.0.1", "keep.test")]).changed, "stable afterwards");
    }

    #[test]
    fn appends_block_and_preserves_existing_content_and_crlf() {
        let p = plan(WINDOWS_DEFAULT, &[e("127.0.0.1", "shop.test"), e("127.0.0.1", "blog.test")]);
        assert!(p.changed);
        assert!(p.content.starts_with(WINDOWS_DEFAULT));
        assert!(p.content.contains("127.0.0.1\tblog.test\r\n127.0.0.1\tshop.test\r\n"), "sorted, CRLF: {:?}", p.content);
        assert!(!p.content.replace("\r\n", "").contains('\n'), "no bare LF introduced");
        assert_eq!(p.add.len(), 2);
    }

    #[test]
    fn applying_the_same_set_twice_is_a_no_op() {
        let first = plan(WINDOWS_DEFAULT, &[e("127.0.0.1", "shop.test")]);
        let second = plan(&first.content, &[e("127.0.0.1", "shop.test")]);
        assert!(!second.changed);
        assert!(second.add.is_empty() && second.remove.is_empty());
    }

    #[test]
    fn replaces_block_in_place_and_removes_it_when_empty() {
        let with_block = format!(
            "10.0.0.1 nas\n{BEGIN}\n127.0.0.1\told.test\n{END}\n192.168.1.1 router\n"
        );
        let p = plan(&with_block, &[e("127.0.0.1", "new.test")]);
        assert_eq!(
            p.content,
            format!("10.0.0.1 nas\n{BEGIN}\n127.0.0.1\tnew.test\n{END}\n192.168.1.1 router\n")
        );
        assert_eq!(p.remove, vec![e("127.0.0.1", "old.test")]);
        let cleared = plan(&p.content, &[]);
        assert_eq!(cleared.content, "10.0.0.1 nas\n192.168.1.1 router\n");
    }

    #[test]
    fn user_lines_are_never_duplicated_or_overridden() {
        let current = "127.0.0.1 mine.test\n10.1.1.1 elsewhere.test # staging\n";
        let p = plan(current, &[e("127.0.0.1", "mine.test"), e("127.0.0.1", "elsewhere.test"), e("127.0.0.1", "new.test")]);
        assert_eq!(p.already_mapped, vec!["mine.test"]);
        assert_eq!(p.conflicts, vec![HostsConflict { domain: "elsewhere.test".into(), mapped_to: "10.1.1.1".into() }]);
        assert!(p.content.starts_with(current));
        assert_eq!(managed_entries(&p.content), vec![e("127.0.0.1", "new.test")]);
    }

    #[test]
    fn stray_begin_marker_never_swallows_user_lines() {
        let current = "# BEGIN zenv - half deleted\n10.0.0.5 keep.me\n";
        let p = plan(current, &[e("127.0.0.1", "a.test")]);
        assert!(p.content.starts_with(current), "{:?}", p.content);
        let again = plan(&p.content, &[]);
        assert_eq!(again.content, current);
    }

    #[test]
    fn reports_domain_states() {
        let content = format!("127.0.0.1 mine.test\n10.0.0.2 other.test\n{BEGIN}\n127.0.0.1\tz.test\n{END}\n");
        assert_eq!(domain_state(&content, "z.test", "127.0.0.1"), DomainState::Managed);
        assert_eq!(domain_state(&content, "mine.test", "127.0.0.1"), DomainState::External);
        assert_eq!(domain_state(&content, "other.test", "127.0.0.1"), DomainState::Conflict);
        assert_eq!(domain_state(&content, "nope.test", "127.0.0.1"), DomainState::Missing);
        assert_eq!(domain_state(&content, "app.localhost", "127.0.0.1"), DomainState::NotNeeded);
    }

    #[test]
    fn helper_applies_valid_requests_to_a_given_file() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = dir.path().join("hosts");
        std::fs::write(&hosts, WINDOWS_DEFAULT).unwrap();
        let req = dir.path().join("req.txt");
        std::fs::write(&req, "127.0.0.1 shop.test\n::1 shop.test\n").unwrap();
        apply_request_file(&req, &hosts).unwrap();
        let out = std::fs::read_to_string(&hosts).unwrap();
        assert!(out.starts_with(WINDOWS_DEFAULT));
        assert_eq!(managed_entries(&out).len(), 2);
    }

    #[test]
    fn helper_rejects_anything_but_loopback_local_domains() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = dir.path().join("hosts");
        std::fs::write(&hosts, "original\n").unwrap();
        for bad in [
            "10.0.0.1 shop.test",
            "127.0.0.1 www.google.com extra",
            "127.0.0.1 app.localhost",
            "127.0.0.1 bad_name.test",
            "garbage",
        ] {
            let req = dir.path().join("req.txt");
            std::fs::write(&req, format!("{bad}\n")).unwrap();
            let (code, _) = apply_request_file(&req, &hosts).unwrap_err();
            assert_eq!(code, 10, "{bad}");
        }
        assert_eq!(std::fs::read_to_string(&hosts).unwrap(), "original\n", "file untouched");
    }
}
