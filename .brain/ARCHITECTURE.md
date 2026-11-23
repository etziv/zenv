# ARCHITECTURE (adopted from the code 2026-11-09; the detailed design is docs/ARCHITECTURE.md)

Status: approved 2026-11-09 (adopted from the code)     Versions verified: 2026-11-09
Platform: other: Windows desktop app (Windows 10/11 x64). macOS later, only when the boss asks.

## Chosen stack (with versions, from package-lock / Cargo.lock, 2026-11-09)
- UI: egui/eframe 0.37 (glow), egui_extras/syntect (editor colours), alacritty_terminal 0.26 (Studio terminals), tray-icon,
  rfd (file dialogs); bin `zenv.exe` from `src-native`, resources by `embed-resource` (task 18). Tauri + React removed 2026-11-23 (task 18, D-021).
- Installer: own NSIS 3 script `installer/zenv.nsi` built by `scripts/build-installer.mjs` (makensis from `ZENV_MAKENSIS`, Tauri's
  NSIS cache or Program Files); same registry keys as the old Tauri installer, so it upgrades C:\zenv in place.
- Backend: Rust 1.99.0 on `x86_64-pc-windows-gnu` (needs a MinGW-w64 GCC: `ZENV_MINGW_BIN`, `scripts/with-toolchain.mjs`).
  rusqlite 0.40.2 (bundled SQLite), minijinja 2.24.0 (server config templates), ureq 3.4.2 (downloads),
  portable-pty 0.9.0 (ConPTY terminals), rcgen (local CA), zip / sevenz-rust2 (installers).
- Tests: Rust unit tests, egui_kittest UI tests (`src-native/tests`); ignored e2e tests against real servers (`core/tests/*_e2e.rs`).

