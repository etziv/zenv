//! Code editors and IDEs installed on this computer, to open an app's project
//! folder in. Only editors from this list run, each from where its own
//! installer puts it, with the app's folder as the one argument. The app's
//! PHP version and each language's default come first on the editor's PATH
//! (its terminal then matches Zenv), when the editor is not already running.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::app_core::Core;
use crate::error::{AppError, AppResult};

const PREFERRED_KEY: &str = "editor.preferred";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorId {
    Vscode,
    VscodeInsiders,
    Cursor,
    Windsurf,
    Antigravity,
    Kiro,
    Trae,
    Zed,
    Phpstorm,
    Webstorm,
    Idea,
    Goland,
    Rustrover,
    Pycharm,
    Sublime,
    NotepadPlusPlus,
}

/// Where an editor's own installer puts it.
enum Place {
    /// `%LOCALAPPDATA%\Programs\<dir>\<exe>` (per-user installers).
    UserPrograms(&'static str, &'static str),
    /// `%ProgramFiles%\<dir>\<exe>`.
    ProgramFiles(&'static str, &'static str),
    /// JetBrains: `<product> <version>\bin\<exe>` under `%ProgramFiles%\JetBrains`,
    /// or `<product>\bin\<exe>` under `%LOCALAPPDATA%\Programs` (Toolbox); newest wins.
    JetBrains(&'static str, &'static str),
}

struct Spec {
    id: EditorId,
    name: &'static str,
    places: &'static [Place],
}

use Place::*;

const EDITORS: &[Spec] = &[
    Spec {
        id: EditorId::Vscode,
        name: "VS Code",
        places: &[UserPrograms("Microsoft VS Code", "Code.exe"), ProgramFiles("Microsoft VS Code", "Code.exe")],
    },
    Spec {
        id: EditorId::VscodeInsiders,
        name: "VS Code Insiders",
        places: &[
            UserPrograms("Microsoft VS Code Insiders", "Code - Insiders.exe"),
            ProgramFiles("Microsoft VS Code Insiders", "Code - Insiders.exe"),
        ],
    },
    Spec { id: EditorId::Cursor, name: "Cursor", places: &[UserPrograms("cursor", "Cursor.exe"), ProgramFiles("Cursor", "Cursor.exe")] },
    Spec { id: EditorId::Windsurf, name: "Windsurf", places: &[UserPrograms("Windsurf", "Windsurf.exe"), ProgramFiles("Windsurf", "Windsurf.exe")] },
    Spec { id: EditorId::Antigravity, name: "Antigravity", places: &[UserPrograms("Antigravity", "Antigravity.exe")] },
    Spec { id: EditorId::Kiro, name: "Kiro", places: &[UserPrograms("Kiro", "Kiro.exe")] },
    Spec { id: EditorId::Trae, name: "Trae", places: &[UserPrograms("Trae", "Trae.exe")] },
    Spec { id: EditorId::Zed, name: "Zed", places: &[UserPrograms("Zed", "Zed.exe"), ProgramFiles("Zed", "Zed.exe")] },
    Spec { id: EditorId::Phpstorm, name: "PhpStorm", places: &[JetBrains("PhpStorm", "phpstorm64.exe")] },
    Spec { id: EditorId::Webstorm, name: "WebStorm", places: &[JetBrains("WebStorm", "webstorm64.exe")] },
    Spec { id: EditorId::Idea, name: "IntelliJ IDEA", places: &[JetBrains("IntelliJ IDEA", "idea64.exe")] },
    Spec { id: EditorId::Goland, name: "GoLand", places: &[JetBrains("GoLand", "goland64.exe")] },
    Spec { id: EditorId::Rustrover, name: "RustRover", places: &[JetBrains("RustRover", "rustrover64.exe")] },
    Spec { id: EditorId::Pycharm, name: "PyCharm", places: &[JetBrains("PyCharm", "pycharm64.exe")] },
    Spec { id: EditorId::Sublime, name: "Sublime Text", places: &[ProgramFiles("Sublime Text", "sublime_text.exe")] },
    Spec { id: EditorId::NotepadPlusPlus, name: "Notepad++", places: &[ProgramFiles("Notepad++", "notepad++.exe")] },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorInfo {
    pub id: EditorId,
    pub name: &'static str,
    pub program: PathBuf,
    /// The editor's own icon, as a data URL.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Editors {
    pub editors: Vec<EditorInfo>,
    /// The one "Open in editor" uses: the last one picked, else the first found.
    pub preferred: Option<EditorId>,
}

/// The folders installers use, as found on this computer.
struct Roots {
    user_programs: Option<PathBuf>,
    program_files: Vec<PathBuf>,
}

impl Roots {
    fn system() -> Self {
        let user_programs = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Programs"));
        let program_files = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .fold(Vec::new(), |mut v, p| {
                if !v.contains(&p) {
                    v.push(p);
                }
                v
            });
        Roots { user_programs, program_files }
    }
}

/// `<root>\<product>[ <version>]\bin\<exe>`, the newest version first.
fn jetbrains_in(root: &Path, product: &str, exe: &str) -> Option<PathBuf> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(product)?;
            (rest.is_empty() || rest.starts_with(' ')).then(|| (rest.trim().to_string(), e.path().join("bin").join(exe)))
        })
        .filter(|(_, p)| p.is_file())
        .collect();
    found.sort_by_key(|(v, _)| std::cmp::Reverse(version_key(v)));
    found.into_iter().next().map(|(_, p)| p)
}

