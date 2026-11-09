use std::ffi::c_void;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    OpenEventW, OpenProcess, QueryFullProcessImageNameW, SetEvent, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    EVENT_MODIFY_STATE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::ListeningSocket;
use crate::error::{AppError, AppResult};

fn last_error(op: &str) -> AppError {
    let code = unsafe { GetLastError() };
    AppError::Platform(format!("{op} failed: {}", std::io::Error::from_raw_os_error(code as i32)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A Windows Job Object configured with KILL_ON_JOB_CLOSE: every process in it
/// (including grandchildren such as Apache's worker child) dies when the job is
/// terminated or when Zenv exits, even abnormally.
pub struct Job(HANDLE);

// The handle is only used through thread-safe kernel calls.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn new() -> AppResult<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(last_error("CreateJobObjectW"));
            }
            let job = Job(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                return Err(last_error("SetInformationJobObject"));
            }
            Ok(job)
        }
    }

    pub fn assign(&self, child: &Child) -> AppResult<()> {
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
        if ok == 0 {
            return Err(last_error("AssignProcessToJobObject"));
        }
        Ok(())
    }

    pub fn terminate(&self, exit_code: u32) -> AppResult<()> {
        if unsafe { TerminateJobObject(self.0, exit_code) } == 0 {
            return Err(last_error("TerminateJobObject"));
        }
        Ok(())
    }

    /// PIDs of all live processes in the job.
    pub fn process_ids(&self) -> AppResult<Vec<u32>> {
        #[repr(C)]
        struct PidList {
            assigned: u32,
            in_list: u32,
            ids: [usize; 256],
        }
        let mut list: PidList = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectBasicProcessIdList,
                &mut list as *mut _ as *mut c_void,
                std::mem::size_of::<PidList>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error("QueryInformationJobObject"));
        }
        let n = (list.in_list as usize).min(list.ids.len());
        Ok(list.ids[..n].iter().map(|&p| p as u32).collect())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Fetches the TCP listener table (IPv4 and IPv6) including owning PIDs.
pub fn listening_sockets() -> AppResult<Vec<ListeningSocket>> {
    let mut out = Vec::new();
    for row in tcp_table::<MIB_TCPROW_OWNER_PID>(AF_INET)? {
        out.push(ListeningSocket {
            address: IpAddr::V4(Ipv4Addr::from(u32::from_be(row.dwLocalAddr))),
            port: u16::from_be(row.dwLocalPort as u16),
            pid: row.dwOwningPid,
        });
    }
    for row in tcp_table::<MIB_TCP6ROW_OWNER_PID>(AF_INET6)? {
        out.push(ListeningSocket {
            address: IpAddr::V6(Ipv6Addr::from(row.ucLocalAddr)),
            port: u16::from_be(row.dwLocalPort as u16),
            pid: row.dwOwningPid,
        });
    }
    Ok(out)
}

/// Reads a `MIB_TCP*TABLE_OWNER_PID` (a u32 count followed by rows).
fn tcp_table<Row: Copy>(family: u16) -> AppResult<Vec<Row>> {
    let mut size: u32 = 0;
    let mut buf: Vec<u64> = Vec::new();
    for _ in 0..5 {
        let ret = unsafe {
            GetExtendedTcpTable(
                if buf.is_empty() { std::ptr::null_mut() } else { buf.as_mut_ptr() as *mut c_void },
                &mut size,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if ret == NO_ERROR {
            if buf.is_empty() {
                return Ok(Vec::new());
            }
            let base = buf.as_ptr() as *const u8;
            let count = unsafe { *(base as *const u32) } as usize;
            // Rows start after the count, aligned to the row's alignment.
            let offset = std::mem::align_of::<Row>().max(std::mem::size_of::<u32>());
            let rows = unsafe { std::slice::from_raw_parts(base.add(offset) as *const Row, count) };
            return Ok(rows.to_vec());
        }
        if ret != ERROR_INSUFFICIENT_BUFFER {
            return Err(AppError::Platform(format!(
                "GetExtendedTcpTable failed: {}",
                std::io::Error::from_raw_os_error(ret as i32)
            )));
        }
        // The table can grow between calls; add headroom.
        buf = vec![0u64; (size as usize / 8) + 64];
        size = (buf.len() * 8) as u32;
    }
    Err(AppError::Platform("GetExtendedTcpTable kept growing".into()))
}

/// Executable file name (e.g. `httpd.exe`) for a PID, when accessible.
pub fn process_image_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let full = std::ffi::OsString::from_wide(&buf[..len as usize]);
        std::path::Path::new(&full).file_name().map(|n| n.to_string_lossy().into_owned())
    }
}

