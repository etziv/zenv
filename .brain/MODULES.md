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
Test: `project_files`, `code_format`, `studio` unit tests; Vitest `format`, `fileIcons`.

### M16 Studio chat
Accept: a prompt streams an answer; permission requests show Allow/Always/Deny; model/effort/mode/settings apply on the next message;
pasted images/files reach Claude; MCP status/toggle work; a saved session loads and continues. Test: `studio_chat` / `studio_history`
unit tests (incl. real-session read `reads_real_sessions`), Vitest `model`, `options`, `Markdown`; protocol probes against the real CLI.
Open: an end-to-end run in the Studio window (not automated).

### M18 tray, thumbnails, windows
Accept: tray panel lists services; thumbnails render through headless Edge in a sandbox. Test: `thumbnails` unit tests; the tray has no automated test.
