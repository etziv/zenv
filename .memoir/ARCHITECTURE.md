# ARCHITECTURE (adopted from the code 2026-11-09; the detailed design is docs/ARCHITECTURE.md)

Status: approved 2026-11-09 (adopted from the code)     Versions verified: 2026-11-09
Platform: other: Windows desktop app (Windows 10/11 x64, WebView2). macOS later, only when the boss asks.

## Chosen stack (with versions, from package-lock / Cargo.lock, 2026-11-09)
- Shell: Tauri 2.12.5 (`@tauri-apps/api` / `@tauri-apps/cli` 2.12.4), plugins opener, dialog, single-instance; NSIS installer.
- Backend: Rust 1.99.0 on `x86_64-pc-windows-gnu` (needs a MinGW-w64 GCC: `ZENV_MINGW_BIN`, `scripts/with-toolchain.mjs`).
  rusqlite 0.40.2 (bundled SQLite), minijinja 2.24.0 (server config templates), ureq 3.4.2 (downloads),
  portable-pty 0.9.0 (ConPTY terminals), rcgen (local CA), zip / sevenz-rust2 (installers).
- Frontend: React 19.3.1, TypeScript 7.0.2, Vite 8.3.4, Tailwind CSS 4.3.3, shadcn/ui on radix-ui, TanStack Query 5.104.3,
  CodeMirror 6 + Prettier 3 (Studio editor), xterm.js 6.0.0 (Studio terminals), lucide-react icons.
- Tests: Vitest 5.0.5 + Testing Library (jsdom); Rust unit tests; ignored e2e tests against real servers (`core/tests/*_e2e.rs`).

## Why this and rejected options
- Tauri + Rust: a small native Windows app that can own processes (Job Objects), ports and files; see docs/ARCHITECTURE.md.
- Rejected in the repo's history: Electron-style heavy shell (not used); a reimplemented Claude agent in Rust (Studio runs the official `claude.exe`, [D-012](DECISIONS.md#d-012-studio-chat-runs-the-official-claude-code-over-pipes)).

## Folder structure
- `src/` React UI: `app/` (shell, routes, navigation), `features/<area>/` (apps, dashboard, logs, runtimes, settings,
  sites, studio, tools, tray, web-servers), `lib/api/` (typed `invoke` client, queries, types), `components/ui/` (shadcn).
- `src-tauri/src/` Tauri shell: `commands.rs` (the whole IPC surface), `lib.rs` (setup, windows, command list),
  `studio_window.rs`, `router_window.rs`, `tray.rs`.
- `src-tauri/core/` crate `zenv-core`: all domain logic, no GUI dependency; `src/storage/migrations/*.sql`; `tests/*_e2e.rs`.
- `src-tauri/core/templates/<server>/` server config templates (minijinja, `include_str!` in `config_generator`); `public/file-icons/` vendored Material Icon Theme SVGs (MIT).
- `scripts/`: `with-toolchain.mjs` (Rust env), `copy-installer.mjs`, `check-nsis-template.mjs`; `installer/Zenv-setup.exe` output.
- Runtime home `C:\zenv` (`ZENV_HOME` overrides): `bin/<component>/<version>`, `data`, `etc`, `logs`, `dev`, `backups`, `downloads`, `zenv.db`.

## Module -> component map (layering)
React feature -> `api.*` in `src/lib/api/client.ts` -> `#[tauri::command]` in `commands.rs` (narrow, typed, no raw exec;
blocking work through `blocking()`) -> `Core` (`core/src/app_core.rs`) which owns `Db`, `ServiceManager`, `SiteManager`,
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
- Outside the DB: localStorage in the UI (theme, Studio layout and chat options); Claude Code's own session files are read, never written.

## Verification commands (exact; used by builders and QA)
- build: `npm run build` (tsc + vite) and `node scripts/with-toolchain.mjs cargo check --manifest-path src-tauri/Cargo.toml`
- lint: `npm run typecheck` (no ESLint/Clippy configured)
- test: `npm test` (Vitest) and `npm run test:rust` (zenv-core unit tests)
- other: `npm run test:e2e` with `ZENV_TEST_*` roots (real Apache/Nginx/PHP/FrankenPHP/MySQL/...); `npm run installer`
  builds `installer/Zenv-setup.exe`; `npm run check:nsis` after a Tauri CLI upgrade.

## Testing strategy
- Pure logic as unit tests next to the code (Rust `#[cfg(test)]`, Vitest `*.test.ts`).
- Anything touching real servers, Docker, Tailscale or installers as `#[ignore]` e2e tests in `core/tests/`, run by hand with real binaries.
- Studio chat protocol checked against the real `claude` CLI before it ships (probe scripts, no tokens where possible).

## Security baseline (from the code and docs/ARCHITECTURE.md)
- IPC commands take typed input only, never a program, argument list or arbitrary path to run or read (`commands.rs` header).
- Processes: allow-listed executables, one Job Object per service; terminals/chats only for the owning Studio window.
- Files: Zenv never edits a server installation's own config, never deletes project files or unmanaged installs, overwrites
  only ledger files (backup first); project file access confined to the project root; `.env` read-only.
- Network: services on 127.0.0.1 by default; tailnet access checks the Host name; local CA name-constrained to `.test`, `.localhost`, `.example`.
- Downloads: official sources with SHA-256 / signature checks; npm routers without install scripts.
- CSP: `default-src 'self'`, `img-src 'self' data:`, `frame-src http: https:` (Studio preview).

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