/// Sets an existing named event (used for Apache's `ap<pid>_shutdown` /
/// `ap<pid>_restart` control events).
pub fn signal_named_event(name: &str) -> AppResult<()> {
    let name_w = wide(name);
    unsafe {
        let handle = OpenEventW(EVENT_MODIFY_STATE, 0, name_w.as_ptr());
        if handle.is_null() {
            return Err(last_error(&format!("OpenEventW({name})")));
        }
        let ok = SetEvent(handle);
        CloseHandle(handle);
        if ok == 0 {
            return Err(last_error(&format!("SetEvent({name})")));
        }
    }
    Ok(())
}

/// Runs `exe params` elevated through the UAC prompt and waits for it.
/// Returns its exit code; a declined prompt is `ElevationDeclined`.
pub fn run_elevated(exe: &std::path::Path, params: &str) -> AppResult<u32> {
    use windows_sys::Win32::Foundation::{ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide("runas");
    let file = wide(&exe.display().to_string());
    let params_w = wide(params);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params_w.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 {
            if GetLastError() == ERROR_CANCELLED {
                return Err(AppError::ElevationDeclined(
                    "administrator permission was declined, so nothing was changed".into(),
                ));
            }
            return Err(last_error("ShellExecuteExW(runas)"));
        }
        if info.hProcess.is_null() {
            return Err(AppError::Platform("elevated helper did not start".into()));
        }
        let waited = WaitForSingleObject(info.hProcess, 120_000);
        let mut code: u32 = 1;
        let got = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if waited != WAIT_OBJECT_0 || got == 0 {
            return Err(AppError::Platform("elevated helper did not finish within 2 minutes".into()));
        }
        Ok(code)
    }
}

/// Windows 11 look for a frameless popup: rounded corners and a thin border
/// in `border_rgb` (0xRRGGBB). Older Windows ignores both.
pub fn style_popup(hwnd: isize, border_rgb: u32) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    let (r, g, b) = ((border_rgb >> 16) & 0xff, (border_rgb >> 8) & 0xff, border_rgb & 0xff);
    let colorref: u32 = r | (g << 8) | (b << 16);
    let round = DWMWCP_ROUND;
    // SAFETY: valid window handle from the caller; attribute values are plain integers.
    unsafe {
        DwmSetWindowAttribute(hwnd as _, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&raw const round).cast(), std::mem::size_of_val(&round) as u32);
        DwmSetWindowAttribute(hwnd as _, DWMWA_BORDER_COLOR as u32, (&raw const colorref).cast(), 4);
    }
}

/// The publisher of a file's embedded Authenticode signature when Windows
/// trusts it (valid chain to a trusted root, file unchanged since signing);
/// `None` when the file is not signed. A signature that is present but not
/// trusted is an error. Revocation is not checked (no network access).
pub fn authenticode_signer(path: &std::path::Path) -> AppResult<Option<String>> {
    use windows_sys::Win32::Foundation::TRUST_E_NOSIGNATURE;
    use windows_sys::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };
    let path_w = wide(&path.display().to_string());
    let mut file = WINTRUST_FILE_INFO { cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32, pcwszFilePath: path_w.as_ptr(), ..Default::default() };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut file;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `file` point at live stack values for both calls; the
    // state is closed below whatever the result.
    let status = unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut data).cast()) };
    let signer = if status == 0 {
        // SAFETY: on success the state data holds the verified signer chain.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            let sgnr = if prov.is_null() { std::ptr::null_mut() } else { WTHelperGetProvSignerFromChain(prov, 0, 0, 0) };
            let cert = if sgnr.is_null() { std::ptr::null_mut() } else { WTHelperGetProvCertFromChain(sgnr, 0) };
            if cert.is_null() || (*cert).pCert.is_null() {
                None
            } else {
                let mut buf = [0u16; 512];
                let n = CertGetNameStringW((*cert).pCert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, std::ptr::null(), buf.as_mut_ptr(), buf.len() as u32);
                Some(String::from_utf16_lossy(&buf[..(n as usize).saturating_sub(1)]))
            }
        }
    } else {
        None
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the state opened above.
    unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut data).cast()) };
    match status {
        0 => Ok(Some(signer.unwrap_or_default())),
        TRUST_E_NOSIGNATURE => Ok(None),
        code => Err(AppError::Validation(format!("{} has a signature Windows does not trust (0x{:08X})", path.display(), code as u32))),
    }
}

