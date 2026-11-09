//! Local domain rules and resolution strategy.
//!
//! Two strategies exist:
//! - `*.localhost` names resolve to loopback inside browsers and curl without
//!   any system change (RFC 6761), so they never need administrator rights.
//! - `*.test`, `*.example` and `*.invalid` (reserved by RFC 2606) need a
//!   hosts-file entry, which Zenv writes only on explicit request.
//!
//! Other endings are refused: they are real names, on the internet
//! (github.com) or on networks (a router's .lan, a company's .internal), and
//! pointing them at this computer would take them over.

pub mod hosts;

use serde::Serialize;

use crate::error::{AppError, AppResult};

pub const DEFAULT_TLD: &str = "test";

/// Endings no real network uses (RFC 2606, RFC 6761).
pub const LOCAL_TLDS: &[&str] = &["test", "localhost", "example", "invalid"];

/// TLDs on the HSTS preload list: browsers force HTTPS for them, so a plain
/// HTTP local site could never be opened (HTTPS arrives in milestone 6).
const HSTS_TLDS: &[&str] = &["dev", "app", "page", "new", "day", "foo"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Needs a line in the Windows hosts file.
    HostsFile,
    /// `*.localhost`: resolved by browsers and curl, no system change.
    Loopback,
}

pub fn resolution(domain: &str) -> Resolution {
    if domain == "localhost" || domain.ends_with(".localhost") {
        Resolution::Loopback
    } else {
        Resolution::HostsFile
    }
}

/// Normalizes and validates a local domain. Returns the lowercase form.
pub fn validate_domain(raw: &str) -> AppResult<String> {
    let domain = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let invalid = |why: &str| Err(AppError::Validation(format!("'{raw}' is not a usable local domain: {why}")));
    if domain.is_empty() {
        return invalid("it is empty");
    }
    if domain.len() > 253 {
        return invalid("it is longer than 253 characters");
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return invalid(&format!("add a suffix such as .{DEFAULT_TLD} or .localhost"));
    }
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return invalid("each part between dots must be 1–63 characters");
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return invalid("use only letters, digits, dots and hyphens");
        }
        if label.starts_with('-') || label.ends_with('-') {
            return invalid("a part cannot start or end with a hyphen");
        }
    }
    let tld = labels[labels.len() - 1];
    if tld.chars().all(|c| c.is_ascii_digit()) {
        return invalid("it looks like an IP address");
    }
    if HSTS_TLDS.contains(&tld) {
        return invalid(&format!(
            "browsers force HTTPS for .{tld}, so it cannot be served over plain HTTP; use .{DEFAULT_TLD} or .localhost"
        ));
    }
    if tld == "local" {
        return invalid(".local is reserved for multicast DNS (Bonjour) and resolves unreliably; use .test or .localhost");
    }
    if !LOCAL_TLDS.contains(&tld) {
        return invalid(&format!(
            ".{tld} names are real names (on the internet, or on networks like a router's .lan); use .{DEFAULT_TLD} or .localhost"
        ));
    }
    Ok(domain)
}

/// Folder name → domain label, e.g. `My Laravel_App` → `my-laravel-app`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    let label: String = trimmed.chars().take(63).collect();
    let label = label.trim_end_matches('-');
    if label.is_empty() { "site".to_string() } else { label.to_string() }
}

pub fn suggest_domain(folder_name: &str) -> String {
    format!("{}.{DEFAULT_TLD}", slug(folder_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_common_local_domains() {
        assert_eq!(validate_domain("My-App.TEST").unwrap(), "my-app.test");
        assert_eq!(validate_domain(" api.shop.localhost. ").unwrap(), "api.shop.localhost");
        assert_eq!(validate_domain("blog.example").unwrap(), "blog.example");
    }

    #[test]
    fn rejects_unusable_domains_with_reasons() {
        for (input, needle) in [
            ("", "empty"),
            ("myapp", "suffix"),
            ("my_app.test", "letters, digits"),
            ("-app.test", "hyphen"),
            ("a..test", "1–63"),
            ("127.0.0.1", "IP address"),
            ("site.dev", "HTTPS"),
            ("site.app", "HTTPS"),
            ("site.local", "multicast"),
            ("github.com", "real names"),
            ("router.lan", "real names"),
            ("intranet.internal", "real names"),
        ] {
            let err = validate_domain(input).unwrap_err().to_string();
            assert!(err.contains(needle), "{input}: {err}");
        }
        assert!(validate_domain(&format!("{}.test", "a".repeat(64))).is_err());
    }

    #[test]
    fn slugs_follow_folder_names() {
        assert_eq!(slug("my-laravel-app"), "my-laravel-app");
        assert_eq!(slug("My Laravel_App (v2)"), "my-laravel-app-v2");
        assert_eq!(slug("___"), "site");
        assert_eq!(suggest_domain("Shop"), "shop.test");
        assert!(validate_domain(&suggest_domain(&"x".repeat(100))).is_ok());
    }

    #[test]
    fn localhost_names_need_no_hosts_entry() {
        assert_eq!(resolution("app.localhost"), Resolution::Loopback);
        assert_eq!(resolution("app.test"), Resolution::HostsFile);
        assert_eq!(resolution("localhost.test"), Resolution::HostsFile);
    }
}
