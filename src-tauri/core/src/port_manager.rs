use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};

use serde::Serialize;

use crate::error::{AppError, AppResult, PortOwner};
use crate::platform::{self, ListeningSocket};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PortStatus {
    Free,
    InUse { owners: Vec<PortOwner> },
    /// Bind fails although nobody listens: typically a Windows excluded port
    /// range (Hyper-V/WSL reservations) or a permission problem.
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInspection {
    pub port: u16,
    pub address: String,
    #[serde(flatten)]
    pub status: PortStatus,
}

/// True when a listener on `listener_addr` would conflict with binding `bind`.
fn overlaps(listener_addr: IpAddr, bind: IpAddr) -> bool {
    listener_addr == bind || listener_addr.is_unspecified() || bind.is_unspecified()
}

pub fn owners_of(sockets: &[ListeningSocket], port: u16, bind: IpAddr) -> Vec<PortOwner> {
    let mut owners: Vec<PortOwner> = sockets
        .iter()
        .filter(|s| s.port == port && s.address.is_ipv4() == bind.is_ipv4() && overlaps(s.address, bind))
        .map(|s| PortOwner {
            pid: s.pid,
            process_name: platform::process_image_name(s.pid),
            address: s.address.to_string(),
        })
        .collect();
    owners.sort_by_key(|o| o.pid);
    owners.dedup_by_key(|o| o.pid);
    owners
}

pub fn inspect(port: u16, bind: IpAddr) -> AppResult<PortInspection> {
    let sockets = platform::listening_sockets()?;
    let owners = owners_of(&sockets, port, bind);
    let status = if !owners.is_empty() {
        PortStatus::InUse { owners }
    } else {
        match TcpListener::bind(SocketAddr::new(bind, port)) {
            Ok(_) => PortStatus::Free,
            Err(e) => PortStatus::Unavailable { reason: e.to_string() },
        }
    };
    Ok(PortInspection { port, address: bind.to_string(), status })
}

pub fn ensure_available(port: u16, bind: IpAddr) -> AppResult<()> {
    match inspect(port, bind)?.status {
        PortStatus::Free => Ok(()),
        PortStatus::InUse { owners } => Err(AppError::PortUnavailable { port, owners, reason: None }),
        PortStatus::Unavailable { reason } => Err(AppError::PortUnavailable {
            port,
            owners: vec![],
            reason: Some(format!(
                "cannot bind: {reason}. It may be inside a Windows excluded port range \
                 (see `netsh interface ipv4 show excludedportrange protocol=tcp`)"
            )),
        }),
    }
}

/// Picks an OS-assigned free port on loopback (used by tests and suggestions).
pub fn suggest_free_port() -> AppResult<u16> {
    let l = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(|e| AppError::Platform(format!("cannot allocate a port: {e}")))?;
    Ok(l.local_addr().map_err(|e| AppError::Platform(e.to_string()))?.port())
}

pub fn parse_bind_address(s: &str) -> AppResult<IpAddr> {
    s.parse::<IpAddr>()
        .map_err(|_| AppError::Validation(format!("'{s}' is not a valid IP address")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sock(addr: &str, port: u16, pid: u32) -> ListeningSocket {
        ListeningSocket { address: addr.parse().unwrap(), port, pid }
    }

    #[test]
    fn overlap_rules() {
        let table = vec![
            sock("0.0.0.0", 80, 10),
            sock("127.0.0.1", 8080, 11),
            sock("::", 443, 12),
            sock("192.168.1.5", 3000, 13),
        ];
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        let any: IpAddr = "0.0.0.0".parse().unwrap();
        assert_eq!(owners_of(&table, 80, lo).len(), 1, "wildcard listener blocks loopback");
        assert_eq!(owners_of(&table, 8080, any).len(), 1, "wildcard bind collides with loopback listener");
        assert!(owners_of(&table, 3000, lo).is_empty(), "different specific addresses do not collide");
        assert!(owners_of(&table, 443, lo).is_empty(), "IPv6 wildcard does not block an IPv4 bind");
        assert!(owners_of(&table, 81, any).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn detects_a_real_conflict_with_owner() {
        let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let listener = TcpListener::bind((lo, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        match ensure_available(port, lo).unwrap_err() {
            AppError::PortUnavailable { owners, .. } => {
                assert_eq!(owners[0].pid, std::process::id());
                assert!(owners[0].process_name.is_some());
            }
            other => panic!("unexpected {other:?}"),
        }
        drop(listener);
        assert!(ensure_available(port, lo).is_ok());
    }
}