const MACHINE_ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
const USER_ENV_KEY: &str = "Environment";

/// The PATH stored for the whole computer (`machine`) or for this user,
/// unexpanded (`%SystemRoot%` stays as written). Missing value: empty.
pub fn read_path_var(machine: bool) -> AppResult<String> {
    Ok(read_env_var(machine, "Path")?.unwrap_or_default())
}

/// An environment variable as stored for the whole computer (`machine`) or
/// for this user, unexpanded; `None` when it is not set. Newer than this
/// process's own environment, which is fixed when the process starts.
pub fn read_env_var(machine: bool, name: &str) -> AppResult<Option<String>> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ};
    let (root, key) = if machine { (HKEY_LOCAL_MACHINE, MACHINE_ENV_KEY) } else { (HKEY_CURRENT_USER, USER_ENV_KEY) };
    let (key_w, name_w) = (wide(key), wide(name));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut size: u32 = 0;
    // SAFETY: sizes the value first, then reads it into a buffer of that size.
    let status = unsafe { RegGetValueW(root, key_w.as_ptr(), name_w.as_ptr(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) };
    if status == 2 {
        return Ok(None); // ERROR_FILE_NOT_FOUND: not set
    }
    if status != 0 {
        return Err(AppError::Platform(format!("cannot read {name} (error {status})")));
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
    let status = unsafe { RegGetValueW(root, key_w.as_ptr(), name_w.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) };
    if status != 0 {
        return Err(AppError::Platform(format!("cannot read {name} (error {status})")));
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(Some(String::from_utf16_lossy(&buf[..len])))
}

/// Replaces this user's PATH (stored as REG_EXPAND_SZ so `%VAR%` entries keep
/// working). Needs no administrator rights.
pub fn write_user_path(value: &str) -> AppResult<()> {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_EXPAND_SZ};
    let (key_w, name_w, data) = (wide(USER_ENV_KEY), wide("Path"), wide(value));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: the key handle is closed below; data is NUL-terminated UTF-16.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key_w.as_ptr(), 0, KEY_SET_VALUE, &mut key) };
    if status != 0 {
        return Err(AppError::Platform(format!("cannot open the user environment (error {status})")));
    }
    let status = unsafe { RegSetValueExW(key, name_w.as_ptr(), 0, REG_EXPAND_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) };
    unsafe { RegCloseKey(key) };
    if status != 0 {
        return Err(AppError::Platform(format!("cannot write PATH (error {status}); security software may be blocking it")));
    }
    Ok(())
}

/// Tells running programs (Explorer, new terminals) that the environment changed.
pub fn broadcast_environment_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let what = wide("Environment");
    let mut result = 0usize;
    // SAFETY: a standard broadcast with a NUL-terminated string and a timeout.
    unsafe { SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, what.as_ptr() as isize, SMTO_ABORTIFHUNG, 3000, &mut result) };
}

/// Native error dialog for failures before the webview exists.
pub fn show_fatal_error(title: &str, message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let (t, m) = (wide(title), wide(message));
    unsafe { MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR) };
}

/// Opens a new console window (cmd.exe) in `cwd` with the given environment.
pub fn open_console(cwd: &std::path::Path, env: &[(std::ffi::OsString, std::ffi::OsString)], banner: &str) -> AppResult<()> {
    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
    let mut cmd = Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
    cmd.arg("/K").arg(format!("echo {banner}")).current_dir(cwd).creation_flags(CREATE_NEW_CONSOLE);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.spawn().map_err(|e| AppError::Platform(format!("cannot open a terminal: {e}")))?;
    Ok(())
}

/// No console window flashes, and Ctrl+C in a parent console is not forwarded.
pub fn hide_console_window(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
}

/// Which Windows certificate stores trust a root certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RootTrust {
    pub current_user: bool,
    pub local_machine: bool,
}

mod cert_store {
    use windows_sys::Win32::Security::Cryptography::*;

    use super::{last_error, wide};
    use crate::error::{AppError, AppResult};

    pub struct Store(HCERTSTORE);

