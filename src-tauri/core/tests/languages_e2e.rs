//! Installs the newest stable PHP, Python, Ruby and Rust from their official
//! sources into a throwaway Zenv home, then uses each one the way a Zenv
//! terminal or project process would (through `runtime_env`).
//!
//!   ZENV_TEST_NETWORK=1   (downloads roughly 400 MB)
//!   npm run test:e2e

use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use zenv_core::app_core::Core;
use zenv_core::paths::AppPaths;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_env;
use zenv_core::runtime_manager::{InstallSource, Installation};

/// Runs `<program> <args>` resolved through the install's own environment.
fn run(install: &Installation, program: &str, args: &[&str], cwd: Option<&std::path::Path>) -> String {
    let env = runtime_env::build(std::slice::from_ref(install), std::env::var_os("PATH"));
    let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
    let exe = std::env::split_paths(&path)
        .flat_map(|d| [d.join(format!("{program}.exe")), d.join(format!("{program}.bat")), d.join(format!("{program}.cmd"))])
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("{program} on PATH"));
    let mut cmd = Command::new(&exe);
    cmd.args(args).envs(env.iter().map(|(k, v)| (k, v)));
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{} {args:?} failed:\n{text}", exe.display());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn newest_stable(core: &Core, c: ComponentId) -> String {
    let offers = core.installer.available(c).unwrap_or_else(|e| panic!("{c}: {e}"));
    assert!(offers.iter().all(|o| o.sha256.len() == 64), "{c}: every offer has a SHA-256");
    offers.iter().find(|o| o.stable).unwrap_or_else(|| panic!("{c}: no stable offer")).version.clone()
}

fn install(core: &Core, c: ComponentId) -> Installation {
    let version = newest_stable(core, c);
    println!("installing {c} {version}");
    let started = std::time::Instant::now();
    let inst = core.installer.install_blocking(c, &version).unwrap_or_else(|e| panic!("{c} {version}: {e}"));
    println!("  done in {:.0?} -> {}", started.elapsed(), inst.executable_path.display());
    assert_eq!(inst.source, InstallSource::Managed);
    assert_eq!(inst.version, version);
    assert!(inst.root_path.starts_with(core.paths.bin_dir.join(c.as_str())), "{}", inst.root_path.display());
    inst
}

fn modified(p: PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn every_language_installs_from_its_official_source() {
    assert_eq!(std::env::var("ZENV_TEST_NETWORK").as_deref(), Ok("1"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    let work = tempfile::tempdir().unwrap();

    // ----- PHP: NTS build with a ready php.ini -------------------------------
    let php = install(&core, ComponentId::Php);
    assert_eq!(run(&php, "php", &["-r", "echo PHP_VERSION;"], None), php.version);
    let modules = run(&php, "php", &["-m"], None).to_ascii_lowercase();
    for m in ["mbstring", "openssl", "pdo_mysql", "curl", "fileinfo"] {
        assert!(modules.lines().any(|l| l.trim() == m), "PHP extension {m} enabled:\n{modules}");
    }
    assert!(php.root_path.join("php-cgi.exe").is_file(), "php-cgi for the FastCGI pools");

    // ----- Python: pip and venv work -----------------------------------------
    let python = install(&core, ComponentId::Python);
    assert!(run(&python, "python", &["--version"], None).contains(&python.version));
    assert!(run(&python, "python", &["-m", "pip", "--version"], None).starts_with("pip "));
    run(&python, "python", &["-m", "venv", &work.path().join("venv").display().to_string()], None);
    assert!(work.path().join("venv").join("Scripts").join("python.exe").is_file());

    // ----- Ruby: RubyInstaller 7z, gem works ---------------------------------
    let ruby = install(&core, ComponentId::Ruby);
    assert_eq!(run(&ruby, "ruby", &["-e", "print RUBY_VERSION"], None), ruby.version);
    assert!(!run(&ruby, "gem", &["--version"], None).is_empty());

    // ----- Rust: own RUSTUP_HOME/CARGO_HOME, builds a program ----------------
    let user_rustup = std::env::var_os("USERPROFILE").map(PathBuf::from).map(|p| p.join(".rustup").join("settings.toml"));
    let before = user_rustup.clone().and_then(modified);
    let rust = install(&core, ComponentId::Rust);
    assert!(rust.root_path.join("cargo").join("bin").join("cargo.exe").is_file());
    assert!(run(&rust, "rustc", &["--version"], None).contains(&rust.version));
    run(&rust, "cargo", &["new", "--vcs", "none", "hello"], Some(work.path()));
    run(&rust, "cargo", &["build", "--offline"], Some(&work.path().join("hello")));
    let hello = work.path().join("hello").join("target").join("debug").join("hello.exe");
    assert_eq!(String::from_utf8_lossy(&Command::new(&hello).output().unwrap().stdout).trim(), "Hello, world!");
    assert_eq!(user_rustup.and_then(modified), before, "the developer's ~/.rustup is untouched");

    // Installed versions are listed as installed by the providers.
    for c in [ComponentId::Php, ComponentId::Python, ComponentId::Ruby, ComponentId::Rust] {
        let offers = core.installer.available(c).unwrap();
        assert!(offers.iter().any(|o| o.installed_id.is_some()), "{c}");
    }

    // Uninstall removes only the managed folders.
    for inst in [&php, &python, &ruby, &rust] {
        core.installer.uninstall(&inst.id).unwrap();
        assert!(!inst.root_path.exists(), "{}", inst.root_path.display());
    }
}

#[test]
#[ignore = "requires ZENV_TEST_NETWORK=1"]
fn node_installs_from_nodejs_org() {
    assert_eq!(std::env::var("ZENV_TEST_NETWORK").as_deref(), Ok("1"));
    let home = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(home.path())).unwrap();
    let work = tempfile::tempdir().unwrap();

    let node = install(&core, ComponentId::Node);
    assert_eq!(run(&node, "node", &["-p", "process.versions.node"], None), node.version);
    assert!(!run(&node, "npm", &["--version"], None).is_empty());
    let app = work.path().join("hello");
    std::fs::create_dir(&app).unwrap();
    run(&node, "npm", &["init", "-y"], Some(&app));
    assert!(app.join("package.json").is_file());
    assert!(core.installer.available(ComponentId::Node).unwrap().iter().any(|o| o.installed_id.is_some()));
    core.installer.uninstall(&node.id).unwrap();
    assert!(!node.root_path.exists());
}
