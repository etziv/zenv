use std::process::Child;

use super::ListeningSocket;
use crate::error::{AppError, AppResult};

fn unsupported<T>() -> AppResult<T> {
    Err(AppError::Unsupported("this operation is only implemented on Windows".into()))
}

pub struct Job;

impl Job {
    pub fn new() -> AppResult<Self> {
        unsupported()
    }
    pub fn assign(&self, _child: &Child) -> AppResult<()> {
        unsupported()
    }
    pub fn terminate(&self, _exit_code: u32) -> AppResult<()> {
        unsupported()
    }
    pub fn process_ids(&self) -> AppResult<Vec<u32>> {
        unsupported()
    }
}

pub fn listening_sockets() -> AppResult<Vec<ListeningSocket>> {
    unsupported()
}

pub fn process_image_name(_pid: u32) -> Option<String> {
    None
}

pub fn signal_named_event(_name: &str) -> AppResult<()> {
    unsupported()
}

pub fn hide_console_window(_cmd: &mut std::process::Command) {}

pub fn run_elevated(_exe: &std::path::Path, _params: &str) -> AppResult<u32> {
    unsupported()
}

pub fn open_console(_cwd: &std::path::Path, _env: &[(std::ffi::OsString, std::ffi::OsString)], _banner: &str) -> AppResult<()> {
    unsupported()
}

pub fn style_popup(_hwnd: isize, _border_rgb: u32) {}

pub fn authenticode_signer(_path: &std::path::Path) -> AppResult<Option<String>> {
    Ok(None)
}

pub fn read_env_var(_machine: bool, name: &str) -> AppResult<Option<String>> {
    Ok(std::env::var(name).ok())
}

pub fn read_path_var(_machine: bool) -> AppResult<String> {
    Ok(std::env::var("PATH").unwrap_or_default())
}

pub fn write_user_path(_value: &str) -> AppResult<()> {
    unsupported()
}

pub fn broadcast_environment_change() {}

pub fn show_fatal_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RootTrust {
    pub current_user: bool,
    pub local_machine: bool,
}

pub fn root_trust(_der: &[u8]) -> AppResult<RootTrust> {
    Ok(RootTrust::default())
}

pub fn trust_root(_der: &[u8]) -> AppResult<()> {
    unsupported()
}

pub fn untrust_root(_der: &[u8]) -> AppResult<bool> {
    unsupported()
}

pub fn restrict_to_current_user(_dir: &std::path::Path) -> AppResult<()> {
    Ok(())
}

pub fn folder_access(_path: &std::path::Path) -> AppResult<super::FolderAccess> {
    Ok(super::FolderAccess::Private)
}

pub fn make_private(_dir: &std::path::Path) -> AppResult<super::FolderAccess> {
    Ok(super::FolderAccess::Private)
}

pub fn exe_icon_rgba(_path: &std::path::Path, _size: u32) -> Option<Vec<u8>> {
    None
}