    impl Store {
        pub fn open_root(location: u32, read_only: bool) -> AppResult<Store> {
            let name = wide("ROOT");
            let mut flags = location | CERT_STORE_OPEN_EXISTING_FLAG;
            if read_only {
                flags |= CERT_STORE_READONLY_FLAG;
            }
            let h = unsafe { CertOpenStore(CERT_STORE_PROV_SYSTEM_W, 0, 0, flags, name.as_ptr().cast()) };
            if h.is_null() {
                return Err(last_error("CertOpenStore(ROOT)"));
            }
            Ok(Store(h))
        }

        /// The store's copy of `cert`, if it has one.
        pub fn find(&self, cert: &Context) -> Option<Context> {
            let found = unsafe {
                CertFindCertificateInStore(self.0, X509_ASN_ENCODING | PKCS_7_ASN_ENCODING, 0, CERT_FIND_EXISTING, cert.0.cast(), std::ptr::null())
            };
            (!found.is_null()).then_some(Context(found))
        }

        /// Adding to the current user's ROOT store shows Windows' own
        /// security warning; FALSE means the user declined it.
        pub fn add(&self, cert: &Context) -> AppResult<()> {
            let ok = unsafe { CertAddCertificateContextToStore(self.0, cert.0, CERT_STORE_ADD_REPLACE_EXISTING, std::ptr::null_mut()) };
            if ok == 0 {
                return Err(AppError::ElevationDeclined(
                    "Windows did not add the certificate; its security warning was declined".into(),
                ));
            }
            Ok(())
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            unsafe { CertCloseStore(self.0, 0) };
        }
    }

    pub struct Context(pub *mut CERT_CONTEXT);

    impl Context {
        pub fn from_der(der: &[u8]) -> AppResult<Context> {
            let c = unsafe { CertCreateCertificateContext(X509_ASN_ENCODING | PKCS_7_ASN_ENCODING, der.as_ptr(), der.len() as u32) };
            if c.is_null() {
                return Err(last_error("CertCreateCertificateContext"));
            }
            Ok(Context(c))
        }

        /// Removing from the current user's ROOT store also asks for confirmation.
        pub fn delete(self) -> AppResult<()> {
            let ptr = self.0;
            std::mem::forget(self); // CertDeleteCertificateFromStore frees the context
            if unsafe { CertDeleteCertificateFromStore(ptr) } == 0 {
                return Err(AppError::ElevationDeclined(
                    "Windows did not remove the certificate; its confirmation was declined".into(),
                ));
            }
            Ok(())
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            unsafe { CertFreeCertificateContext(self.0) };
        }
    }
}

pub fn root_trust(der: &[u8]) -> AppResult<RootTrust> {
    use windows_sys::Win32::Security::Cryptography::{CERT_SYSTEM_STORE_CURRENT_USER, CERT_SYSTEM_STORE_LOCAL_MACHINE};
    let cert = cert_store::Context::from_der(der)?;
    let in_store = |loc| cert_store::Store::open_root(loc, true).ok().and_then(|s| s.find(&cert)).is_some();
    Ok(RootTrust {
        current_user: in_store(CERT_SYSTEM_STORE_CURRENT_USER),
        local_machine: in_store(CERT_SYSTEM_STORE_LOCAL_MACHINE),
    })
}

/// Adds a root to the current user's trusted roots. Windows shows its own
/// confirmation dialog; no administrator rights are involved.
pub fn trust_root(der: &[u8]) -> AppResult<()> {
    use windows_sys::Win32::Security::Cryptography::CERT_SYSTEM_STORE_CURRENT_USER;
    let cert = cert_store::Context::from_der(der)?;
    cert_store::Store::open_root(CERT_SYSTEM_STORE_CURRENT_USER, false)?.add(&cert)
}

/// Removes the root from the current user's trusted roots (if present).
pub fn untrust_root(der: &[u8]) -> AppResult<bool> {
    use windows_sys::Win32::Security::Cryptography::CERT_SYSTEM_STORE_CURRENT_USER;
    let cert = cert_store::Context::from_der(der)?;
    let store = cert_store::Store::open_root(CERT_SYSTEM_STORE_CURRENT_USER, false)?;
    match store.find(&cert) {
        Some(found) => found.delete().map(|_| true),
        None => Ok(false),
    }
}

