use eframe::egui::{Response, Role, WidgetInfo};
use zenv_core::local_sites::{PhpEngine, ProjectType, Site, SiteInput, Strategy};
use zenv_core::runtime_manager::Installation;

pub fn name(r: &Response, role: Role, label: &str) {
    r.widget_info(|| WidgetInfo::labeled(role, true, label));
}

// ---------- labels

pub fn project_type_label(t: ProjectType) -> &'static str {
    match t {
        ProjectType::Laravel => "Laravel",
        ProjectType::Php => "PHP",
        ProjectType::Static => "Static site",
        ProjectType::Go => "Go app",
        ProjectType::Rust => "Rust app",
        ProjectType::Python => "Python app",
        ProjectType::Ruby => "Ruby app",
        ProjectType::Custom => "Custom",
    }
}

pub const PROJECT_TYPES: [ProjectType; 8] = [
    ProjectType::Laravel,
    ProjectType::Php,
    ProjectType::Static,
    ProjectType::Go,
    ProjectType::Rust,
    ProjectType::Python,
    ProjectType::Ruby,
    ProjectType::Custom,
];

pub fn strategy_label(s: Strategy) -> &'static str {
    match s {
        Strategy::Php => "PHP",
        Strategy::Static => "Static files",
        Strategy::ReverseProxy => "Proxy to app",
    }
}

pub fn strategy_hint(s: Strategy) -> &'static str {
    match s {
        Strategy::Php => "Laravel and other PHP apps, with the PHP version you pick.",
        Strategy::Static => "Plain files from the document root.",
        Strategy::ReverseProxy => "An app you run yourself on a local port.",
    }
}

pub fn backend_label(backend: &str) -> &'static str {
    match backend {
        "frankenphp" => "FrankenPHP",
        _ => "PHP-CGI",
    }
}

pub fn engine_label(e: PhpEngine) -> &'static str {
    match e {
        PhpEngine::Auto => "Automatic",
        PhpEngine::Fastcgi => "PHP-CGI",
        PhpEngine::Frankenphp => "FrankenPHP",
    }
}

pub fn engine_label_long(e: PhpEngine) -> &'static str {
    match e {
        PhpEngine::Auto => "Automatic (FrankenPHP when it has this PHP version)",
        PhpEngine::Fastcgi => "PHP-CGI through the web server",
        PhpEngine::Frankenphp => "FrankenPHP",
    }
}

// ---------- domain/path helpers

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in s.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out.truncate(50);
    if out.is_empty() { "app".to_owned() } else { out }
}

pub fn service_name_for(domain: &str) -> String {
    let label = domain.split('.').next().unwrap_or("");
    let s = slug(label);
    let s: String = s.chars().take(63).collect();
    if s.is_empty() { "app".to_owned() } else { s }
}

pub fn valid_service_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }
    let bytes = s.as_bytes();
    let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    alnum(bytes[0]) && alnum(bytes[bytes.len() - 1]) && bytes.iter().all(|&b| alnum(b) || b == b'-')
}

pub fn relative_doc_root(project_root: &str, document_root: &str) -> String {
    let norm = |s: &str| s.trim_end_matches(['\\', '/']).to_owned();
    let root = norm(project_root);
    let doc = norm(document_root);
    if doc.to_lowercase() == root.to_lowercase() {
        return ".".to_owned();
    }
    for sep in ['\\', '/'] {
        let prefix = format!("{}{sep}", root.to_lowercase());
        if doc.to_lowercase().starts_with(&prefix) {
            return doc[root.len() + 1..].to_owned();
        }
    }
    document_root.to_owned()
}

pub fn to_input(site: &Site) -> SiteInput {
    SiteInput {
        name: site.name.clone(),
        domain: site.domain.clone(),
        project_root: site.project_root.to_string_lossy().into_owned(),
        document_root: site.document_root.to_string_lossy().into_owned(),
        project_type: site.project_type,
        service_id: site.service_id.clone(),
        strategy: site.strategy,
        upstream_port: site.upstream_port,
        runtime_installation_id: site.runtime_installation_id.clone(),
        php_engine: site.php_engine,
        https_enabled: site.https_enabled,
        https_redirect: site.https_redirect,
        enabled: site.enabled,
    }
}

// ---------- PHP backend resolution

pub struct PhpResolution {
    pub backend: Option<&'static str>,
    pub franken: Option<Installation>,
    pub error: Option<String>,
}

fn minor_of(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.chars().all(|c| c.is_ascii_digit()) && !major.is_empty() && minor.chars().all(|c| c.is_ascii_digit()) && !minor.is_empty() {
        Some(format!("{major}.{minor}"))
    } else {
        None
    }
}

