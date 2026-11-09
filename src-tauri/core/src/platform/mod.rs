//! OS-specific primitives. Windows is the supported target; other platforms
//! compile with explicit `Unsupported` fallbacks so platform-independent logic
//! stays testable everywhere.

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod fallback;
#[cfg(not(windows))]
pub use fallback::*;

use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListeningSocket {
    pub address: IpAddr,
    pub port: u16,
    pub pid: u32,
}

pub const SYSTEM: &str = "S-1-5-18";
pub const ADMINISTRATORS: &str = "S-1-5-32-544";
pub const EVERYONE: &str = "S-1-1-0";
/// Accounts that may always reach Zenv's folders besides the user: they
/// already control the whole computer.
const TRUSTED: &[&str] = &[
    SYSTEM,
    ADMINISTRATORS,
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464", // TrustedInstaller
    "S-1-3-0",                                                        // CREATOR OWNER (applies to whoever creates a child)
    "S-1-3-4",                                                        // OWNER RIGHTS
];

/// Owner and the SIDs granted access to a file or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Security {
    pub owner: String,
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderAccess {
    /// Only this user, SYSTEM and Administrators.
    Private,
    /// These other accounts have access too.
    Shared(Vec<String>),
    /// Another account owns it, and an owner can always change permissions.
    ForeignOwner(String),
}

pub fn classify(sec: &Security, user_sid: &str) -> FolderAccess {
    let trusted = |sid: &str| sid == user_sid || TRUSTED.contains(&sid);
    if !trusted(&sec.owner) {
        return FolderAccess::ForeignOwner(sec.owner.clone());
    }
    let mut others: Vec<String> = sec.allowed.iter().filter(|s| !trusted(s)).cloned().collect();
    others.dedup();
    if others.is_empty() {
        FolderAccess::Private
    } else {
        FolderAccess::Shared(others)
    }
}

/// Zenv starts `zenv.exe` with administrator rights (hosts file) and runs the
/// programs under its home, so nobody but this user may change them. Locks
/// the folder when other accounts can reach it.
pub fn secure_folder(dir: &std::path::Path) -> crate::error::AppResult<FolderAccess> {
    match folder_access(dir)? {
        FolderAccess::Shared(others) => {
            tracing::info!(folder = %dir.display(), ?others, "limiting the folder to this user, SYSTEM and Administrators");
            make_private(dir)
        }
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "S-1-5-21-1-2-3-1001";

    fn sec(owner: &str, allowed: &[&str]) -> Security {
        Security { owner: owner.into(), allowed: allowed.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn only_user_system_and_admins_is_private() {
        assert_eq!(classify(&sec(ME, &[ME, SYSTEM, ADMINISTRATORS]), ME), FolderAccess::Private);
        assert_eq!(classify(&sec(ADMINISTRATORS, &[ME, SYSTEM]), ME), FolderAccess::Private);
    }

    #[test]
    fn inherited_drive_root_access_is_shared() {
        // C:\ grants Authenticated Users modify and Users read.
        let c = sec(ME, &[ADMINISTRATORS, SYSTEM, "S-1-5-32-545", "S-1-5-11", "S-1-5-11"]);
        assert_eq!(classify(&c, ME), FolderAccess::Shared(vec!["S-1-5-32-545".into(), "S-1-5-11".into()]));
    }

    #[test]
    fn another_owner_is_never_private() {
        let other = "S-1-5-21-1-2-3-1002";
        assert_eq!(classify(&sec(other, &[ME]), ME), FolderAccess::ForeignOwner(other.into()));
    }
}
