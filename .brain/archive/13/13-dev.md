# 13 M20 native egui UI
## Plan
Goal: start M20 (D-021): Zenv's UI in pure Rust (egui/eframe 0.37) on `zenv-core`, no WebView. The boss: "lanjut task ... paralel jika bisa lebih cepat".
Step 1 (this session, serial: everything else builds on it): spike = new crate `src-native/` (bin `zenv-native`), theme from DESIGN.md,
header (wordmark, nav, theme switch, port LED rail), Dashboard (service tiles with ON/OFF/ERR switch, Start apps / Stop all, Languages, Health),
background job runner, page stubs for the other routes. Then the risk gates (gnu build, idle memory, screenshot).
Steps 2-6 after the spike: parallel workers, one page module each (owned files under `src-native/src/pages/`), contract = the spike's
`Page` API (`ui(&mut self, ui, &Data) -> Vec<Action>` style) and widgets.
Files: src-native/** (new), src-tauri/Cargo.toml (workspace member), .brain/project.json (verify test-native), .brain/MODULES.md, DECISIONS.
Risks: wgpu on windows-gnu (fallback glow); memory > 150 MB (glow + smaller atlas); fonts are woff only in node_modules (convert to TTF).

## Test plan
- unit: service grouping + sort (component, version desc, name); version compare ("9.7.0" > "9.3.0" > "8.0.30", "10" > "9");
  LED rail lists only running/failed services; start-apps label (stopped count / "Apps running" / disabled when none);
  stop-all hidden when nothing runs; job runner delivers a result and an error.
- kittest: dashboard renders group headings and a tile per service; clicking a tile switch emits Start/Stop for that id;
  empty group shows its "Add ..." button that navigates; error state shows the message; loading state shows a spinner label.
- review focus: a failed service (ERR pill, red LED in rail); service without installation (no version text); a web server
  behind another ("via X"); a core call failing (error shown, UI keeps working); zero services ("No services yet").
- measurements: idle private memory with the dashboard target <= 150 MB; build on x86_64-pc-windows-gnu.
- device/browser: run `zenv-native` with ZENV_HOME=C:\zenv-dev, screenshot the dashboard in dark and light, compare to DESIGN.md.

## Parallel split (steps 2-6; the boss asked "paralel")
Contract for every part: a page struct `XPage` with `ui(&mut self, ui, cx: &mut Cx)` (pages/mod.rs); data through `Loader<T>`
(`refresh_every`, `invalidate` when `cx.tasks_finished`); every core write is `cx.actions.push(Action::Run(Task::new(key, label, job)))`
(busy state from `cx.busy`, errors become toasts); pure `view` fns tested with egui_kittest like tests/dashboard.rs; widgets from
widgets.rs (read-only for parts), icons from lucide.rs, colors from theme.rs. No file outside the owned list.
| Part | Routes | Owned files | Model | Acceptance check |
|---|---|---|---|---|
| A components | Languages, Web Servers, Databases, Cache | src-native/src/pages/components.rs + pages/components/**, tests/components.rs | sonnet | its kittest + unit tests pass, `cargo build -p zenv-native` clean |
| B apps | Apps | src-native/src/pages/apps.rs + pages/apps/**, tests/apps.rs | sonnet | same |
| C tools/logs/settings | Tools, Logs, Settings | pages/{tools,logs,settings}.rs + pages/{tools,logs,settings}/**, tests/{tools,logs,settings}.rs | sonnet | same |

## Test results
- step 1 build (debug, x86_64-pc-windows-gnu, wgpu): `cargo build -p zenv-native` -> Finished, 0 errors (gate 1 passed)
- step 1 tests: `cargo test -p zenv-native` -> lib 1 passed; tests/dashboard 10 passed, 1 ignored (PNG render)
- step 1 look: `--ignored render_dashboard_png` -> target/native-shots/dashboard-{dark,light}.png read: header + LED rail + tiles
  (ON/OFF/ERR) + languages match DESIGN.md in both themes
- step 1 memory gate (release, ZENV_HOME=C:\zenv-dev, dashboard idle 15-20 s): wgpu 462 / 441 MB private -> FAILED; glow 58.0 / 57.4 MB
  private, 110 MB working set -> PASSED (target <= 150). Fallback applied: glow only. exe 40 MB.
- step 1 live run: PrintWindow capture of the real window (core on C:\zenv-dev): first run content overflowed ~30 px right
  (Frame inside ScrollArea); fixed by moving the margin to the CentralPanel frame; recapture shows header, rail, empty states, health "All 2 checks passed".

## Rulings
- bin named `zenv-native` until step 9 | the Tauri package already builds `zenv.exe` into the same target dir (cargo filename collision) | rename at step 9
- `src-native` is a member of the `src-tauri` workspace (`package.workspace`) | one Cargo.lock and target dir, zenv-core compiled once | move the workspace root at step 9
- glow renderer, not wgpu | wgpu idled at 441-462 MB private on this PC, glow at 57 MB (the plan's pre-chosen fallback) | none known; glow is egui's mature backend
- background loops (autostart, tailnet names, router update) moved from the Tauri lib.rs into `Core::spawn_background` | both shells need them, one copy | none (same code, router thread now holds a Weak)
- crates as approved on the PROPOSAL card (D-021): eframe/egui_extras/egui_kittest 0.37.0 (rechecked 2026-11-18) | boss chose A | none

## Side requests
- 2026-11-18 boss: "untuk uinya design layoutnya harus tetap sama lo jangan berubah" -> D-022; round 2 below. Done when every page pair matches.

## Round 2: match React exactly (D-022)
- Reference: React UI on Vite :1420 with mocked Tauri IPC (scratch `mock-ipc.js` + `react-shot.cjs`, playwright-core from the MCP's npx cache,
  no new dependency); native: kittest renders through `shell::frame` (the same frame as the app) at 1280x820.
- Lead: theme tokens = `src/index.css` exactly (DESIGN.md had stale dark values: fixed); header (underline tabs, version, group rules,
  32px icon buttons), rail (no services / all services stopped, red failed port, LED trace), page container (max 1280, px 32, pt 28),
  silkscreen sections, buttons (h-8/h-7, outline=input border, dashed), StatusSwitch, Pulse, ComponentLogo monogram, version chips,
  EmptyState, ErrorPanel, Toaster (popover, left bar, title + description, action), crash notices (running -> failed), inline
  task errors in tiles (`Task::inline`, `Shared.errors`). Dashboard pair react-Dashboard.png / dashboard-dark.png read: matches.
- Parts A/B/C: one worker each matches its pages (brief in scratch MATCH-BRIEF.md).
- step: `cargo test -p zenv-native -j 2 --lib --test dashboard` -> 19 + 13 passed (incl. crash notice and inline error tests)
- Unexplained: one release launch on the Languages route exited within 9 s (screenshot loop right after a force-killed instance);
  not reproduced in 10 further launches; a panic hook now writes any panic to zenv.log.

## Round 2b: part A parity, done by the lead (2026-11-19, boss: "lanjut a")
Worker A reported only cosmetic leftovers, but its own test failed (stale glyph label) and the component rows miss React behavior.
Plan: 1) language list: core order (React does not sort), rows px16/py10 with full-width dividers, logo 48. 2) `widgets::confirm_modal`
+ `Kind::Destructive` (settings/https and apps reuse them). 3) installation menu shared by language, version and server rows, with the
Uninstall/Remove confirm (React ConfirmDialog text) and FrankenPHP "Switch X on/off" item. 4) version_row: dashed card, ⋯ menu
("Set up a server on X", installation items). 5) server_row = React ServiceCard: switch disabled when exe missing / needs init /
data blocked; address with https port or data dir; copy connection URL; Initialize + confirm; notices (needs init, data blocked,
initialized, probe, validation); menu: reload, restart, test connection, check configuration, logs & activity, config file, document
root / data folder, edit / delete (disabled while running, delete confirm), another server on X + installation items.
6) Logs & activity expander (Logs: source buttons + follow + box max 288; Activity: events, relative time, level colors).
Files: src-native/src/widgets.rs, pages/components.rs, pages/components/{detail,dialogs}.rs, new pages/components/service_row.rs,
pages/settings/https.rs, pages/apps.rs, tests/components.rs.
### Test plan
- kittest: language rows in core order; Uninstall opens a confirm, Confirm emits `installation:uninstall:<id>`, Cancel emits nothing;
  version row ⋯ has "Set up a server on X"; FrankenPHP switch item emits `installation:enable:<id>`; server ⋯ Delete opens confirm
  -> `service:delete:<id>`; Edit/Delete disabled while running; needs-init MySQL shows Initialize + warning and a disabled switch;
  data-blocked shows the error notice; Logs & activity toggles the panel with Logs/Activity tabs.
- unit: notice store set/clear; address text (web https port, db data dir).
- review focus: running server (Delete disabled), missing executable (switch disabled), no data dir (no Data folder item),
  managed vs registered install (Uninstall vs Remove text), router component (panel still shows).
- device: `--include-ignored` PNG renders of Databases (mysql) and Web Servers compared to React shots.
### Test results (round 2b)
- worker A's suite rerun: `cargo test -p zenv-native -- --include-ignored` -> components 15 passed, 1 FAILED (`open_terminal_menu_item...`
  queried the glyph label; icon_button labels "More actions" since round 2) -> test fixed.
- `cargo test -p zenv-native --test components -- --include-ignored` -> 25 passed (9 new: core order, uninstall confirm + cancel,
  Remove for external installs, version row menu + FrankenPHP switch, delete confirm + disabled while running, MySQL initialize +
  blocked switch, foreign data notice, logs & activity toggle, notice store).
- `cargo test -p zenv-core --lib util::` -> 1 passed (relative_time, port of React formatRelative incl. bad input).
- PNG pairs read: native-Databases-mysql / react-Databases-mysql, native-WebServers-apache / react-WebServers match; fixed on the way:
  notes footer clipped (height now measured, not guessed) and a long distribution line ran under the header buttons (now wraps, buttons top).
### Rulings (round 2b)
- confirm dialogs close on Confirm and the task error shows inline in the row | the row already shows inline task errors; React keeps the dialog open while pending | a slow uninstall shows no spinner in the dialog, only the row
- "Data folder" menu item only when the service has a data dir | React shows it always and the reveal then fails | none
- Copy connection URL copies without a toast | other native copy buttons (tailnet, tools) do the same | boss may want the toast
- menu items keep text only, no lucide icons | every native popup menu is text-only so far | visual gap vs React menus, one shared fix later
- worker A's last leftovers closed by the lead: install dialog (source + "Checked by" line, right-aligned pre-release checkbox,
  bordered list with dividers, size via `format_bytes` = React formatBytes, ✓ Installed, small Install button disabled while a job
  runs, width 472); sidebar rows hover fill + padding 12; `widgets::modal` = every dialog padded 20 like React's p-5 (apps, components,
  settings). `cargo test -p zenv-native --lib --test components --test apps --test settings -- --include-ignored` -> 20 + 25 + 12 + 8
  passed; shots read: native-install-dialog vs react-install-dialog match, native-Apps-hosts and native-Settings-ca-confirm still fine.
- Open (part B, not this round): native hosts dialog lacks React's boxed "Left alone" warning and the boxed preview.
- comment gate: 19 banned comments left by parts B/C (apps, grid, panel, log_panel, tools, tests) deleted, no code change.
- final round 2b: `task.mjs check` -> build, build-rust, lint, test (Vitest), test-rust, test-native all OK.

## Round 2c: part B/C leftovers (2026-11-19, boss: "lanjut")
Plan: 1) hosts dialog = React HostsDialog: boxed "Left alone" warning, bordered preview, shield line, mono spans, result panel after apply
(unchanged / updated + backup), declined panel (ElevationDeclined, retry allowed), Close instead of Cancel after success.
2) every native dialog gets React's DialogContent chrome: 16px title + × close (`widgets::dialog_title`, replaces the components-only copy).
3) part C leftovers: Codex picker card bg-muted/30 p-2.5; router tile chrome and log filter icon already closed (router panel lives in
the round 2b ServiceCard row; `text_input` overlays the icon).
### Test results (round 2c)
- `cargo test -p zenv-native --test apps --test tools -- --include-ignored` -> 15 + 8 passed (3 new hosts tests: changes/conflicts/preview
  + Update emits hosts:apply; success shows backup and only Close; declined explains and keeps Update)
- PNG pair native-Apps-hosts / react-Apps-hosts-dialog read: boxes, preview, shield line match; × and mono spans added after.
- `cargo test -p zenv-native -- --include-ignored` (after × chrome) -> 20 + 15 + 25 + 14 + 10 + 8 + 8 = 100 passed.
- 4) menu items: `widgets::menu_item` = React DropdownMenuItem (16px muted lucide icon, 14px text, h32, hover fill, destructive tint);
  every native ⋯ menu converted with React's icons (installation, version row, server row, app card/panel); apps grid helpers now wrap it.
- 5) editor split button = React edit-buttons: always shown ("No editor" disabled), the editor's own icon (data URL PNG), menu
  "OPEN IN" + every editor + "default" tag + "No editor found…" item. Studio items wait for step 8.
- bug found on the way (debugging): apps kittests panicked "No glyph ⚠" only when run in parallel; `--test-threads=1` passed ->
  root cause: `icons::program_icon`/`installation_icon` wrote the PNG cache with `fs::write`, a concurrent reader decoded a half-written
  file. Fix: `managed_files::atomic_write` (temp + rename). Image loaders now installed in `theme::install` so tests render like the app.
### Rulings (round 2c)
- Studio button and "Studio: editor, Claude Code, preview" item left out | Studio is step 8, a button to nothing is worse | panel lacks React's Studio row until step 8
- icon cache uses the existing `atomic_write` | the same race exists between the Tauri and native app on first run | none
- 6) hosts banner = React card (p-3, icon 16, text fills, small outline button right): PNG native-Apps-hosts-banner vs react-Apps-hosts-banner match.
- bug (explains round 2's "Unexplained" exit): live shot run, the Tools launch showed "Zenv failed to start: database error: disk I/O error".
  Repro `scratchpad/restart-loop.ps1` (start, 3 s, Stop-Process -Force, start again): 6/12 failed, strictly alternating; with WaitForExit
  between runs 0/8; two live instances side by side fine. Logged error: SqliteFailure SystemIoFailure extended 1546 = SQLITE_IOERR_TRUNCATE:
  the new process truncates the WAL index while Windows still maps it for the dying process. Fix (external timing): `storage::Db::open`
  retries open + configure on SystemIoFailure, 30 x 100 ms, warn log per retry; fatal start errors now also go to zenv.log.
  Tests: 3 new unit tests (retries then succeeds, gives up after budget, no retry for other errors) failed to compile before, pass after
  (`cargo test -p zenv-core --lib storage::` 16 passed); repro after fix 0/12 failed, 8 retries logged.
- live app (release, ZENV_HOME=C:\zenv-dev): all 9 routes open, private 57-59 MB each; live-Tools / live-Logs read: correct pages.
- apps/components after menu labels: `--test apps --test components` 16 + 24 passed (menu_item names itself by its text, like React's aria).
### Rulings (round 2c, continued)
- open retry 3 s total, only for SQLite I/O errors | covers a restart right after a crash or force-kill; real disk faults still fail after 3 s | a broken disk shows its error 3 s later
- the Logs launch that exited silently in the first shot run was not reproduced in 21 later launches | it followed the failed Tools instance | if it returns, zenv.log now records start errors
- final round 2c: `task.mjs check` -> build, build-rust, lint, test (Vitest), test-rust, test-native all OK (after restoring icons.rs CRLF so the diff stays 4 lines)

## Round 3: QA round 1 findings (report `.brain/handoff/13-qa.md`, verdict FAIL 4/3/5; security 0 findings)
Checked against the code: all 12 are real.
Plan: 1) notes from jobs: `loader::Note {tone, title, description}`, `Task::reporting` (job returns notes), `Action::Toast`; toasts get a
tone (success/info/error icon + bar). 2) F1 + F6: Start apps / Stop all outcomes -> React's toasts; success/info toasts where React has them
(install done/failed, Codex, router update, PATH, tailnet open, copy). 3) F2: failed install jobs (< 30 min) listed with their error, toast on
the active -> done/failed transition. 4) F3: thumbnails invalidated on tasks_finished. 5) F5: auto-capture inline, muted "Preview: <error>"
line. 6) F4: `Core::{create_site, update_site, set_site_enabled, delete_site, create_docker_site}` in zenv-core with tests, both shells call
them. 7) F7 MODULES bin name. Nice: F8 finished counter in Shared; F9 Disconnected fetch -> error; F11 router open error toast;
F12 test temp dir cleanup; F10 (UI-thread detect/free_port) ruling.
### Test plan
- unit: start outcomes -> notes (failed each, started joined, all running -> info, none -> "No app needs a service yet"); stop outcomes;
  install transition notes (active->done success, active->failed error, done already seen -> nothing); loader Disconnected -> Err.
- core: create/update/enable/delete site through Core (temp home), moved-root check on update, docker create.
- kittest: QA's `qa_failed_install` passes; toast tones render; Preview error line in panel.
- review focus: a start where every service fails (no success toast), zero needed services, an install failing twice, a capture failing
  for 8 apps (no toasts), a delete while the process runs.
### Test results (round 3)
- F4: `Core::{create_site, create_docker_site, update_site, set_site_enabled, delete_site}` (app_process.rs), Tauri commands and every
  native copy call them (panel's inline save now gets the moved check it lacked). `cargo test -p zenv-core --lib app_process::` -> 8 passed
  (2 new: lifecycle through Core; docker binding kept, bad binding creates nothing). `cargo build -p zenv-native -p zenv` clean.
- notes: `Task::reporting` + `Note`/`Tone`, `Action::Toast`, toasts with React's icon/bar per tone; router-open failures via a channel.
- F1: `dashboard::{start_notes, stop_notes}`: 4 new tests (failures + started joined; all failed -> no success; nothing to do -> info; stop count).
- F2: failed jobs < 30 min listed (`detail::shown_job`, also the install dialog); `app::install_notes` toasts the active -> end transition
  and refreshes pages; new test (first poll silent, done + failed announced once). QA's `qa_failed_install` 2/2 now.
- F3/F5: thumbnails invalidated after tasks; captures inline and one at a time (React's queue), "Preview: <error>" line in the panel (new test).
- F6: Codex (setup/default/model), router update (`tools::update_note`, new test), PATH add/remove, tailnet open, "Connection URL copied".
- F8: entering a route refreshes that page. F9: loader refetches after an in-flight invalidate, a dead fetch becomes an error (2 unit tests).
- F11: router dashboard open failure -> "Couldn't open <name>" toast. F7: MODULES bin name fixed.
- `cargo test -p zenv-native -j 2 -- --include-ignored` -> lib 22, apps 18, components 25, dashboard 19, logs 10, qa_failed_install 2,
  settings 8, tools 8 (+1 new run separately) passed.
### Rulings (round 3)
- "Copy connection URL" now toasts (overrules round 2b's ruling) | React's ServiceCard does | none
- F10 left: `detect_project` / `free_port` stay on the click | local folder reads and a port bind take milliseconds | a project on a slow network drive freezes the window briefly
- F12 left: the apps test keeps its per-run temp core dir | a fixed shared dir would race parallel runs (QA and the lead ran tests at once today) | %TEMP% litter of a few KB per run
- single-service start/stop errors stay inline in the tile, no toast | round 2 inline errors | none
- round 3 final: `task.mjs check` -> all OK (first run hit a linker 'memory exhausted' with 6.2 GB commit left on this PC; rerun clean)
- QA round 2: PASS WITH NOTES (F1-7, 9, 11 confirmed fixed; 109 native tests). Closed after it: F8 Logs > Projects reloads on the refresh
  signal (`project_logs::poll`; new test fails without it: a ready list stays fresh), "Best free port" failure -> "Couldn't find a free port" toast
  (form + program). `--test logs --test apps` -> 10 + 17 passed (+ ignored renders).
