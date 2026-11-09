use serde::ser::SerializeStruct;
use serde::Serialize;
use serde_json::{json, Value};

/// A process that holds a TCP listener, reported in port diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortOwner {
    pub pid: u32,
    pub process_name: Option<String>,
    pub address: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("{0}")]
    Validation(String),

    #[error("invalid path '{path}': {reason}")]
    InvalidPath { path: String, reason: String },

    #[error("{0}")]
    Conflict(String),

    #[error("port {port} is not available{}", describe_owners(.owners, .reason))]
    PortUnavailable {
        port: u16,
        owners: Vec<PortOwner>,
        reason: Option<String>,
    },

    #[error("configuration check failed: {message}")]
    ConfigInvalid { message: String, output: String },

    #[error("{message}")]
    StartFailed { message: String, log_tail: Vec<String> },

    #[error("version detection failed: {0}")]
    Detection(String),

    #[error("{0}")]
    Unsupported(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("template error: {0}")]
    Template(String),

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("system call failed: {0}")]
    Platform(String),

    #[error("{0}")]
    ElevationDeclined(String),
}

pub type AppResult<T> = Result<T, AppError>;

fn describe_owners(owners: &[PortOwner], reason: &Option<String>) -> String {
    if let Some(owner) = owners.first() {
        let name = owner.process_name.as_deref().unwrap_or("unknown process");
        return format!(" (in use by {name}, PID {})", owner.pid);
    }
    match reason {
        Some(r) => format!(" ({r})"),
        None => String::new(),
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound { .. } => "not_found",
            AppError::Validation(_) => "validation",
            AppError::InvalidPath { .. } => "invalid_path",
            AppError::Conflict(_) => "conflict",
            AppError::PortUnavailable { .. } => "port_unavailable",
            AppError::ConfigInvalid { .. } => "config_invalid",
            AppError::StartFailed { .. } => "start_failed",
            AppError::Detection(_) => "detection_failed",
            AppError::Unsupported(_) => "unsupported",
            AppError::Database(_) => "database",
            AppError::Migration(_) => "migration",
            AppError::Template(_) => "template",
            AppError::Io { .. } => "io",
            AppError::Platform(_) => "platform",
            AppError::ElevationDeclined(_) => "elevation_declined",
        }
    }

    fn details(&self) -> Value {
        match self {
            AppError::NotFound { entity, id } => json!({ "entity": entity, "id": id }),
            AppError::InvalidPath { path, reason } => json!({ "path": path, "reason": reason }),
            AppError::PortUnavailable { port, owners, reason } => {
                json!({ "port": port, "owners": owners, "reason": reason })
            }
            AppError::ConfigInvalid { output, .. } => json!({ "output": output }),
            AppError::StartFailed { log_tail, .. } => json!({ "logTail": log_tail }),
            _ => Value::Null,
        }
    }

    pub fn invalid_path(path: impl AsRef<std::path::Path>, reason: impl Into<String>) -> Self {
        AppError::InvalidPath {
            path: path.as_ref().display().to_string(),
            reason: reason.into(),
        }
    }
}

/// Errors cross the IPC boundary as `{ code, message, details }`.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("details", &self.details())?;
        s.end()
    }
}

pub trait IoContext<T> {
    fn ctx(self, context: impl FnOnce() -> String) -> AppResult<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn ctx(self, context: impl FnOnce() -> String) -> AppResult<T> {
        self.map_err(|source| AppError::Io {
            context: context(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_message_and_details() {
        let err = AppError::PortUnavailable {
            port: 80,
            owners: vec![PortOwner {
                pid: 42,
                process_name: Some("httpd.exe".into()),
                address: "0.0.0.0".into(),
            }],
            reason: None,
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "port_unavailable");
        assert_eq!(v["message"], "port 80 is not available (in use by httpd.exe, PID 42)");
        assert_eq!(v["details"]["owners"][0]["pid"], 42);
    }
}
