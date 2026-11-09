use serde::Serialize;

use crate::app_core::Core;
use crate::error::AppResult;
use crate::port_manager::{self, PortStatus};
use crate::runtime_manager;
use crate::service_manager::Health;
use crate::util::new_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
}

pub fn check(id: impl Into<String>, label: impl Into<String>, status: CheckStatus, detail: impl Into<String>) -> Check {
    Check { id: id.into(), label: label.into(), status, detail: detail.into() }
}

pub fn run(core: &Core) -> AppResult<Vec<Check>> {
    let mut checks = Vec::new();

    let probe = core.paths.root.join(format!(".write-test-{}", new_id()));
    checks.push(match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
        Ok(()) => check("data_dir", "Data directory is writable", CheckStatus::Ok, core.paths.root.display().to_string()),
        Err(e) => check("data_dir", "Data directory is writable", CheckStatus::Error, format!("{}: {e}", core.paths.root.display())),
    });

    checks.push(check(
        "database",
        "Metadata database",
        CheckStatus::Ok,
        format!("schema v{} at {}", core.db.schema_version(), core.paths.db_file.display()),
    ));

    for inst in runtime_manager::list(&core.db, None)? {
        let label = format!("{} {} executable", inst.component, inst.version);
        checks.push(if inst.executable_exists {
            check(format!("install:{}", inst.id), label, CheckStatus::Ok, inst.executable_path.display().to_string())
        } else {
            check(
                format!("install:{}", inst.id),
                label,
                CheckStatus::Error,
                format!("{} is missing; re-register the installation", inst.executable_path.display()),
            )
        });
    }

    for status in core.services.statuses()? {
        let s = &status.service;
        let id = format!("service_port:{}", s.id);
        let label = format!("Port {} for '{}'", s.http_port, s.name);
        if status.health == Health::Running {
            checks.push(check(id, label, CheckStatus::Ok, format!("in use by this service ({})", status.url)));
            if status.connection.is_some() {
                let probe_id = format!("service_probe:{}", s.id);
                let probe_label = format!("'{}' answers connections", s.name);
                checks.push(match core.services.probe(&s.id) {
                    Ok(detail) => check(probe_id, probe_label, CheckStatus::Ok, detail),
                    Err(e) => check(probe_id, probe_label, CheckStatus::Error, e.to_string()),
                });
            }
            continue;
        }
        let bind = port_manager::parse_bind_address(&s.listen_address)?;
        let inspection = port_manager::inspect(s.http_port, bind)?;
        checks.push(match inspection.status {
            PortStatus::Free => check(id, label, CheckStatus::Ok, "free"),
            PortStatus::InUse { owners } => {
                let who: Vec<String> = owners
                    .iter()
                    .map(|o| format!("{} (PID {})", o.process_name.as_deref().unwrap_or("unknown"), o.pid))
                    .collect();
                check(id, label, CheckStatus::Warning, format!("in use by {}; the service cannot start", who.join(", ")))
            }
            PortStatus::Unavailable { reason } => check(id, label, CheckStatus::Warning, reason),
        });
        if status.health == Health::Failed {
            checks.push(check(
                format!("service_health:{}", s.id),
                format!("'{}' state", s.name),
                CheckStatus::Error,
                "the service exited unexpectedly; see its logs",
            ));
        }
    }
    Ok(checks)
}