/// "2026.2.1" as numbers, so 2026.10 sorts after 2026.9.
fn version_key(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect()
}

fn locate(spec: &Spec, roots: &Roots) -> Option<PathBuf> {
    spec.places.iter().find_map(|place| match *place {
        UserPrograms(dir, exe) => roots.user_programs.as_ref().map(|r| r.join(dir).join(exe)).filter(|p| p.is_file()),
        ProgramFiles(dir, exe) => roots.program_files.iter().map(|r| r.join(dir).join(exe)).find(|p| p.is_file()),
        JetBrains(product, exe) => roots
            .program_files
            .iter()
            .map(|r| r.join("JetBrains"))
            .chain(roots.user_programs.clone())
            .find_map(|r| jetbrains_in(&r, product, exe)),
    })
}

fn installed(roots: &Roots) -> Vec<(&'static Spec, PathBuf)> {
    EDITORS.iter().filter_map(|s| locate(s, roots).map(|p| (s, p))).collect()
}

impl Core {
    fn preferred_editor(&self) -> Option<EditorId> {
        let v = self.setting(PREFERRED_KEY).ok()??;
        serde_json::from_value(serde_json::Value::String(v)).ok()
    }

    /// The editors installed here, with their icons.
    pub fn editors(&self) -> AppResult<Editors> {
        let found = installed(&Roots::system());
        let saved = self.preferred_editor().filter(|id| found.iter().any(|(s, _)| s.id == *id));
        let editors: Vec<EditorInfo> = found
            .into_iter()
            .map(|(s, program)| EditorInfo {
                id: s.id,
                name: s.name,
                icon: crate::icons::program_icon(&self.paths, &format!("editor-{}", s.name.replace(' ', "-")), &program),
                program,
            })
            .collect();
        let preferred = saved.or_else(|| editors.first().map(|e| e.id));
        Ok(Editors { editors, preferred })
    }

    /// Opens the app's project folder in `editor` (None: the preferred one),
    /// and remembers a chosen editor for next time.
    pub fn open_in_editor(&self, site_id: &str, editor: Option<EditorId>) -> AppResult<EditorId> {
        let site = self.sites.get(site_id)?;
        if !site.project_root.is_dir() {
            return Err(AppError::invalid_path(&site.project_root, "is not an existing folder"));
        }
        let found = installed(&Roots::system());
        let wanted = editor.or_else(|| self.preferred_editor());
        let (spec, program) = match wanted {
            Some(id) => found.into_iter().find(|(s, _)| s.id == id),
            None => found.into_iter().next(),
        }
        .ok_or_else(|| AppError::Validation("No supported editor is installed (VS Code, Cursor, PhpStorm, ...).".into()))?;
        let mut cmd = Command::new(&program);
        cmd.arg(&site.project_root).current_dir(&site.project_root);
        for (k, v) in self.studio_env(site_id)? {
            cmd.env(k, v);
        }
        cmd.spawn().map_err(|e| AppError::Platform(format!("cannot start {}: {e}", spec.name)))?;
        if editor.is_some() {
            let value = serde_json::to_value(spec.id).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
            self.set_setting(PREFERRED_KEY, &value)?;
        }
        tracing::info!(domain = %site.domain, editor = spec.name, "opened in editor");
        Ok(spec.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn finds_per_user_and_machine_installs() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("Programs");
        let pf = dir.path().join("PF");
        touch(&user.join(r"Microsoft VS Code\Code.exe"));
        touch(&pf.join(r"Sublime Text\sublime_text.exe"));
        let roots = Roots { user_programs: Some(user), program_files: vec![pf] };
        let ids: Vec<_> = installed(&roots).into_iter().map(|(s, _)| s.id).collect();
        assert_eq!(ids, vec![EditorId::Vscode, EditorId::Sublime]);
    }

    #[test]
    fn newest_jetbrains_version_wins() {
        let dir = tempfile::tempdir().unwrap();
        let jb = dir.path().join("PF").join("JetBrains");
        touch(&jb.join(r"PhpStorm 2025.3\bin\phpstorm64.exe"));
        touch(&jb.join(r"PhpStorm 2026.2.1\bin\phpstorm64.exe"));
        touch(&jb.join(r"PhpStormish\bin\phpstorm64.exe"));
        let roots = Roots { user_programs: None, program_files: vec![dir.path().join("PF")] };
        let (spec, path) = installed(&roots).into_iter().next().unwrap();
        assert_eq!(spec.id, EditorId::Phpstorm);
        assert!(path.to_string_lossy().contains("2026.2.1"));
    }

    #[test]
    fn ids_round_trip_as_settings() {
        let v = serde_json::to_value(EditorId::VscodeInsiders).unwrap();
        assert_eq!(v, "vscodeInsiders");
        assert_eq!(serde_json::from_value::<EditorId>(v).unwrap(), EditorId::VscodeInsiders);
    }
}