fn embedded_php(version_detail: &str) -> Option<String> {
    let bytes = version_detail.as_bytes();
    let mut i = 0;
    while let Some(off) = version_detail[i..].find("PHP ") {
        let start = i + off + 4;
        let rest = &version_detail[start..];
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        let token = &rest[..end];
        if token.matches('.').count() >= 2 {
            return Some(token.to_owned());
        }
        i = start.max(i + off + 1);
        if i >= bytes.len() {
            break;
        }
    }
    None
}

pub fn resolve_backend(engine: PhpEngine, php: &Installation, frankens: &[Installation]) -> PhpResolution {
    let want = minor_of(&php.version);
    let mut matching: Vec<&Installation> = frankens
        .iter()
        .filter(|f| {
            f.enabled && f.executable_exists && want.is_some() && embedded_php(&f.version_detail).and_then(|v| minor_of(&v)) == want
        })
        .collect();
    matching.sort_by_key(|f| !f.is_default);
    let franken = matching.first().map(|f| (*f).clone());
    match engine {
        PhpEngine::Fastcgi => PhpResolution { backend: Some("fastcgi"), franken: None, error: None },
        PhpEngine::Auto => match franken {
            Some(f) => PhpResolution { backend: Some("frankenphp"), franken: Some(f), error: None },
            None => PhpResolution { backend: Some("fastcgi"), franken: None, error: None },
        },
        PhpEngine::Frankenphp => match franken {
            Some(f) => PhpResolution { backend: Some("frankenphp"), franken: Some(f), error: None },
            None => PhpResolution {
                backend: None,
                franken: None,
                error: Some(format!("No FrankenPHP with PHP {} is set up and switched on.", want.as_deref().unwrap_or(&php.version))),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_strips_and_joins() {
        assert_eq!(slug("My_Shop new"), "my-shop-new");
        assert_eq!(slug("   "), "app");
        assert_eq!(slug("Já!!!"), "j");
    }

    #[test]
    fn service_name_takes_first_label() {
        assert_eq!(service_name_for("Vault_Warden.test"), "vault-warden");
        assert_eq!(service_name_for(".test"), "app");
    }

    #[test]
    fn service_name_validates() {
        assert!(valid_service_name("vault-warden"));
        assert!(!valid_service_name("-bad"));
        assert!(!valid_service_name("Bad"));
        assert!(!valid_service_name(""));
    }

    #[test]
    fn relative_doc_root_handles_subpath_and_outside() {
        assert_eq!(relative_doc_root(r"C:\proj", r"C:\proj"), ".");
        assert_eq!(relative_doc_root(r"C:\proj", r"C:\proj\public"), "public");
        assert_eq!(relative_doc_root(r"C:\proj", r"D:\other"), r"D:\other");
    }

    fn php(version: &str) -> Installation {
        use std::path::PathBuf;
        use zenv_core::runtime_catalog::ComponentId;
        use zenv_core::runtime_manager::InstallSource;
        Installation {
            id: format!("php-{version}"),
            component: ComponentId::Php,
            version: version.into(),
            version_detail: String::new(),
            root_path: PathBuf::new(),
            executable_path: PathBuf::new(),
            source: InstallSource::Managed,
            is_default: false,
            enabled: true,
            executable_exists: true,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn franken(version_detail: &str, enabled: bool) -> Installation {
        use std::path::PathBuf;
        use zenv_core::runtime_catalog::ComponentId;
        use zenv_core::runtime_manager::InstallSource;
        Installation {
            id: "franken".into(),
            component: ComponentId::Frankenphp,
            version: "1.0".into(),
            version_detail: version_detail.into(),
            root_path: PathBuf::new(),
            executable_path: PathBuf::new(),
            source: InstallSource::Managed,
            is_default: true,
            enabled,
            executable_exists: true,
            detected_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn resolve_backend_auto_prefers_matching_franken() {
        let r = resolve_backend(PhpEngine::Auto, &php("8.3.14"), &[franken("FrankenPHP PHP 8.3.1 ...", true)]);
        assert_eq!(r.backend, Some("frankenphp"));
        assert!(r.franken.is_some());
    }

    #[test]
    fn resolve_backend_auto_falls_back_to_fastcgi() {
        let r = resolve_backend(PhpEngine::Auto, &php("8.3.14"), &[]);
        assert_eq!(r.backend, Some("fastcgi"));
    }

    #[test]
    fn resolve_backend_frankenphp_without_match_errors() {
        let r = resolve_backend(PhpEngine::Frankenphp, &php("8.3.14"), &[]);
        assert_eq!(r.backend, None);
        assert!(r.error.is_some());
    }

    #[test]
    fn resolve_backend_fastcgi_is_explicit() {
        let r = resolve_backend(PhpEngine::Fastcgi, &php("8.3.14"), &[franken("PHP 8.3.1", true)]);
        assert_eq!(r.backend, Some("fastcgi"));
    }
}
