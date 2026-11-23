# MODULES (roadmap, edit in place, max 40 rows; adopted 2026-11-09, status backed by the tests named below)

| ID | Module | MVP/Later | Layer (backend/frontend) | Depends on | Status (planned/doing/done) |
|---|---|---|---|---|---|
| M1 | Storage: SQLite, migrations, backups | MVP | backend | - | done |
| M2 | Runtime catalog, discovery, registry, version probes | MVP | backend + frontend | M1 | done |
| M3 | Processes, services, ports, logs, diagnostics | MVP | backend + frontend | M1, M2 | done |
| M4 | Web servers: Apache, Nginx, Caddy, FrankenPHP; shared port | MVP | backend + frontend | M3 | done |
| M5 | Apps (local sites), virtual hosts, hosts file | MVP | backend + frontend | M4 | done |
| M6 | PHP per app (php-cgi pools, FrankenPHP) | MVP | backend | M5 | done |
| M7 | Databases and caches: MySQL, PostgreSQL, Redis | MVP | backend + frontend | M3 | done |
| M8 | Installers (languages, web servers, databases), PATH | MVP | backend + frontend | M2 | done |
| M9 | HTTPS: local CA, per-app certificates | MVP | backend + frontend | M5 | done |
| M10 | Proxy app programs (start commands, approval) | MVP | backend + frontend | M5 | done |
| M11 | Docker apps | MVP | backend + frontend | M5 | done |
| M12 | AI routers (OmniRoute, 9router) and Codex config | MVP | backend + frontend | M8 | done |
| M13 | Tailnet: per-port mode and Tailscale Services | MVP | backend + frontend | M5 | done |
| M14 | Studio: file tree, CodeMirror editor, formatters, file icons | MVP | backend + frontend | M5 | done |
| M15 | Studio: terminals (Claude Code, Codex, PowerShell) | MVP | backend + frontend | M14 | done |
| M16 | Studio: Claude Code chat, palette, history | MVP | backend + frontend | M15 | doing |
| M17 | Editors detection, "Open in <editor>" | MVP | backend + frontend | M5 | done |
| M18 | Tray panel, app thumbnails, windows | MVP | backend + frontend | M3 | doing |
| M19 | macOS port | Later | all | all | planned |
| M20 | Native UI (egui) without WebView, replaces Tauri + React | Later | frontend | M1-M18 | doing |

## Acceptance criteria and test plan per module

### M1-M12 core (proven)
Accept: as listed in README "Works now". Test: `npm run test:rust` (237 passed, 5 ignored as of 2026-11-09) and the e2e tests named
there (`apache_e2e`, `sites_e2e`, `nginx_e2e`, `php_e2e`, `db_e2e`, `languages_e2e`, `webservers_e2e`, `https_e2e`, `docker_apps_e2e`, `ai_router_e2e`).

### M13 tailnet: per-port mode and Tailscale Services
Accept: port mode answers `http://<pc>:<port>` from another tailnet device; a Service answers `https://<svc>.<tailnet>.ts.net` with a valid
certificate; another app's Service name is refused. Test: `tailnet_e2e` (both cases, real Apache/Nginx/FrankenPHP; passed 2026-11-09);
Real Service proven 2026-11-09: `svc:vaultwarden` opened from another tailnet device by the boss ("oke bisa").

### M14-M15 Studio editor and terminals
Accept: files confined to the project root, save refuses outside changes, formatters per language, terminals end with their window.
Test: `project_files`, `code_format`, `studio` unit tests; native `tests/studio*.rs` (Vitest `format`, `fileIcons` removed with React, task 18).

### M16 Studio chat
Accept: a prompt streams an answer; permission requests show Allow/Always/Deny; model/effort/mode/settings apply on the next message;
pasted images/files reach Claude; MCP status/toggle work; a saved session loads and continues. Test: `studio_chat` / `studio_history`
unit tests (incl. real-session read `reads_real_sessions`), native `tests/chat*.rs` and `studio::chat` unit tests (Vitest removed with React, task 18); protocol probes against the real CLI.
Open: an end-to-end run in the Studio window (not automated).

### M18 tray, thumbnails, windows
Accept: tray panel lists services; thumbnails render through headless Edge in a sandbox. Test: `thumbnails` unit tests; the tray has no automated test.

### M20 Native UI (egui) without WebView
Chosen by the boss 2026-11-18, started in task 13 ([D-021](DECISIONS.md#d-021-pure-rust-ui-with-eguieframe-instead-of-tauri--webview)).
Plan: new crate `src-native/` (bin `zenv-native` until step 9, then `zenv`) on `src-tauri/core`; dev builds use `ZENV_HOME=C:\zenv-dev` so the installed Tauri Zenv keeps
running. Crates (versions checked 2026-11-18, recheck at start): eframe/egui/egui_extras(syntect)/egui_kittest 0.37, egui_commonmark 0.26,
alacritty_terminal 0.26 (own widget; egui_term is stuck on egui 0.31), tray-icon 0.26.1, rfd 0.18; Lucide icon font vendored (ISC);
single instance by a Windows mutex; Studio preview opens in the default browser; NSIS script without tauri-cli; Vitest -> Rust + kittest.
Order: 1 shell, theme, dashboard (spike) -> 2 web servers, services -> 3 apps -> 4 runtimes, installers -> 5 tools (AI routers, tailnet,
Docker) -> 6 logs, settings -> 7 tray, windows -> 8 Studio: 8a files + editor (task 15), 8b terminals, 8c chat -> 9 installer, remove Tauri and React. Step 9 done in task 18
(2026-11-23): bin `zenv`, own NSIS installer (`installer/zenv.nsi`), Tauri shell and React deleted, verify without tsc/Vitest.
Risk gates (at the spike unless named): builds and opens on `x86_64-pc-windows-gnu` (else glow renderer, then msvc); idle private memory
<= 150 MB with the dashboard (else glow + smaller font atlas); the boss accepts the dashboard look against DESIGN.md (else restyle first);
step 8 terminal runs `claude`, vim, truecolor, resize, paste (else open Windows Terminal); editor typing <= 16 ms/frame on a 1 MB file
(else large files open in VS Code). Gate 8a result (2026-11-23, task 15): coloured 1 MB failed (~13 ms per 100 KB);
colours only up to 100 KB with lines <= 2 KB (minified files: 115 ms per key at 20 KB), plain up to 1 MB (14.8 ms median), read-only above 1 MB or a 40 KB line ([G-017](DECISIONS.md)); Studio with a 1 MB file open: 128 MB private.
Gate 8b result (2026-11-23, task 16): passed in the real window: Claude Code (colours, keys, Esc, mouse), vim, truecolor, resize, paste;
full 203x62 screen 0.6-0.9 ms per frame (release median); terminals end with their window. Own widget on `alacritty_terminal` 0.26 (F-008).
Step 8c (2026-11-23, task 17): chat tab in the native Studio (F-004, F-005), first tab when Claude Code is installed; Markdown is a port of React's parser
(no egui_commonmark). 1,000-item chat 1.1 ms per frame, streaming into one 60 KB message up to 17.4 ms (release, task 17 `long_conversation_frame_time`).
