//! Project-type detection from marker files. Results are only defaults: every
//! field stays editable before the site is created.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{ProjectType, Strategy};
use crate::domain_manager;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProject {
    pub project_root: PathBuf,
    pub name: String,
    pub domain: String,
    pub project_type: ProjectType,
    pub document_root: PathBuf,
    pub strategy: Strategy,
    pub upstream_port: Option<u16>,
    /// Which files led to the guess, e.g. "artisan and composer.json".
    pub evidence: String,
    /// What the user should know about this default.
    pub notes: Vec<String>,
}

fn has(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

/// First of `candidates` that exists as a directory, else the root itself.
fn first_dir(root: &Path, candidates: &[&str]) -> PathBuf {
    candidates
        .iter()
        .map(|c| root.join(c))
        .find(|p| p.is_dir())
        .unwrap_or_else(|| root.to_path_buf())
}

pub fn detect(project_root: &Path) -> AppResult<DetectedProject> {
    if !project_root.is_dir() {
        return Err(AppError::invalid_path(project_root, "is not an existing folder"));
    }
    let root = dunce::canonicalize(project_root).map_err(|e| AppError::invalid_path(project_root, e.to_string()))?;
    let folder = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "site".into());

    let proxy_note = |cmd: &str, _port: u16| {
        format!("Zenv starts your app ({cmd}) with the project's .env and gives it the port below as PORT / APP_PORT; the .env file is not changed.")
    };

    let (project_type, document_root, strategy, upstream_port, evidence, notes) = if has(&root, "artisan")
        && has(&root, "composer.json")
    {
        (ProjectType::Laravel, first_dir(&root, &["public"]), Strategy::Php, None, "artisan and composer.json", vec![])
    } else if has(&root, "composer.json") || has(&root, "index.php") || has(&root, "public/index.php") {
        (ProjectType::Php, first_dir(&root, &["public", "web", "htdocs"]), Strategy::Php, None, "composer.json or index.php", vec![])
    } else if has(&root, "go.mod") {
        (ProjectType::Go, root.clone(), Strategy::ReverseProxy, Some(3000), "go.mod", vec![proxy_note(&format!("`{}`", crate::app_process::detect(&root).unwrap_or_else(|| "go run .".into())), 3000)])
    } else if has(&root, "Cargo.toml") {
        (ProjectType::Rust, root.clone(), Strategy::ReverseProxy, Some(3000), "Cargo.toml", vec![proxy_note("`cargo run`", 3000)])
    } else if has(&root, "manage.py") || has(&root, "pyproject.toml") || has(&root, "requirements.txt") {
        (ProjectType::Python, root.clone(), Strategy::ReverseProxy, Some(8000), "manage.py, pyproject.toml or requirements.txt", vec![proxy_note("your app server (for example `uvicorn` or `manage.py runserver`)", 8000)])
    } else if has(&root, "Gemfile") {
        (ProjectType::Ruby, root.clone(), Strategy::ReverseProxy, Some(3000), "Gemfile", vec![proxy_note("`bin/rails server` or `bundle exec rackup -p 3000`", 3000)])
    } else if has(&root, "index.html") || has(&root, "public/index.html") || has(&root, "dist/index.html") {
        let doc = ["dist", "public"]
            .iter()
            .map(|d| root.join(d))
            .find(|d| d.join("index.html").is_file())
            .unwrap_or_else(|| root.clone());
        (ProjectType::Static, doc, Strategy::Static, None, "index.html", vec![])
    } else {
        (
            ProjectType::Custom,
            root.clone(),
            Strategy::Static,
            None,
            "no known project files",
            vec!["Choose how this folder is served: as static files, or proxied to an app you run yourself.".to_string()],
        )
    };

    Ok(DetectedProject {
        name: folder.clone(),
        domain: domain_manager::suggest_domain(&folder),
        project_root: root,
        project_type,
        document_root,
        strategy,
        upstream_port,
        evidence: evidence.to_string(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::Builder::new().prefix("My Shop_").tempdir().unwrap();
        for f in files {
            let p = dir.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        dir
    }

    #[test]
    fn laravel_defaults_to_public_and_php() {
        let d = project(&["artisan", "composer.json", "public/index.php"]);
        let r = detect(d.path()).unwrap();
        assert_eq!(r.project_type, ProjectType::Laravel);
        assert!(r.document_root.ends_with("public"));
        assert_eq!((r.strategy, r.upstream_port), (Strategy::Php, None));
        assert!(r.domain.starts_with("my-shop-") && r.domain.ends_with(".test"), "{}", r.domain);
    }

    #[test]
    fn php_without_public_uses_the_root() {
        let d = project(&["index.php"]);
        let r = detect(d.path()).unwrap();
        assert_eq!(r.project_type, ProjectType::Php);
        assert_eq!(r.document_root, dunce::canonicalize(d.path()).unwrap());
    }

    #[test]
    fn runtimes_map_to_reverse_proxy_defaults() {
        for (file, ty, port) in [
            ("go.mod", ProjectType::Go, 3000),
            ("Cargo.toml", ProjectType::Rust, 3000),
            ("requirements.txt", ProjectType::Python, 8000),
            ("Gemfile", ProjectType::Ruby, 3000),
        ] {
            let r = detect(project(&[file]).path()).unwrap();
            assert_eq!((r.project_type, r.strategy, r.upstream_port), (ty, Strategy::ReverseProxy, Some(port)), "{file}");
        }
    }

    #[test]
    fn static_sites_prefer_built_output() {
        let r = detect(project(&["dist/index.html", "src/main.js"]).path()).unwrap();
        assert_eq!((r.project_type, r.strategy), (ProjectType::Static, Strategy::Static));
        assert!(r.document_root.ends_with("dist"));
    }

    #[test]
    fn unknown_folders_become_custom_and_missing_ones_fail() {
        let r = detect(project(&["notes.txt"]).path()).unwrap();
        assert_eq!(r.project_type, ProjectType::Custom);
        assert!(detect(Path::new(r"Z:\definitely\missing")).is_err());
    }
}