## Why this and rejected options
- Rust (first Tauri, now egui, [D-021](DECISIONS.md#d-021-pure-rust-ui-with-eguieframe-instead-of-tauri--webview)): a small native Windows app that can own processes (Job Objects), ports and files; see docs/ARCHITECTURE.md.
- Rejected in the repo's history: Electron-style heavy shell (not used); a reimplemented Claude agent in Rust (Studio runs the official `claude.exe`, [D-012](DECISIONS.md#d-012-studio-chat-runs-the-official-claude-code-over-pipes)).

## Folder structure
- `src-tauri/` is only the Cargo workspace root now (members `core`, `../src-native`; name kept, task 18) plus `icons/`.
- `src-native/` crate `zenv-native`, bin `zenv` (`main.rs`: elevated hosts helper `--zenv-apply-hosts` first, then the GUI;
  `build.rs` + `windows/zenv.rc`: icon and version info):
  egui/eframe 0.37 UI on `Core` directly. `app.rs` (frame, tasks, toasts), `shell.rs` (header, port LED rail), `pages/<area>.rs`
  (page struct + pure `view` fns; data via `loader::Loader`, writes via `Action::Run(Task)`), `widgets.rs`, `theme.rs` (DESIGN.md
  tokens), `lucide.rs` (generated icon consts), `assets/fonts/` (Chakra Petch, IBM Plex Sans, JetBrains Mono TTF, Lucide; OFL/ISC).
  Tests: `tests/*.rs` egui_kittest; `--ignored render_dashboard_png` writes `src-tauri/target/native-shots/*.png`.
- `src-tauri/core/` crate `zenv-core`: all domain logic, no GUI dependency; `src/storage/migrations/*.sql`; `tests/*_e2e.rs`.
- `src-tauri/core/templates/<server>/` server config templates (minijinja, `include_str!` in `config_generator`); file icons: `src-native/assets/file-icons.png` atlas (Material Icon Theme, MIT).
- `scripts/`: `with-toolchain.mjs` (Rust env), `build-installer.mjs`; `installer/zenv.nsi` -> `installer/Zenv-setup.exe`.
- Runtime home `C:\zenv` (`ZENV_HOME` overrides): `bin/<component>/<version>`, `data`, `etc`, `logs`, `dev`, `backups`, `downloads`, `zenv.db`.

## Module -> component map (layering)
`src-native` page -> `loader::Loader` (reads) / `Action::Run(Task)` (writes, off the UI thread) -> `Core` (`core/src/app_core.rs`) which owns `Db`, `ServiceManager`, `SiteManager`,
`Installer`, `Tls`, `PhpRuntimes`, `AppProcesses`, studio `Terminals` and `Chats` -> domain modules
(`runtime_*`, `service_manager`, `web_server/*`, `local_sites`, `virtual_host`, `domain_manager`, `database_manager`,
`cache_manager`, `tls`, `tailnet`, `docker_apps`, `ai_router`, `app_process`, `studio*`, `project_files`, `code_format`)
-> `storage` (SQLite) and `platform/windows` (Job Objects, TCP tables, UAC, CryptoAPI).
Studio windows are labelled `studio-<site id>`; terminals and chats belong to the window that opened them (`studio_owner`).

## Data model overview (SQLite `zenv.db`, WAL, `PRAGMA user_version` = 14)
- `installations` (registered/managed component versions), `services` (+ `service_events`), `managed_files` (ledger with SHA-256).
- `sites` (apps: domain, project/document root, strategy static|php|reverse_proxy, upstream port, PHP version, engine,
  https_enabled, https_redirect, enabled, config_status), `app_commands` (approved start commands of proxy apps).
- `tailnet_apps` (site -> loopback port, optional Tailscale Service name), `docker_apps` (site -> container/compose),
  `path_choices` (one version per component on the user PATH), `app_settings` (key/value).
- Migrations `0001`..`0014` in `core/src/storage/migrations/`; backup before upgrade, newer schema refused.
- Outside the DB: eframe storage `%APPDATA%\Zenv\data` (route, theme, Studio layout and chat options); Claude Code's own session files are read, never written.

## Verification commands (exact; used by builders and QA)
- build: `node scripts/with-toolchain.mjs cargo check --manifest-path src-tauri/Cargo.toml`; no lint step (no Clippy configured)
- test: `cargo test ... -p zenv-core -j 1` (test-rust) and `-p zenv-native -j 1` (test-native), exact lines
  in project.json; `-j 1` because commit memory runs out at higher parallelism on this PC (G-011)
- other: `npm run test:e2e` with `ZENV_TEST_*` roots (real Apache/Nginx/PHP/FrankenPHP/MySQL/...); `npm run installer`
  builds `installer/Zenv-setup.exe` (extra makensis `/D` args pass through: `/DPRODUCTNAME=ZenvTest` for a test install that
  leaves the real Zenv's registry and shortcuts alone).

## Testing strategy
- Pure logic as unit tests next to the code (Rust `#[cfg(test)]`); UI behaviour in egui_kittest (`src-native/tests`).
- Anything touching real servers, Docker, Tailscale or installers as `#[ignore]` e2e tests in `core/tests/`, run by hand with real binaries.
- Studio chat protocol checked against the real `claude` CLI before it ships (probe scripts, no tokens where possible).

## Security baseline (from the code and docs/ARCHITECTURE.md)
- `Core` methods take typed input only, never a program, argument list or arbitrary path to run or read.
- Processes: allow-listed executables, one Job Object per service; terminals/chats only for the owning Studio window.
- Files: Zenv never edits a server installation's own config, never deletes project files or unmanaged installs, overwrites
  only ledger files (backup first); project file access confined to the project root; `.env` read-only.
- Network: services on 127.0.0.1 by default; tailnet access checks the Host name; local CA name-constrained to `.test`, `.localhost`, `.example`.
- Downloads: official sources with SHA-256 / signature checks; npm routers without install scripts.
- Installer: per-user, refuses a drive root, Windows and Program Files, locks a new home with icacls before any file lands;
  closes only the zenv.exe running from its own folder; uninstall keeps everything Zenv created.

## Risk gates (pass criterion + pre-chosen fallback each)
- Claude Code stream-json protocol changes: probe passes (`initialize`, a prompt, a permission request) -> else keep the terminal tab as the fallback.
- Tailscale Services on the real tailnet: `https://<svc>.<tailnet>.ts.net` answers from another device -> else the per-port tailnet mode.

## Conformance checklist (used by dev and QA)
- [ ] stack and versions as above
- [ ] new IPC command: typed input, owner/path checks, listed in `lib.rs`
- [ ] schema change = a new numbered migration, never an edited old one
- [ ] new domain logic in `zenv-core` with unit tests; real-binary behaviour in an e2e test
- [ ] UI uses the design tokens in `src/index.css` (DESIGN.md)

## Out of scope
- macOS/Linux builds (later, on request); project profiles / "Stacks" (dropped by the boss, [D-002](DECISIONS.md#d-002-no-project-profiles-or-stacks)); a Cloudflare tunnel (removed, [D-005](DECISIONS.md#d-005-tailscale-instead-of-a-public-tunnel)).
