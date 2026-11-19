# 13 M20 native egui UI — Part A: components (Languages / Web Servers / Databases / Cache)

## Plan
Goal: port ComponentsPage (React, `src/features/runtimes/*.tsx` + `src/features/web-servers/*.tsx`) to egui, one shared layout
(component sidebar | servers + installed versions) driving Route::Languages / WebServers / Databases / Cache.

Steps:
1. Read reference pattern (dashboard.rs, pages/mod.rs, loader.rs, widgets.rs, theme.rs, lucide.rs, tests/dashboard.rs) and the
   React source + the backend fns in commands.rs / zenv_core (runtime_catalog, runtime_manager, runtime_installer, service_manager,
   port_manager, app_services, icons, path_env).
2. `ComponentsPage` (components.rs): owns Loaders for installations/runtimes/jobs/discover + per-component icon Loaders + a
   per-route `picked: HashMap<Route, ComponentId>` + one `Dialog` enum (install/register/service-form). Resolves
   installations/runtimes/jobs once per frame, builds a `Data` bundle, delegates to a pure `view()` fn (same split as
   dashboard.rs: stateful glue vs. testable pure render).
3. `components/sidebar.rs`: component list with logo (data-url PNG via `zenv_core::icons::component_icon`), running dot,
   installed version chips, click to pick.
4. `components/detail.rs`: header (logo, name, unofficial-build badge, distribution, Install version / Add from folder),
   active install progress rows, then either `server_version_list` (WebServer/Database/Cache: installed version + its
   server(s), start/stop switch reusing `dashboard::service_key`/`led_of`/`port_text` for busy-key consistency, restart/
   reload/delete/edit menu) or `component_card` (Language: installed versions, default/source/missing badges, php-cgi/
   frankenphp runtime badge, make-default/refresh/open-folder/uninstall menu).
5. `components/dialogs.rs`: `Dialog` enum + 3 modal dialogs (egui::Modal) — Install (available versions, pre-release
   toggle, per-version Install button -> Task), Register (folder/exe path via `rfd`, verify+register -> Task), Service
   form (add/edit server: name, installation combo, listen address, http/https port, data folder via `rfd` when needed,
   autostart -> Task).
6. `components/discovery.rs`: "Scan this computer" panel (Loader-driven, lazy — only fetches once clicked), register
   button per unregistered discovery result.
7. `src-native/tests/components.rs`: kittest + unit tests.

Files (all owned, none outside the list):
- src-native/src/pages/components.rs (rewritten; stub replaced)
- src-native/src/pages/components/{sidebar,detail,dialogs,discovery}.rs (new)
- src-native/tests/components.rs (new)
- .brain/handoff/13-dev-components.md (this file)

