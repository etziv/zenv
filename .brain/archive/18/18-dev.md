# 18 M20 step 9: native installer, remove Tauri and React
## Plan
Goal: Zenv ships as the native egui app only: `zenv.exe` from `src-native`, an NSIS installer built without tauri-cli that upgrades the
existing C:\zenv install in place, and the Tauri shell + React UI removed from the repo (M20 step 9, D-021).
Steps:
1) Native entry: `zenv.exe --zenv-apply-hosts <file>` runs the elevated hosts helper before any GUI (the Tauri main did this; native
   lacked it, so "Apply hosts" through UAC would have opened a second GUI). Bin renamed `zenv-native` -> `zenv`. `build.rs` embeds
   icon, version info and the Common Controls v6 manifest via `embed-resource` (already in Cargo.lock through tauri-build).
2) Workspace: `src-tauri/Cargo.toml` becomes a virtual workspace (core + ../src-native); delete `src-tauri/src`, `build.rs`,
   `tauri.conf.json`, `capabilities/`, `gen/`, unused Tauri store icons.
3) Installer: `installer/zenv.nsi` (own script, no handlebars, no nsis_tauri_utils) + `scripts/build-installer.mjs` (cargo build
   --release, find makensis, write `installer/Zenv-setup.exe`). Same registry keys as the Tauri installer (HKCU Uninstall\Zenv,
   Software\Zenv\Zenv) so it upgrades in place; removes Tauri's `WebView2Loader.dll`; keeps the home rules from hooks.nsh (refused
   folders, icacls on a new home, zenv-home.txt, uninstall keeps data); closes only the Zenv running from $INSTDIR.
4) Remove React: `src/`, `public/`, `index.html`, `vite.config.ts`, `tsconfig.json`, `components.json`, npm dependencies; package.json
   keeps only scripts; `.gitignore`; `copy-installer.mjs`, `check-nsis-template.mjs` deleted.
