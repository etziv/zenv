//! Can an app be served by FrankenPHP (Caddy) or Nginx, or does it need
//! Apache? Only Apache reads `.htaccess`, so an app whose `.htaccess` files do
//! more than the usual "send everything to index.php" needs Apache. Files are
//! only read.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{Site, Strategy};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerCompat {
    /// No Apache-only rules: FrankenPHP, Caddy and Nginx serve it as well.
    pub frankenphp_ready: bool,
    /// Why it needs Apache, e.g. "public/.htaccess: php_value".
    pub needs_apache: Vec<String>,
}

const SKIP_DIRS: &[&str] = &["vendor", "node_modules", ".git", "storage", "bootstrap", "cache", "tmp"];
const MAX_DEPTH: usize = 4;
const MAX_FILES: usize = 40;

/// Lines every front-controller `.htaccess` has (Laravel, WordPress,
/// CodeIgniter, Symfony): Caddy's `php_fastcgi` and Nginx's `try_files`
/// already do the same.
fn is_standard(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    let l = l.trim();
    if l.is_empty() || l.starts_with('#') {
        return true;
    }
    let directive = l.split_whitespace().next().unwrap_or_default();
    match directive {
        "<ifmodule" | "</ifmodule>" | "options" | "rewriteengine" | "rewritebase" | "directoryindex" | "adddefaultcharset" => true,
        "rewritecond" => {
            l.contains("%{request_filename}")
                || l.contains("%{http:authorization}")
                || l.contains("%{http:x-xsrf-token}")
                || l.contains("%{request_uri} (.+)/$")
                || l.contains("%{request_uri}::$0")
                || l.contains("%{env:redirect_status}")
        }
        "rewriterule" => {
            let mut parts = l.split_whitespace().skip(1);
            let (pattern, target) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            // Catch-all patterns only: a rule for specific URLs is routing
            // that only Apache would apply.
            let catch_all = matches!(pattern, "^" | "." | ".*" | "^.*$" | "^(.*)$" | "^(.*)" | "(.*)" | "^(.+)$")
                || pattern.starts_with(r"^index\.php");
            // Laravel 5's "remove the trailing slash" redirect.
            (pattern == "^(.*)/$" && target == "/$1")
                || (catch_all
                    && (target.trim_start_matches('/').starts_with("index.php")
                        || target == "-"
                        || target == "%1"
                        || target.starts_with("%{env:base}")))
        }
        _ => false,
    }
}

fn htaccess_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if out.len() >= MAX_FILES {
            return;
        }
        let f = dir.join(".htaccess");
        if f.is_file() {
            out.push(f);
        }
        if depth >= MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if e.file_type().is_ok_and(|t| t.is_dir()) && !SKIP_DIRS.contains(&name.as_str()) {
                walk(&e.path(), depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out
}

/// What serves `site`: static files and proxies work everywhere; PHP apps
/// are checked for Apache-only `.htaccess` rules.
pub fn check(site: &Site) -> ServerCompat {
    if site.strategy != Strategy::Php {
        return ServerCompat { frankenphp_ready: true, needs_apache: vec![] };
    }
    let mut needs = Vec::new();
    for file in htaccess_files(&site.document_root) {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let rel = file.strip_prefix(&site.project_root).unwrap_or(&file).display().to_string().replace('\\', "/");
        let mut directives: Vec<String> = text
            .lines()
            .filter(|l| !is_standard(l))
            .filter_map(|l| l.split_whitespace().next().map(|d| d.trim_start_matches('<').to_string()))
            .collect();
        directives.dedup();
        if !directives.is_empty() {
            directives.truncate(3);
            needs.push(format!("{rel}: {}", directives.join(", ")));
        }
    }
    ServerCompat { frankenphp_ready: needs.is_empty(), needs_apache: needs }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARAVEL: &str = r#"<IfModule mod_rewrite.c>
    <IfModule mod_negotiation.c>
        Options -MultiViews -Indexes
    </IfModule>

    RewriteEngine On

    # Handle Authorization Header
    RewriteCond %{HTTP:Authorization} .
    RewriteRule .* - [E=HTTP_AUTHORIZATION:%{HTTP:Authorization}]

    # Redirect Trailing Slashes If Not A Folder...
    RewriteCond %{REQUEST_FILENAME} !-d
    RewriteCond %{REQUEST_URI} (.+)/$
    RewriteRule ^ %1 [L,R=301]

    # Send Requests To Front Controller...
    RewriteCond %{REQUEST_FILENAME} !-d
    RewriteCond %{REQUEST_FILENAME} !-f
    RewriteRule ^ index.php [L]
</IfModule>
"#;

    const WORDPRESS: &str = "# BEGIN WordPress\n<IfModule mod_rewrite.c>\nRewriteEngine On\nRewriteBase /\nRewriteRule ^index\\.php$ - [L]\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteCond %{REQUEST_FILENAME} !-d\nRewriteRule . /index.php [L]\n</IfModule>\n# END WordPress\n";

    #[test]
    fn standard_front_controllers_are_ready() {
        for text in [LARAVEL, WORDPRESS, "RewriteEngine on\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^(.*)$ index.php/$1 [L]\n"] {
            assert!(text.lines().all(is_standard), "{text}");
        }
        assert!(is_standard("RewriteRule ^(.*)/$ /$1 [L,R=301]"), "Laravel 5 trailing slash");
        assert!(!is_standard("RewriteRule ^old-page$ index.php?from=htaccess [L]"), "routing for a specific URL");
    }

    #[test]
    fn apache_only_rules_need_apache() {
        let dir = tempfile::tempdir().unwrap();
        let public = dir.path().join("public");
        std::fs::create_dir_all(public.join("uploads")).unwrap();
        std::fs::write(public.join(".htaccess"), format!("{LARAVEL}\nphp_value upload_max_filesize 64M\nRewriteRule ^old/(.*)$ /new/$1 [R=301,L]\n")).unwrap();
        std::fs::write(public.join("uploads").join(".htaccess"), "Deny from all\n").unwrap();
        let mut site = crate::local_sites::tests::sample_site(dir.path(), &public);
        let c = check(&site);
        assert!(!c.frankenphp_ready);
        assert_eq!(c.needs_apache, vec!["public/.htaccess: php_value, RewriteRule", "public/uploads/.htaccess: Deny"]);

        std::fs::write(public.join(".htaccess"), LARAVEL).unwrap();
        std::fs::remove_file(public.join("uploads").join(".htaccess")).unwrap();
        assert!(check(&site).frankenphp_ready);

        site.strategy = Strategy::Static;
        std::fs::write(public.join(".htaccess"), "Deny from all\n").unwrap();
        assert!(check(&site).frankenphp_ready, "only PHP apps are checked");
    }
}
