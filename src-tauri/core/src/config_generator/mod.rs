//! Server-specific configuration rendering and safe, tracked file writes.
//! Templates live in `src-tauri/templates/<server>/` and are separate from the
//! adapters that build their typed contexts.

pub mod managed_files;

use std::sync::OnceLock;

use minijinja::{Environment, UndefinedBehavior};
use serde::Serialize;

use crate::error::{AppError, AppResult};

pub const APACHE_MAIN: &str = "apache/httpd.conf";
pub const APACHE_SITE: &str = "apache/site.conf";
pub const FRANKENPHP_CADDYFILE: &str = "frankenphp/Caddyfile";
pub const CADDY_MAIN: &str = "caddy/Caddyfile";
pub const CADDY_SITE: &str = "caddy/site.conf";
pub const NGINX_MAIN: &str = "nginx/nginx.conf";
pub const NGINX_SITE: &str = "nginx/site.conf";
pub const MYSQL_INI: &str = "mysql/my.ini";
pub const REDIS_CONF: &str = "redis/redis.conf";

fn environment() -> &'static Environment<'static> {
    static ENV: OnceLock<Environment<'static>> = OnceLock::new();
    ENV.get_or_init(|| {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);
        env.add_template(APACHE_MAIN, include_str!("../../templates/apache/httpd.conf.j2"))
            .expect("apache template parses");
        env.add_template(APACHE_SITE, include_str!("../../templates/apache/site.conf.j2"))
            .expect("apache site template parses");
        env.add_template(FRANKENPHP_CADDYFILE, include_str!("../../templates/frankenphp/Caddyfile.j2"))
            .expect("frankenphp template parses");
        env.add_template(CADDY_MAIN, include_str!("../../templates/caddy/Caddyfile.j2"))
            .expect("caddy template parses");
        env.add_template(CADDY_SITE, include_str!("../../templates/caddy/site.conf.j2"))
            .expect("caddy site template parses");
        env.add_template(NGINX_MAIN, include_str!("../../templates/nginx/nginx.conf.j2"))
            .expect("nginx template parses");
        env.add_template(NGINX_SITE, include_str!("../../templates/nginx/site.conf.j2"))
            .expect("nginx site template parses");
        env.add_template(MYSQL_INI, include_str!("../../templates/mysql/my.ini.j2")).expect("mysql template parses");
        env.add_template(REDIS_CONF, include_str!("../../templates/redis/redis.conf.j2")).expect("redis template parses");
        env
    })
}

/// Renders a template. Output is deterministic for a given context.
pub fn render(template: &str, ctx: &impl Serialize) -> AppResult<String> {
    let tmpl = environment()
        .get_template(template)
        .map_err(|e| AppError::Template(format!("{template}: {e}")))?;
    tmpl.render(ctx).map_err(|e| AppError::Template(format!("{template}: {e:#}")))
}

/// Rejects values that could break out of a quoted config string or inject
/// directives (quotes, newlines, control characters), and values the servers
/// would expand even inside quotes: Nginx variables (`$arg_r` would let a URL
/// pick the folder served), Caddy placeholders (`{query.r}`) and Apache's
/// `${VAR}` / `%{VAR}`.
pub fn safe_value<'a>(field: &str, value: &'a str) -> AppResult<&'a str> {
    if value.is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    if let Some(bad) = value.chars().find(|c| matches!(c, '"' | '$' | '{' | '}') || c.is_control()) {
        return Err(AppError::Validation(format!(
            "{field} contains a character that is not allowed in server configuration: {bad:?}"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_undefined_catches_missing_context() {
        #[derive(Serialize)]
        struct Empty {}
        let err = render(APACHE_MAIN, &Empty {}).unwrap_err();
        assert!(matches!(err, AppError::Template(_)));
    }

    #[test]
    fn unknown_template_is_an_error() {
        assert!(render("nope/x.conf", &()).is_err());
    }

    #[test]
    fn safe_value_rejects_injection() {
        assert!(safe_value("path", "C:/sites/app").is_ok());
        assert!(safe_value("path", "C:/a\"\nListen 1").is_err());
        assert!(safe_value("path", "a\rb").is_err());
        assert!(safe_value("path", "").is_err());
        assert!(safe_value("path", r"C:\Users\me\projects\shop (v2)").is_ok());
        for expands in ["C:/www/$arg_r", "C:/www/{query.r}", "C:/www/${SRVROOT}", "C:/www/%{HTTP_HOST}"] {
            assert!(safe_value("path", expands).is_err(), "{expands}");
        }
    }
}
