# 18 security review: native installer, Tauri + React removed (round 2)

## Plan
Round 2, scoped: re-check findings 1 and 2 of round 1 against the fix diff only (no new audit): `installer/zenv.nsi` (owner check after
the lock step, `AllowRootDirInstall true`, `SetErrorLevel 2`), `src-tauri/core/src/domain_manager/hosts.rs` (`ensure_safe_to_elevate`
plus its new test), and the new `SetCurrentProcessExplicitAppUserModelID` call in `src-native/src/main.rs`. Evidence read: `.brain/handoff/18-dev.md`
"Review round 1 fixes". Nothing was installed and C:\zenv was not touched.

Round 1 scope and method (kept for reference): the tree vs HEAD 08366ae on the four named surfaces; `supply-chain-ci.md`; hand trace of
the installer and elevation path; release `zenv.exe` imports (objdump) and manifest; `icacls C:\` (read-only).

## Findings

1. [medium] [fixed] Another account can pre-create the install folder and get its code run as the installing user (`installer/zenv.nsi:L133-156`)
   - Round 1: a `C:\zenv` that another account created with a `zenv-home.txt` skipped icacls and was never owner-checked, so a planted `dwmapi.dll` ran as the installing user.
   - Fix checked: after the lock step, Setup now always runs one Sysnative PowerShell check (L148-156). The path arrives through `ZENV_SETUP_DIR` and is only used with `-LiteralPath`, so nothing is interpolated into the command. The check compares the owner SID of the folder, plus its top-level entries (marker branch) or every entry recursively (new home), with the current user, SYSTEM and Administrators. Any mismatch, or any error (`$ErrorActionPreference = 'Stop'`: an unreadable ACL or an access-denied subfolder ends PowerShell with exit 1; a failed nsExec returns "error"), leads to a refusal with exit 2 before `ZenvCloseRunning` and `File`.
     - The attack cases are refused. A folder owned by the other account is refused whether or not it holds the marker. When it has no marker, icacls first fails closed for lack of WRITE_DAC, or, when the other account granted WRITE_DAC, the owner check refuses it.
     - The other account cannot make its file look like it belongs to the user or to Administrators: that takes SeRestore or SeTakeOwnership.
     - Dev evidence: an entry with an unreadable ACL gives exit 2 and no exe; an own folder (read-only dry run on C:\zenv) passes. A foreign owner could not be created without admin, so that branch shares only the SID comparison that was tested.
   - Status: closed.

2. [medium] [fixed] A race between CreateDirectory and icacls leaves another account's DLL in a new home, which the elevated hosts helper would load as admin (`installer/zenv.nsi:L133-156`, `src-tauri/core/src/domain_manager/hosts.rs:L342-363`)
   - Round 1: a file planted during the window between CreateDirectory and icacls kept its foreign owner, and `ensure_safe_to_elevate` checked only the folder and the exe.
   - Fix checked (installer): for a new home the owner check runs recursively after icacls, so a file or subfolder planted in the window (owned by the other account) is refused. Once the lock holds, the other account has no grant on the folder and cannot add entries. Its only way in would have been a subfolder it owns, and the recursive check catches that.
   - Fix checked (runtime): `ensure_safe_to_elevate` now runs `folder_access` on the folder and on every regular file directly in it, the exe included. That function catches both a foreign owner and a foreign grant. A `read_dir` error refuses elevation. Test `elevation_is_refused_when_another_account_can_change_a_file_next_to_the_exe` grants Everyone Modify on `dwmapi.dll` and expects the refusal to name it (dev reports the hosts tests 10/10; I did not rerun them).
   - Status: closed.

New in the fix diff, checked:
- `SetCurrentProcessExplicitAppUserModelID` runs at the start of `run()`, after the hosts helper check, with the constant `dev.zenv.desktop`; nothing is taken from input. No finding.
- `AllowRootDirInstall true`: `/D=C:\` now reaches `ZenvInstDirIsBad` (length <= 3) and is refused with exit 2, where before it silently fell back to C:\zenv. Test builds (PRODUCTNAME other than Zenv) default to `$TEMP\<name>`. No finding.

Rejected:
- Round 1 rejections stand (code unchanged): helper check equivalence, PowerShell injection, Stop-Process scope, EBWebView RMDir, refused-folder bypass, embed-resource, asInvoker manifest, Tauri CSP/IPC removal, makensis location.
- A handle opened during the window and kept to add a file after the lock: not raised. NTFS checks add-file access against the parent's current security descriptor when the file is created, and I have no trace showing otherwise.

Hardening (not counted):
- The marker branch checks owners only, not grants. An existing home that the user owns but whose ACL is still shared (an old home never started since Zenv began locking it) stays writable by others until `secure_home` locks it at first start. Locking the top-level folder there too would close that window.
- `ensure_safe_to_elevate` skips non-regular entries (`is_file()` is false for a symlink). Today another account cannot add one to a locked folder; checking every entry would keep the rule simple.
- `secure_home`/`make_private` leave files deeper in a home that another account owns (for example under `bin\`) owned by that account. The installer now refuses such entries at the top level of an existing home only.
- Round 1 items still apply: grant icacls by SID, pin or hash-check makensis, Authenticode-sign the installer and the exe, `SetDefaultDllDirectories` early in `main`.

Findings: high 0, medium 0, low 0
Safe to proceed: yes
Not checked: the installer was not run (no second local account, so the foreign-owner refusal and the race were not reproduced; C:\zenv off-limits); `hosts.rs` tests not rerun by me (to avoid colliding with the dev's cargo builds, G-006); NSIS 3.13 provenance; DLL load order observed live.