## Test plan
- kittest (`tests/components.rs`): a WebServer component with an installed version and a running server renders the
  sidebar entry, "VERSIONS & SERVERS · N", the server row and its port text; clicking the server's switch emits
  `Action::Run` with the same key format dashboard uses (`service:<id>`); opening the Install dialog and feeding it an
  `AvailableVersion` via a directly-constructed `Data` (same no-network pattern as dashboard's tests) shows the version
  and clicking its Install button emits `installer:install:<component>:<version>`; a component with zero installations
  shows the "not installed yet" empty state; the discovery panel's error and empty states render their messages.
- unit: `category_of`/`title_of` route mapping, `current_component` fallback (picked -> installed -> first),
  `default_name_port` / `needs_data_dir` (dialogs.rs, used to default the service-form fields).
- `cargo build -p zenv-native`: no errors/warnings from any file under `src-native/src/pages/components*`.

## Test results
- `cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native` (3 runs, ~10 min apart): zero errors or warnings
  attributed to `pages/components.rs` or `pages/components/*.rs` in all 3 runs. The crate as a whole still fails to
  build, every time, with the same 3 pre-existing errors in files owned by the parallel "apps" worker (not mine):
  `pages/apps.rs:3` (`mod form;` — `apps/form.rs` does not exist yet), `pages/apps/panel.rs:22` and
  `pages/apps/program.rs:98` (`support::to_input` not found). Those files are mid-edit by that worker; I fixed only the
  two issues that were in my own files (a missing lifetime on `dialogs::component_info`, an unused import) — both are
  gone from the error list after the fix, confirmed by re-running the build twice more.
- Could NOT run `cargo test -p zenv-native --test components` or get a pass count: the whole crate must link first, and
  it cannot while `apps.rs`/`apps/panel.rs`/`apps/program.rs` (another worker's in-progress files) don't compile. This
  is a shared-crate blocker, not a defect in the files I own.
- Traced through every test in `tests/components.rs` by hand against the actual code paths (widget labels, Task key
  formats, the `Data`/`Loader` plumbing) to the best of static review; this is NOT the same as a green test run and is
  called out as unverified below.

## Follow-up: PATH controls + open terminal
Added per the coordinator's request, same owned files: `path_env::{PathStatus, PathConflict}` via a
`path_status: HashMap<ComponentId, Loader<PathStatus>>` refreshed per visible component (`core.path_status`); a
`path_conflicts_panel` (shown once per component, above the version list) and a `path_button` (Add to PATH / PATH badge
+ remove, disabled+tooltip when blocked) wired into `component_card`, `version_row` and `server_row` via
`core.path_add`/`core.path_remove`. "Open terminal with this version" added to `version_menu` (Language only, matching
React's `ComponentCard`) via `core.open_terminal(&[id], None)`.
Checked `commands::reveal`: it only knows how to open a path via `tauri_plugin_opener` (needs a Tauri `AppHandle`,
which this native app doesn't have) — no reusable `zenv_core`/`platform` fn exists for it, so `open_folder_task`'s
`std::process::Command::new("explorer")` stays as the only option, confirmed by re-reading `commands.rs`.

## Test results (after the fix + the follow-up)
- `cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2`: clean, 0 errors/warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2 --test components`: **9 passed, 0 failed**
  (added `open_terminal_menu_item_emits_the_terminal_task_key`, `path_button_emits_add_to_path_task`,
  `path_conflicts_panel_renders_the_blocking_entries`). One iteration: the conflicts panel's "→" glyph isn't in the
  loaded font set under kittest (panics on missing glyph) — swapped for plain text.

## Rulings
- "Open folder" (installation menu) and the discovery scan have no Tauri `opener`/AppHandle in this app; used
  `std::process::Command::new("explorer")` directly (no new dependency) instead of inventing a core fn | smallest
  change, matches existing platform-specific code style | if wrong, swap for a real `zenv_core::platform` helper later.
- Reused `dashboard::service_key` / `led_of` / `port_text` / `compare_versions_desc` (all `pub fn`) instead of
  duplicating them, so the start/stop busy-key format stays identical between Dashboard and this page | consistency,
  read-only use of another owned file's public API | none.
- Skipped the PATH button/conflicts panel (`path-controls.tsx`) and the "open terminal with this version" menu item —
  budget; neither blocks start/stop/install/register/service CRUD | lower priority than the primary flows | add as a
  follow-up task.
- Service-form "Save"/"Add server" closes the dialog immediately after queuing the Task (optimistic); Register dialog
  stays open until Cancel so its "Verifying…" busy state is visible | matches "errors become toasts" from the plan,
  simplest per-dialog behavior | if the boss wants symmetric behavior, trivial follow-up.

## Round 2: match the React UI pixel-for-pixel (MATCH-BRIEF)

### What was wrong (found by shooting both UIs and reading the PNGs side by side)
- **Sidebar flowed sideways, not down.** `ui.allocate_ui(230, ...)` inside the page's `ui.horizontal_top` inherits the
  parent's left-to-right layout, so every component card in `sidebar::render`'s loop was placed next to the previous
  one instead of below it — the whole left column rendered as a single wrapping row. Fixed by wrapping each pane
  (sidebar AND detail) in its own `ui.vertical(...)` before the frame, exactly like `detail::render`'s right pane
  already did (that's why only the broken side was visible).
- **No monogram/icon.** `sidebar::logo()` predates `widgets::component_logo` (added for round 1's shared-widget pass)
  and just painted an empty rounded rect — every "php"/"go"/"my" logo, including the big one in the detail header
  (same fn), was a blank box. Now delegates to `widgets::component_logo(ui, id, installed, icon_bytes, size)`.
- **No outer `Card` around each pane.** React wraps the whole nav list and the whole detail pane each in one bordered
  `<Card>` (`p-2`/`p-5`, 17rem + gap-5, no separator line); the page only had per-item cards and a manual
  `ui.separator()`. Replaced with two framed, scrollable panes (`SIDEBAR_WIDTH = 272.0`, matching `17rem`).
- **Sidebar nav rows had a border on every item.** React's inactive rows are `border-transparent` (invisible) with a
  primary-tinted border + 7% fill only when active; the native version drew `widgets::card`'s solid border on all of
  them. Rewrote with a custom frame that's fully transparent unless active.
- **Badges were plain colored text, not pills.** React's `Badge` is a bordered, tinted pill (`rounded-sm border px-1.5
  bg-<color>/10`); `source_badge`/`warning_badge`/discovery's version badges were `RichText` with just a color. Added
  `components::badge(ui, text, BadgeKind)` (no shared `widgets::badge` exists yet — named below) reproducing all 5
  variants, and fixed the badge *order* to match React's `InstallationRow` (default, then source, then missing/off).
  Version numbers in `component_card`/`version_row` are bold mono (`font-semibold`) like React; `server_row`'s is
  deliberately plain, matching `ServiceCard`.
- **`server_row` didn't match `ServiceCard` at all.** It used a plain `led` dot instead of the start/stop switch, and
  put the switch on the right next to the port. Rewrote to React's actual layout: switch on the left, then a
  two-line block (name + version + badges on top, `listenAddress:port[ · data_dir | · httpsPort]` below), menu/
  open-in-browser on the right. Added `address_text()` so DB/cache rows show the listen address and data folder, not
  just the bare port.
- **"Add from folder" used the wrong icon** (`FOLDER_OPEN` instead of React's `PlusIcon`).
- **Dialogs**: added `dialog_title()` (title + top-right `X`, matching every React `DialogContent`'s built-in close)
  used by all three dialogs; removed the install dialog's extra bottom "Close" button (React has none — only the X)
  and renamed its title from "Install {name}" to the real "Install a version"; the register dialog's title is static
  ("Register an existing installation", not per-component) and now has the `Component` picker React's
  `NativeSelect` provides (the `RegisterState.component` field already existed, just wasn't exposed as a combo box).

### New: `install_indicator`
Added `pub fn install_indicator(ui: &mut egui::Ui, jobs: &[InstallJob]) -> Option<ComponentId>` in `components.rs`,
reproducing `install-progress.tsx`'s `InstallIndicator`: filters to active jobs, builds the same label ("Installing
{name} {version}[ · pct%]" for one, "Installing N versions" for several), a tooltip listing every active job's
phase, and returns the clicked job's component so the lead can navigate there from the header. Drawn as a small
bordered chip (`panel_raised`/`panel_line`, matching the header's other chips) with an `egui::Spinner`; renders
nothing when no job is active, same as React returning `null`. 4 new tests cover: absent-without-jobs, the one-job
label with percent, the multi-job count, and the click returning the right `ComponentId`.

### New shared API used (per the coordinator's mid-task note)
`Task::new(...).inline()` + `cx.shared.errors` now back the row-level mutations that React shows inline rather than
as a toast: server start/stop/restart/reload (`service_error()` in `detail.rs`, checked in `server_row`) and the
installation menu's make-default/refresh/terminal/reveal/uninstall/unregister/enable (`installation_error()`,
checked in `component_card` and `version_row`), plus the register dialog's own register task. `Data` gained an
`errors: &HashMap<String, String>` field threaded from `cx.shared.errors`; `tests/components.rs`'s `Fixture`/`page()`
got a matching field. 2 new tests (`inline_service_error_...`, `inline_installation_error_...`) prove the error
shows under the right row.

### Screenshot pairs (native-shots/)
`react-Languages-php.png` / `native-Languages-php.png`, `react-Databases-mysql.png` / `native-Databases-mysql.png`,
`react-Cache-empty.png` / `native-Cache-empty.png`, `react-WebServers.png` (round-1 reference, re-checked against the
same fixes), `react-install-dialog.png` / `native-install-dialog.png`, `react-register-dialog.png` /
`native-register-dialog.png`. All six read as the same layout after the fixes above: sidebar column, logos, badge
pills/order, header buttons+icons, server row (switch/name/port), and dialog chrome all match. Mock file:
`S/mock-components.js` (adds `installer_available`; the base `S/mock-ipc.js` already had PHP/MySQL with 2 versions
each and a failed MySQL service).

### Still different (budget, not structural)
- `version_row`'s card border is solid; React's unattached-version row (`ServerVersionList`'s `VersionRow`) is
  dashed (`border-dashed bg-card/60`) like `widgets::empty_card`. Cosmetic only.
- FrankenPHP's enable/disable is an inline switch in `version_row`; React puts it as a "Switch on/off" item inside
  the installation's "⋯" menu (`useInstallationMenu({switchable})`). Only affects FrankenPHP's own row.
- `InstallDialog`'s "releases from `<source>` / Checked by: `<verified_by>`" line and the pre-release checkbox's
  right-aligned position aren't reproduced; the version list and its badges/Install buttons are.
- PATH button hover state (`hover:bg-accent` on sidebar nav rows) isn't painted — needs a two-pass hover read that
  wasn't worth it for a static-screenshot match; the active/inactive states that the brief's screenshots needed are
  correct.

### Test results
`cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2`: clean, 0 errors/warnings (one retry needed
mid-session while two parallel workers' in-progress files — `pages/settings/https.rs`, `pages/logs/service_logs.rs`
— didn't compile; both are outside my owned files and resolved on their own).
`cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2 --test components -- --include-ignored`:
**16 passed, 0 failed** (9 round-1 + 2 inline-error + 4 install_indicator + 1 `render_components_png`, which writes
the 5 native-*.png files above).
