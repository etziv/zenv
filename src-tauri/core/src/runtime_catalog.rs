//! Static, reviewed metadata about every component Zenv knows about.
//! Statuses here are claims shown to users, so they must only describe what is
//! implemented and tested (see docs/COMPATIBILITY.md).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentId {
    Apache,
    Nginx,
    Caddy,
    Frankenphp,
    Php,
    Go,
    Rust,
    Ruby,
    Python,
    Node,
    Mysql,
    Postgresql,
    Redis,
    Omniroute,
    Ninerouter,
}

impl ComponentId {
    pub const ALL: [ComponentId; 15] = [
        ComponentId::Apache,
        ComponentId::Nginx,
        ComponentId::Caddy,
        ComponentId::Frankenphp,
        ComponentId::Php,
        ComponentId::Go,
        ComponentId::Rust,
        ComponentId::Ruby,
        ComponentId::Python,
        ComponentId::Node,
        ComponentId::Mysql,
        ComponentId::Postgresql,
        ComponentId::Redis,
        ComponentId::Omniroute,
        ComponentId::Ninerouter,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ComponentId::Apache => "apache",
            ComponentId::Nginx => "nginx",
            ComponentId::Caddy => "caddy",
            ComponentId::Frankenphp => "frankenphp",
            ComponentId::Php => "php",
            ComponentId::Go => "go",
            ComponentId::Rust => "rust",
            ComponentId::Ruby => "ruby",
            ComponentId::Python => "python",
            ComponentId::Node => "node",
            ComponentId::Mysql => "mysql",
            ComponentId::Postgresql => "postgresql",
            ComponentId::Redis => "redis",
            ComponentId::Omniroute => "omniroute",
            ComponentId::Ninerouter => "ninerouter",
        }
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ComponentId {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ComponentId::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| AppError::Validation(format!("unknown component '{s}'")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    WebServer,
    Language,
    Database,
    Cache,
    /// Developer tools Zenv runs as services (OmniRoute, 9router).
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsSupport {
    /// An official or de facto standard native Windows build exists.
    Native,
    /// Native builds exist with documented functional limitations.
    NativeWithCaveats,
    /// No official native Windows build; only third-party or commercial ports.
    NoOfficialBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationLevel {
    /// Start/stop/validate/logs implemented and tested against a real binary.
    ServiceManaged,
    /// Existing installations can be discovered, registered and version-probed.
    DetectionOnly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub id: ComponentId,
    pub name: &'static str,
    pub category: Category,
    pub windows_support: WindowsSupport,
    pub integration: IntegrationLevel,
    /// Managed installs are offered only once a verified provider exists.
    pub installable: bool,
    pub distribution: &'static str,
    pub notes: &'static [&'static str],
    /// Milestone in which full lifecycle support is planned.
    pub planned_milestone: u8,
}

pub fn catalog() -> &'static [ComponentInfo] {
    CATALOG
}

pub fn info(id: ComponentId) -> &'static ComponentInfo {
    CATALOG.iter().find(|c| c.id == id).expect("every ComponentId has a catalog entry")
}

use Category::*;
use IntegrationLevel::*;
use WindowsSupport::*;

static CATALOG: &[ComponentInfo] = &[
    ComponentInfo {
        id: ComponentId::Apache,
        name: "Apache HTTP Server",
        category: WebServer,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "Apache Lounge builds (the ASF publishes no Windows binaries); needs the Visual C++ 2015-2022 x64 runtime",
        notes: &[
            "Runs as a console process (mpm_winnt). Zenv stops and reloads it through Apache's named shutdown/restart events, because `httpd -k` only targets an installed Windows service.",
            "Zenv generates its own httpd.conf per service; the installation's conf/httpd.conf is never modified.",
            "mod_php needs a thread-safe PHP build with a compatible Visual C++ runtime; FastCGI via php-cgi is the alternative.",
        ],
        planned_milestone: 1,
    },
    ComponentInfo {
        id: ComponentId::Nginx,
        name: "Nginx",
        category: WebServer,
        windows_support: NativeWithCaveats,
        integration: ServiceManaged,
        installable: true,
        distribution: "nginx.org Windows zips (nginx/nginx GitHub releases)",
        notes: &[
            "nginx.org describes the Windows build as beta quality: select() only and a limited number of connections.",
            "PHP is served through FastCGI from Zenv's php-cgi workers; PHP-FPM does not exist on Windows.",
            "Zenv generates its own nginx.conf per service and controls it with `nginx -s quit|reload`.",
        ],
        planned_milestone: 3,
    },
    ComponentInfo {
        id: ComponentId::Caddy,
        name: "Caddy",
        category: WebServer,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "Official GitHub release zips",
        notes: &["PHP is served via php_fastcgi to php-cgi.exe."],
        planned_milestone: 3,
    },
    ComponentInfo {
        id: ComponentId::Frankenphp,
        name: "FrankenPHP",
        category: WebServer,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "Official Windows builds (php/frankenphp GitHub releases)",
        notes: &[
            "Worker mode and hot reload are supported natively on Windows.",
            "Bundles its own PHP (separate from any registered PHP version) and embeds Caddy; it is configured with a Caddyfile.",
        ],
        planned_milestone: 3,
    },
    ComponentInfo {
        id: ComponentId::Php,
        name: "PHP",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "windows.php.net zips (Zenv installs the non-thread-safe x64 build)",
        notes: &["PHP-FPM is not available on Windows; web integrations use php-cgi.exe or mod_php."],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Go,
        name: "Go",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "go.dev zip/MSI with published SHA-256 checksums",
        notes: &["A toolchain, not a web server. Go apps are served behind a reverse proxy."],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Rust,
        name: "Rust",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "rustup-init from static.rust-lang.org, into its own RUSTUP_HOME/CARGO_HOME",
        notes: &[
            "Zenv never changes your existing ~/.rustup or ~/.cargo; its own Rust lives under the Zenv folder.",
            "Project rust-toolchain.toml files take precedence.",
        ],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Ruby,
        name: "Ruby",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "RubyInstaller2 7z archives (GitHub releases with SHA-256 digests)",
        notes: &["Native gems need the MSYS2 DevKit that RubyInstaller can install."],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Python,
        name: "Python",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "python.org packages from the Python install manager index",
        notes: &[
            "Discovery skips WindowsApps App Execution Alias stubs, which can open the Microsoft Store.",
            "Projects should use a venv rather than the global interpreter.",
        ],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Node,
        name: "Node.js",
        category: Language,
        windows_support: Native,
        integration: DetectionOnly,
        installable: true,
        distribution: "nodejs.org Windows x64 zips (SHASUMS256)",
        notes: &[
            "npm and npx come with each version; global packages installed with npm stay with that version.",
            "Existing installs (Program Files, nvm-windows) are detected and used as they are.",
        ],
        planned_milestone: 4,
    },
    ComponentInfo {
        id: ComponentId::Mysql,
        name: "MySQL",
        category: Database,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "cdn.mysql.com ZIP archives (checked by Oracle's code signature)",
        notes: &["Data directories are never initialized, migrated or overwritten without explicit confirmation."],
        planned_milestone: 5,
    },
    ComponentInfo {
        id: ComponentId::Postgresql,
        name: "PostgreSQL",
        category: Database,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "EDB binaries (linked from postgresql.org; HTTPS only, no published checksums)",
        notes: &["Uses initdb/pg_ctl; its data-directory layout and config format differ from MySQL's."],
        planned_milestone: 5,
    },
    ComponentInfo {
        id: ComponentId::Redis,
        name: "Redis",
        category: Cache,
        windows_support: NoOfficialBuild,
        integration: ServiceManaged,
        installable: false,
        distribution: "None official. Memurai is the commercial Windows partner; community MSYS2/Cygwin builds exist",
        notes: &[
            "Redis 8 is licensed RSALv2 / SSPLv1 / AGPLv3.",
            "Zenv does not bundle or download Redis. You may register a build you have chosen yourself.",
        ],
        planned_milestone: 5,
    },
    ComponentInfo {
        id: ComponentId::Omniroute,
        name: "OmniRoute",
        category: Tool,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "npm registry (package omniroute), installed with your Node.js 22.22+ or 24+",
        notes: &[
            "One local endpoint (http://127.0.0.1:20128/v1) for many AI providers, with a dashboard.",
            "Zenv keeps it on 127.0.0.1: by default OmniRoute listens on every network and asks no API key.",
            "Zenv installs new releases automatically when auto-update is on.",
        ],
        planned_milestone: 6,
    },
    ComponentInfo {
        id: ComponentId::Ninerouter,
        name: "9router",
        category: Tool,
        windows_support: Native,
        integration: ServiceManaged,
        installable: true,
        distribution: "npm registry (package 9router), installed with your Node.js 22+",
        notes: &[
            "One local endpoint (http://127.0.0.1:20128/v1) for many AI providers and combos, with a dashboard.",
            "Zenv keeps it on 127.0.0.1: by default 9router listens on every network.",
            "Zenv installs new releases automatically when auto-update is on.",
        ],
        planned_milestone: 6,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_has_exactly_one_entry() {
        for id in ComponentId::ALL {
            assert_eq!(CATALOG.iter().filter(|c| c.id == id).count(), 1, "{id}");
        }
        assert_eq!(CATALOG.len(), ComponentId::ALL.len());
    }

    #[test]
    fn installable_matches_a_real_install_provider() {
        for c in CATALOG {
            assert_eq!(c.installable, crate::runtime_installer::provider_for(c.id).is_some(), "{}", c.id);
        }
    }

    #[test]
    fn ids_round_trip() {
        for id in ComponentId::ALL {
            assert_eq!(id.as_str().parse::<ComponentId>().unwrap(), id);
        }
        assert!("tomcat".parse::<ComponentId>().is_err());
    }
}