/// Limits a folder to the current user (no inherited access for other
/// accounts). Used for private keys.
pub fn restrict_to_current_user(dir: &std::path::Path) -> AppResult<()> {
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => return Err(AppError::Platform("cannot determine the current user".into())),
    };
    let mut cmd = Command::new("icacls");
    cmd.arg(dir).args(["/inheritance:r", "/grant:r"]).arg(format!("{user}:(OI)(CI)F")).arg("/Q");
    hide_console_window(&mut cmd);
    let out = cmd.output().map_err(|e| AppError::Platform(format!("cannot run icacls: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Platform(format!(
            "icacls failed: {}",
            String::from_utf8_lossy(&out.stdout).trim()
        )));
    }
    Ok(())
}

/// The current user's SID, e.g. `S-1-5-21-…-1001`.
pub fn current_user_sid() -> AppResult<String> {
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the token handle is closed below; the buffer is sized by the first call.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(last_error("OpenProcessToken"));
        }
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8) + 1];
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32, &mut len);
        CloseHandle(token);
        if ok == 0 {
            return Err(last_error("GetTokenInformation"));
        }
        sid_string((*(buf.as_ptr() as *const TOKEN_USER)).User.Sid)
    }
}

/// # Safety
/// `sid` must point at a valid SID.
unsafe fn sid_string(sid: windows_sys::Win32::Security::PSID) -> AppResult<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    let mut s: *mut u16 = std::ptr::null_mut();
    if ConvertSidToStringSidW(sid, &mut s) == 0 {
        return Err(last_error("ConvertSidToStringSidW"));
    }
    let len = (0..).take_while(|&i| *s.add(i) != 0).count();
    let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, len));
    LocalFree(s.cast());
    Ok(out)
}

/// Owner and access-granting entries of a file or folder, as SIDs.
fn security_of(path: &std::path::Path) -> AppResult<super::Security> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        GetAce, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    };
    let path_w = wide(&path.display().to_string());
    let (mut owner, mut dacl, mut sd): (PSID, *mut ACL, PSECURITY_DESCRIPTOR) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: out-pointers are valid; `sd` owns owner/dacl and is freed below.
    unsafe {
        let rc = GetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sd,
        );
        if rc != 0 {
            return Err(AppError::Platform(format!(
                "cannot read the permissions of {}: {}",
                path.display(),
                std::io::Error::from_raw_os_error(rc as i32)
            )));
        }
        let result = (|| {
            let owner = sid_string(owner)?;
            let Some(acl) = dacl.as_ref() else {
                // No DACL at all: everyone has full access.
                return Ok(super::Security { owner, allowed: vec![super::EVERYONE.to_string()] });
            };
            let mut allowed = Vec::new();
            for i in 0..acl.AceCount as u32 {
                let mut ace: *mut c_void = std::ptr::null_mut();
                if GetAce(acl, i, &mut ace) == 0 {
                    return Err(last_error("GetAce"));
                }
                match (*(ace as *const ACE_HEADER)).AceType {
                    // Allowed and allowed-callback entries share this layout.
                    0 | 9 => allowed.push(sid_string((&raw const (*(ace as *const ACCESS_ALLOWED_ACE)).SidStart).cast_mut().cast())?),
                    // Deny entries only take access away.
                    1 | 6 | 10 | 12 => {}
                    other => allowed.push(format!("(unrecognized entry type {other})")),
                }
            }
            Ok(super::Security { owner, allowed })
        })();
        LocalFree(sd);
        result
    }
}

/// Who besides this user, SYSTEM and Administrators can reach `path`.
pub fn folder_access(path: &std::path::Path) -> AppResult<super::FolderAccess> {
    Ok(super::classify(&security_of(path)?, &current_user_sid()?))
}

/// Limits `dir` (and everything inheriting from it) to this user, SYSTEM and
/// Administrators. Names are given as SIDs, so it works on any Windows
/// language. Changing a large tree takes a while.
pub fn make_private(dir: &std::path::Path) -> AppResult<super::FolderAccess> {
    let user = current_user_sid()?;
    let icacls = |args: Vec<String>| -> AppResult<()> {
        let mut cmd = Command::new("icacls");
        cmd.arg(dir).args(args).args(["/C", "/Q"]);
        hide_console_window(&mut cmd);
        let out = cmd.output().map_err(|e| AppError::Platform(format!("cannot run icacls: {e}")))?;
        if !out.status.success() {
            return Err(AppError::Platform(format!("icacls failed on {}: {}", dir.display(), String::from_utf8_lossy(&out.stdout).trim())));
        }
        Ok(())
    };
    let grants = [super::SYSTEM, super::ADMINISTRATORS, &user].map(|sid| format!("*{sid}:(OI)(CI)F"));
    icacls(["/inheritance:r".into(), "/grant:r".into()].into_iter().chain(grants).collect())?;
    // Entries set on the folder itself (not inherited) survive the step above.
    if let super::FolderAccess::Shared(others) = folder_access(dir)? {
        let sids: Vec<String> = others.into_iter().filter(|s| s.starts_with("S-")).map(|s| format!("*{s}")).collect();
        if !sids.is_empty() {
            icacls(std::iter::once("/remove:g".to_string()).chain(sids).collect())?;
        }
    }
    folder_access(dir)
}

