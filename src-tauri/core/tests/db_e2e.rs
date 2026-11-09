//! Databases and caches end to end with real servers.
//!
//!   ZENV_TEST_MYSQL_ROOT=C:\...\mysql-9.7.0-winx64
//!   ZENV_TEST_POSTGRES_ROOT=C:\Program Files\PostgreSQL\9.2
//!   ZENV_TEST_REDIS=C:\...\redis-8.8.0\...;C:\...\redis-5.0.14   (';'-separated)
//!   npm run test:e2e

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use zenv_core::app_core::Core;
use zenv_core::error::AppError;
use zenv_core::paths::AppPaths;
use zenv_core::port_manager;
use zenv_core::runtime_catalog::ComponentId;
use zenv_core::runtime_manager::{self, Installation};
use zenv_core::service_manager::{Health, Service, ServiceInput};
use zenv_core::web_server::DataDirState;

fn service(core: &Core, name: &str, inst: &Installation, data_dir: &Path) -> Service {
    core.services
        .create(&ServiceInput {
            name: name.into(),
            installation_id: inst.id.clone(),
            listen_address: "127.0.0.1".into(),
            http_port: port_manager::suggest_free_port().unwrap(),
            autostart: false,
            data_dir: Some(data_dir.display().to_string()),
            https_port: None,
        })
        .unwrap()
}

fn stopped_gracefully(core: &Core, id: &str) -> bool {
    core.services.events(id, 5).unwrap().iter().any(|e| e.message == "stopped gracefully")
}

fn client(exe: PathBuf, args: &[&str]) -> String {
    let out = Command::new(&exe).args(args).output().unwrap_or_else(|e| panic!("{}: {e}", exe.display()));
    assert!(out.status.success(), "{}: {}", exe.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn resp(port: u16, parts: &[&str]) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut req = format!("*{}\r\n", parts.len());
    for p in parts {
        req.push_str(&format!("${}\r\n{p}\r\n", p.len()));
    }
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = [0u8; 256];
    let n = s.read(&mut buf).unwrap();
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

#[test]
#[ignore = "requires ZENV_TEST_MYSQL_ROOT, ZENV_TEST_POSTGRES_ROOT and ZENV_TEST_REDIS"]
fn databases_and_caches_end_to_end() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let data = tempfile::tempdir().unwrap();
    let dbs = tempfile::tempdir().unwrap();
    let core = Core::initialize(AppPaths::new(data.path())).unwrap();

    // ----- MySQL -------------------------------------------------------------
    let mysql = runtime_manager::register(&core.db, ComponentId::Mysql, Path::new(&env("ZENV_TEST_MYSQL_ROOT"))).unwrap();
    let my_dir = dbs.path().join("mysql");
    let my = service(&core, "MySQL", &mysql, &my_dir);
    let st = core.services.status(&my.id).unwrap();
    assert_eq!(st.data_dir_info.as_ref().unwrap().state, DataDirState::Missing);
    assert_eq!(st.connection.as_ref().unwrap().url, format!("mysql://root@127.0.0.1:{}", my.http_port));
    assert!(matches!(core.start_service(&my.id), Err(AppError::Conflict(m)) if m.contains("not initialized")));
    core.services.initialize_data_dir(&my.id).expect("mysqld --initialize-insecure");
    assert!(matches!(core.services.initialize_data_dir(&my.id), Err(AppError::Conflict(_))), "never re-initialize");
    core.start_service(&my.id).expect("start mysql");
    let hello = core.services.probe(&my.id).unwrap();
    assert!(hello.starts_with(&format!("MySQL {}", mysql.version)), "{hello}");
    let mysql_cli = mysql.root_path.join("bin").join("mysql.exe");
    let port = my.http_port.to_string();
    client(mysql_cli.clone(), &["-h127.0.0.1", "-P", &port, "-uroot", "-e", "CREATE DATABASE zenv_e2e"]);
    core.services.stop(&my.id).unwrap();
    assert!(stopped_gracefully(&core, &my.id), "MySQL should stop through its shutdown event");
    core.start_service(&my.id).unwrap();
    let dbs_list = client(mysql_cli, &["-h127.0.0.1", "-P", &port, "-uroot", "-N", "-e", "SHOW DATABASES LIKE 'zenv_e2e'"]);
    assert!(dbs_list.contains("zenv_e2e"), "data survived a restart: {dbs_list}");
    core.services.stop(&my.id).unwrap();

    // A folder holding something else is refused and left untouched.
    let foreign = dbs.path().join("not-a-db");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("notes.txt"), "keep me").unwrap();
    let bad = service(&core, "MySQL foreign", &mysql, &foreign);
    assert!(core.services.initialize_data_dir(&bad.id).is_err());
    assert!(matches!(core.start_service(&bad.id), Err(AppError::Validation(_))));
    assert_eq!(std::fs::read_to_string(foreign.join("notes.txt")).unwrap(), "keep me");

    // ----- PostgreSQL ------------------------------------------------------------
    let pg_root = PathBuf::from(env("ZENV_TEST_POSTGRES_ROOT"));
    let pg = runtime_manager::register(&core.db, ComponentId::Postgresql, &pg_root).unwrap();
    let pg_svc = service(&core, "PostgreSQL", &pg, &dbs.path().join("pg"));
    core.services.initialize_data_dir(&pg_svc.id).expect("initdb");
    core.start_service(&pg_svc.id).expect("start postgres");
    assert!(core.services.probe(&pg_svc.id).unwrap().contains("accepting connections"));
    let psql = pg.root_path.join("bin").join("psql.exe");
    let pport = pg_svc.http_port.to_string();
    client(psql.clone(), &["-h", "127.0.0.1", "-p", &pport, "-U", "postgres", "-c", "CREATE TABLE zenv_e2e (id int)"]);
    core.services.stop(&pg_svc.id).unwrap();
    assert!(stopped_gracefully(&core, &pg_svc.id), "pg_ctl stop -m fast");
    core.start_service(&pg_svc.id).unwrap();
    let tables = client(psql, &["-h", "127.0.0.1", "-p", &pport, "-U", "postgres", "-tAc", "SELECT count(*) FROM zenv_e2e"]);
    assert_eq!(tables.trim(), "0");
    core.services.stop(&pg_svc.id).unwrap();

    // ----- Redis (each registered build, native and MSYS2) ---------------------
    for (i, root) in env("ZENV_TEST_REDIS").split(';').enumerate() {
        let redis = runtime_manager::register(&core.db, ComponentId::Redis, Path::new(root.trim())).unwrap();
        let svc = service(&core, &format!("Redis {}", redis.version), &redis, &dbs.path().join(format!("redis-{i}")));
        core.start_service(&svc.id).unwrap_or_else(|e| panic!("start redis {}: {e}", redis.version));
        assert!(core.services.probe(&svc.id).unwrap().contains("PONG"));
        assert!(resp(svc.http_port, &["SET", "zenv", "persisted"]).starts_with("+OK"));
        core.services.stop(&svc.id).unwrap();
        assert!(stopped_gracefully(&core, &svc.id), "Redis SHUTDOWN");
        core.start_service(&svc.id).unwrap();
        assert!(resp(svc.http_port, &["GET", "zenv"]).contains("persisted"), "Redis {} saved on shutdown", redis.version);
        core.services.stop(&svc.id).unwrap();
        assert_eq!(core.services.status(&svc.id).unwrap().health, Health::Stopped);
    }

    // Deleting a database service never deletes its data.
    core.services.delete(&my.id).unwrap();
    assert!(my_dir.join("mysql").is_dir(), "MySQL data folder kept");
    core.shutdown();
}