5) Verify config (`.brain/project.json`): drop `lint` (tsc) and `test` (Vitest); docs and memory updated.
Files: src-native/{Cargo.toml,build.rs,windows/zenv.rc,windows/zenv.manifest,src/main.rs}, src-tauri/Cargo.toml (+ deletions),
installer/zenv.nsi, scripts/build-installer.mjs, scripts/with-toolchain.mjs, package.json, .gitignore, .brain/*, README.md, docs/ARCHITECTURE.md.
Risks: installer test clobbering the boss's real install (registry, shortcuts, killing his running Zenv) -> test build uses
PRODUCTNAME=ZenvTest and a scratch folder, kill matches the exe path only; the real upgrade of C:\zenv only after the boss says so
(it stops his running services). Upgrade leaves Tauri files behind -> delete WebView2Loader.dll.

## Test plan
- unit: none new in Rust logic (main.rs wiring); helper path proven by running the exe.
- exe: `zenv.exe --zenv-apply-hosts` (no file) exits 2-ish code (EXIT_INVALID) without a window; `--zenv-apply-hosts missing.txt` same;
  VersionInfo ProductName "Zenv", FileVersion 1.0.0; icon present (ExtractAssociatedIcon not default).
- installer (ZenvTest, scratch dir): silent install /S /D=<dir> -> zenv.exe, uninstall.exe, zenv-home.txt present; HKCU Uninstall\ZenvTest
  values; Start menu ZenvTest.lnk -> exe; refused dir (C:\) exits without files; upgrade over a folder holding WebView2Loader.dll deletes it;
  silent uninstall removes exe/uninstall/marker/shortcut/keys and keeps a data file; running exe from that dir is closed, the boss's
  C:\zenv\zenv.exe keeps running (pid unchanged).
- review focus: install dir with spaces (refused under Program Files, allowed elsewhere); reinstall same version (no prompt, overwrite);
  old Tauri install's registry values overwritten (MainBinaryName stays zenv.exe); uninstall while app running (closed first).
- device: release zenv.exe with ZENV_HOME=C:\zenv-dev opens dashboard, tray, Studio (chat tab) -> screenshot.
- full: `task.mjs check` (build-rust, test-rust, test-native).

## Test results
- step 1: debug `zenv.exe`: VersionInfo ProductName Zenv, FileVersion 1.0.0, OriginalFilename zenv.exe; icon extracted (green pixel);
  `--zenv-apply-hosts` (no file / missing file) -> exit 10, no window. First build failed (windres dropped the quoted define;
  fixed with a stringify macro); a custom manifest clashed with Rust std's own manifest ("multiple non-default manifests"), dropped.
- step 3 (ZenvTest installer, scratch folder "inst dir" with a space): refused C:\ (exit 0, no files), C:\Program Files\ZenvT and
  C:\Windows\ZenvT (exit 2, no files); fresh install exit 0: zenv.exe, uninstall.exe, zenv-home.txt, HKCU Uninstall\ZenvTest values,
  Software\Zenv\ZenvTest, Start menu link -> exe, ACL protected with SYSTEM/Administrators/LENOVO only.
  Upgrade with the test Zenv running from that folder: first run did NOT close it (32-bit NSIS -> 32-bit PowerShell cannot read a
  64-bit process Path) and still reported success; fixed (Sysnative PowerShell + error check after File): test app closed, boss's
  C:\zenv\zenv.exe pid 88692 untouched, fake WebView2Loader.dll and scratch EBWebView removed, data kept, exit 0.
  zenv.exe locked by another handle -> exit 2. Silent uninstall: exe/uninstall/marker/link/keys gone, user file + data kept,
  real Zenv keys and link untouched.
- device: release zenv.exe with ZENV_HOME=C:\zenv-dev: window "Zenv", Apps page rendered, 63 MB private, closed after.

- full check 1: build-rust ok, test-rust ok, test-native 55/1 failed: `a_mode_picked_between_turns_survives_the_restart` flaky
  (3/40 alone; chat code untouched by this task). Cause: test faked `live` while the worker's `Op::End` was still pending. Fix: wait
  for `live == None` before faking; 0/200 after (G-025).
- final: `task.mjs check` build-rust OK, test-rust OK, test-native OK (after the G-025 fix); real installer rebuilt
  (ProductName Zenv, no ZenvTest string), 11.87 MB.

## Review round 1 fixes (qa FAIL, security 2 medium)
- qa 1 (Must): `/D=C:\` was dropped by NSIS (AllowRootDirInstall false) -> .onInit fell back to C:\zenv; my "refused C:\ (exit 0)"
  result above was a FALSE PASS. qa's test build then installed into the boss's C:\zenv: his Zenv, frankenphp and mysqld were killed,
  zenv.exe became the native build, uninstall.exe a ZenvTest uninstaller, ZenvTest keys + Start menu link left. Data untouched.
  Restoring with his old Tauri installer was refused by the auto-mode classifier -> left for the boss (HAMBATAN card).
  Fix: `AllowRootDirInstall true`, `SetErrorLevel 2` before every refusal Quit, test builds (PRODUCTNAME != Zenv) default to
  `$TEMP\<name>` instead of C:\zenv. ZenvT18 build: C:\, D:\, Program Files, Windows -> exit 2; no /D -> %TEMP%\ZenvT18;
  C:\zenv files byte-identical (name/size/mtime) before and after every run.
- security 1+2: owner check after the lock step (deep for a new home, top level for an existing one), PowerShell fails closed.
  Dry run on the live C:\zenv (read-only): every entry own. Entry whose ACL cannot be read (OWNER RIGHTS:(RD)) -> exit 2, no exe.
  A foreign owner cannot be created without admin rights: that branch is not run, only the SID comparison it shares.
  Runtime: `ensure_safe_to_elevate` also checks every file next to the exe; new test
  `elevation_is_refused_when_another_account_can_change_a_file_next_to_the_exe` (hosts tests 10/10).
- qa 2: `SetCurrentProcessExplicitAppUserModelID("dev.zenv.desktop")` at start of `run()` (not proven on the taskbar).
- qa 3/4/5: SCAFFOLD, COMPATIBILITY, MODULES fixed; ruling text corrected; `-j` (`ZENV_JOBS`, default 2) in build-installer.
- Retest ZenvT18: path with space + apostrophe fresh install exit 0; upgrade with the app running from it -> closed, exit 0,
  WebView2Loader + EBWebView removed; locked exe -> exit 2; uninstall keeps data, removes key and link. Real installer rebuilt.

- full check 2: test-native `tests/chat.rs` failed once: `a_message_typed_while_busy_waits_its_turn_and_a_failed_start_is_shown`
  asserted [user, user] and busy before the worker's async start error could land (12/60). Assertion now checks the first two kinds
  and the cleared box; 0/100 after. Every other native test binary 10x: 0 failures.

- review round 2: security 0 open (both fixed); qa PASS WITH NOTES. qa 6 (Should): test builds still read InstallDirRegKey ->
  wrapped in `!if PRODUCTNAME == Zenv`; ZenvT19 with a key pointing at a scratch folder, no /D -> %TEMP%\ZenvT19 (exit 0), scratch
  folder untouched, C:\zenv unchanged; test copy uninstalled. Real installer rebuilt. Final `task.mjs check` OK (all 3, run after the
  last code change; zenv.nsi is not in verify).

## Rulings
- Keep the `src-tauri/` folder name as the Cargo workspace (core stays at `src-tauri/core`) | renaming touches every path in
  verify, scripts, tests, docs and memory for no behaviour change | a later mechanical move task if the boss wants a clean name
- Exe resources through `embed-resource` (in Cargo.lock via tauri-build) instead of a hand-rolled windres call | handles GNU and MSVC |
  one build-dependency
- Prettier formatting and CodeMirror extras go with React (accepted in task 15) | native has gofmt/rustfmt/ruff only | JS/TS/PHP files
  have no formatter until a task adds one
- Setup closes a running Zenv with Stop-Process (as the Tauri installer did), after asking | the native app has no "quit" signal yet
  | services are killed, not stopped gracefully (MySQL runs crash recovery on next start)
- Upgrade over a Tauri install deletes `WebView2Loader.dll` and `%LOCALAPPDATA%\dev.zenv.desktop\EBWebView` (325 MB here) | only the
  removed WebView used them | the old React UI's localStorage (theme) is gone; the native app keeps its own in %APPDATA%\Zenv
- Desktop shortcut offered unchecked on the finish page | an upgrade keeps the boss's existing `OneDrive\Desktop\Zenv.lnk` (qa) | one click to tick it
- Package name stays `zenv-native` (only the bin is `zenv`) | keeps `-p zenv-native` in verify and tests | none
- Uninstall leaves `%APPDATA%\Zenv` (egui settings, a few KB) | Tauri's "delete app data" box is not ported | a stray folder
- qa 7 (Nice): the chat test no longer asserts `busy` right after the second Enter | it raced the worker's async start error | the
  queued-while-busy state is no longer checked at that instant; the end state (error notice, turn ended) still is
- Restore of C:\zenv left to the boss | the auto-mode classifier refused running his old installer | his Zenv stays stopped until he acts