/// The icon embedded in an executable, as `size`×`size` RGBA pixels (top row
/// first). `None` when the file carries no icon of its own.
pub fn exe_icon_rgba(path: &std::path::Path, size: u32) -> Option<Vec<u8>> {
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, PrivateExtractIconsW, ICONINFO};

    let name = wide(&path.to_string_lossy());
    let mut icon = std::ptr::null_mut();
    let n = unsafe { PrivateExtractIconsW(name.as_ptr(), 0, size as i32, size as i32, &mut icon, std::ptr::null_mut(), 1, 0) };
    if n == 0 || n == u32::MAX || icon.is_null() {
        return None;
    }
    let mut info = ICONINFO::default();
    let ok = unsafe { GetIconInfo(icon, &mut info) } != 0;
    let read = |bitmap| -> Option<Vec<u8>> {
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        };
        let mut buf = vec![0u8; (size * size * 4) as usize];
        let dc = unsafe { GetDC(std::ptr::null_mut()) };
        let lines = unsafe { GetDIBits(dc, bitmap, 0, size, buf.as_mut_ptr().cast(), &mut bmi, DIB_RGB_COLORS) };
        unsafe { ReleaseDC(std::ptr::null_mut(), dc) };
        (lines as u32 == size).then_some(buf)
    };
    let result = if ok && !info.hbmColor.is_null() {
        read(info.hbmColor).map(|mut bgra| {
            // Old-style icons keep transparency in the mask instead of alpha.
            if bgra.chunks_exact(4).all(|p| p[3] == 0) {
                if let Some(mask) = (!info.hbmMask.is_null()).then(|| read(info.hbmMask)).flatten() {
                    for (p, m) in bgra.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                        p[3] = if m[0] == 0 { 255 } else { 0 };
                    }
                }
            }
            for p in bgra.chunks_exact_mut(4) {
                p.swap(0, 2); // BGRA -> RGBA
            }
            bgra
        })
    } else {
        None
    };
    unsafe {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
        DestroyIcon(icon);
    }
    result.filter(|px| px.chunks_exact(4).any(|p| p[3] > 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_table_contains_our_own_socket() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let sockets = listening_sockets().unwrap();
        let mine = sockets.iter().find(|s| s.port == port).expect("own listener visible");
        assert_eq!(mine.pid, std::process::id());
        assert_eq!(mine.address, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn own_image_name_is_resolved() {
        let name = process_image_name(std::process::id()).unwrap();
        assert!(name.to_ascii_lowercase().ends_with(".exe"), "{name}");
    }

    #[test]
    fn a_shared_folder_is_made_private_with_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join("tool.exe"), "x").unwrap();
        // Explicit and inheritable, like C:\ hands down to a new folder.
        let st = Command::new("icacls").arg(&home).args(["/grant", "*S-1-5-11:(OI)(CI)M", "/Q"]).status().unwrap();
        assert!(st.success());
        assert!(matches!(folder_access(&home).unwrap(), crate::platform::FolderAccess::Shared(s) if s.contains(&"S-1-5-11".to_string())));
        assert!(matches!(folder_access(&home.join("bin").join("tool.exe")).unwrap(), crate::platform::FolderAccess::Shared(_)));

        assert_eq!(crate::platform::secure_folder(&home).unwrap(), crate::platform::FolderAccess::Private);
        assert_eq!(folder_access(&home.join("bin").join("tool.exe")).unwrap(), crate::platform::FolderAccess::Private);
        assert!(std::fs::read(home.join("bin").join("tool.exe")).is_ok(), "the user keeps access");
    }

    #[test]
    fn signalling_missing_event_is_a_clear_error() {
        let err = signal_named_event("zenv_test_event_that_does_not_exist").unwrap_err();
        assert!(matches!(err, AppError::Platform(_)));
    }
}
