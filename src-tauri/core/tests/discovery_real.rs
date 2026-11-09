//! Runs discovery and version probes against whatever is installed on this
//! machine. Ignored by default because results depend on the host.
//!   cargo test -p zenv-core --test discovery_real -- --ignored --nocapture

use zenv_core::runtime_manager;
use zenv_core::storage::Db;

#[test]
#[ignore = "depends on software installed on the host"]
fn discovers_and_probes_local_installations() {
    let db = Db::open_in_memory().unwrap();
    let found = runtime_manager::discover(&db, None, &[]).unwrap();
    assert!(!found.is_empty(), "nothing discovered on this machine");
    for f in &found {
        match (&f.version, &f.error) {
            (Some(v), _) => println!("{:<11} {:<8} {}", f.component.as_str(), v, f.executable_path.display()),
            (None, Some(e)) => println!("{:<11} FAILED   {} ({e})", f.component.as_str(), f.executable_path.display()),
            (None, None) => unreachable!("probe returns a version or an error"),
        }
    }
    // Every discovered candidate that probed successfully can be registered.
    for f in found.iter().filter(|f| f.version.is_some()) {
        let inst = runtime_manager::register(&db, f.component, &f.executable_path)
            .unwrap_or_else(|e| panic!("register {}: {e}", f.executable_path.display()));
        assert_eq!(Some(&inst.version), f.version.as_ref());
    }
}
