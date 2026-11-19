# Task 13 (M20, D-021) — Part C: Tools / Logs / Settings pages (worker: dev-tools)

## Goal
Port three React pages to egui, reusing the dashboard.rs Loader/Task/Action pattern:
Tools (AI routers OmniRoute/9router + Codex), Logs (Projects/Services/Diagnostics/Port
inspector tabs), Settings (About, theme, HTTPS CA, storage locations).

## Owned files (only these touched)
- `src-native/src/pages/tools.rs`, `src-native/src/pages/tools/codex_row.rs`
- `src-native/src/pages/logs.rs`, `src-native/src/pages/logs/{log_panel,project_logs,service_logs,diagnostics}.rs`
- `src-native/src/pages/settings.rs`, `src-native/src/pages/settings/{https,theme_picker}.rs`
- `src-native/tests/{tools,logs,settings}.rs`

## Steps
1. Read the reference pattern (dashboard.rs, mod.rs, loader.rs, widgets.rs, shell.rs,
   tests/dashboard.rs) and the React sources + their Tauri commands/core functions.
2. Tools: per router (Omniroute, Ninerouter) render any existing service as a tile
   (reusing `dashboard::service_key/led_of/port_text`), plus a router panel: open
   dashboard (Action::OpenRouter), copy base URL, update/auto-update row, Codex row
   (add/update, default switch, model picker with live router model list, show/copy
   config.toml snippet). Empty state ("<Name> has no service yet.") when the component
   has no service.
3. Logs: tab bar (Projects/Services/Diagnostics/Port inspector) over a shared
   `log_panel::show` (source tabs with sizes, Follow, filter, Open file via a
   `file:///` OpenUrl, stick-to-bottom ScrollArea). Projects lists sites (search +
   SiteQuery), shows a failed-server banner with a jump-to-service-log button.
   Services lists Zenv log + services by category + PHP runtime groups. Diagnostics
   runs `zenv_core::diagnostics::run`; Port inspector calls `port_manager::inspect`
   on demand.
4. Settings: About (version/schema/platform from `Core::info()`), theme picker using
   the same `ctx.set_theme`/`ThemePreference` as shell.rs, HTTPS card (CaStatus,
   create/trust/untrust with a confirm Modal), storage locations with Open buttons
   (file:// OpenUrl) for the two rows React makes openable.
5. Wrote kittest integration tests per page; fixed issues found while running them
   (see Test results).

## Ruling
- `Action::OpenRouter` (mod.rs/app.rs, not mine) only opens a router's home page, no
  per-page parameter. React's RouterPanel also deep-links into specific dashboard
  pages (Providers, Usage, …); that is not reachable from Tools without a contract
  change I don't own. Scoped Tools down to "Open dashboard" (home) + copy URL +
  update + Codex, and reported it instead of guessing at a new Action variant.
  Cost if wrong: a boss who wants per-page shortcuts asks for a mod.rs change later.
- No Tauri `reveal`/`opener` available outside Tauri; "Open file"/"Open" buttons use
  `Action::OpenUrl("file:///...")` (already-loaded paths), which opens via the OS
  default handler through the same `webbrowser::open` path egui-winit already uses
  for OpenUrl. No existence check or error surfaced on failure (OpenUrl is fire-and-
  forget); acceptable since the paths used are either core-guaranteed (AppPaths::ensure)
  or taken from an already-loaded LogSource/LogTail.
- Logs' list/detail split uses `ui.columns(2, ...)`, not `ui.horizontal` wrapping
  `ui.vertical` holding a `ScrollArea`: that combination silently drops click
  hit-testing on rows inside the ScrollArea under `egui_kittest` 0.37 (reproduced in
  isolation; `ui.columns` with the same ScrollArea does not). Cost if wrong: a real
  click failing the same way in the live app — low risk since `ui.columns` renders
  identically otherwise, and QA can re-check the Logs tabs in the running app.

## Test plan
kittest harness per page, calling the pure `view`/`show` functions directly (never
the real `ui()`, so no Core/filesystem access in tests), with `Loader::ready(...)`
fixtures: empty/loading/error states, a click producing the right `Action::Run` key
or `Action::OpenRouter`/`Action::OpenUrl`, and the Codex model picker's open/pick/save
flow.

## Test results
`node scripts/with-toolchain.mjs cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native --test tools --test logs --test settings`
→ tools: 9 passed; logs: 9 passed; settings: 7 passed (25/25).
`node scripts/with-toolchain.mjs cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native`
→ builds with 0 warnings (forced a touch-rebuild of my three files to confirm no
warnings were cached away).

## Not checked
- Not run inside the live app (not asked to; "do not launch the app").
- No automated test for the model picker's numeric context/output filtering beyond
  the inline unit tests in `codex_row.rs` (`cargo test -p zenv-native` default target,
  outside the `--test tools --test logs --test settings` filter).
- Accessibility pass is the kittest default check (`with_accessibility_check`) only;
  no screen-reader manual check.

## Round 2: visual match to React (D-022)

### Plan
Shoot the React reference with a new `S/mock-tools.js` (OmniRoute running + 9router
stopped, router_status/codex_status/router_models per component, sites + site/service
log sources and tails, 3-state diagnostics, tls_status, app_info) via `react-shot.cjs`;
add `#[ignore]` render tests per page calling `shell::frame` with matching fixtures;
compare the PNG pairs and fix layout/spacing/sizing in owned files only; keep every
existing test green.

### Ruling
- React's "Tools" nav route is `ComponentsPage(category="tool")` (catalog browser:
  left component nav, install/register dialogs, `ServiceCard` with a dropdown menu and
  version badges) — a different, much bigger page than `pages::tools`. MODULES.md's
  M20 order already splits this on purpose ("4 runtimes, installers" vs "5 tools (AI
  routers, …)"), and `tests/components.rs` (not mine) renders that catalog page for
  Languages/Databases/Cache/WebServers but never Tools — confirming `Route::Tools` is
  `pages::tools` alone. Matched the part that *is* shared ground truth (React's
  `RouterPanel`/`CodexModelPicker`: shortcut grid, button sizes, switch/label order,
  codex row) instead of rebuilding a catalog browser I don't own. Cost if wrong: the
  boss asks for the router tile's switch/menu chrome to also match `ServiceCard`
  (dropdown, version badges) — a `components`-side change, not mine.
- Added a local segmented-pill control in `theme_picker.rs` and a separate one in
  `log_panel.rs` (source tabs) rather than a new `widgets::segmented`, to avoid
  touching the shared `widgets.rs` while other round-2 workers build in parallel.
  Cost if wrong: minor duplication the boss can ask to hoist into `widgets.rs` later.

### Test plan
Per MATCH-BRIEF: React shots via `S/mock-tools.js` + `react-shot.cjs` (nav label,
clicks); native shots via new `#[ignore] render_*_png` tests in each test file, using
`shell::frame` with fixture data mirroring the mock, saved to `native-shots/`; read
both PNGs and fix spacing/sizing/order; re-run every existing test after each fix.

### Test results
PNG pairs matched (native-shots/): `react-Tools-omniroute` / `native-Tools-both`
(both routers, shortcut rows), `react-Tools-empty` / `native-Tools-empty`,
`react-Logs-projects` / `native-Logs-projects` (Projects tab, blog.test selected),
`react-Logs-services` / `native-Logs-services` (Services tab, MySQL log open),
`react-Logs-diagnostics` / `native-Logs-diagnostics`, `react-Settings-ca-missing` /
`native-Settings-ca-missing`, `react-Settings-ca-trusted` / `native-Settings-ca-trusted`,
`react-Settings-ca-confirm` / `native-Settings-ca-confirm` (CA confirm modal).

Fixes made: Tools shortcut groups now a 2-column grid (was 4 stacked rows); every
`RouterPanel`/`CodexModelPicker`-equivalent button switched from default to `button_sm`
(React's `size="sm"`); "Default for Codex" now reads label-then-switch (matched
`<label>text<Switch/></label>`); Logs gained the missing "Logs" title + an
underline tab bar (`logs::tab_bar`, was unstyled filled buttons, and was only drawn by
`LogsPage::ui()` so the old render tests skipped it — fixed by drawing it in the render
test's `shell_and_page` too); the log viewer and both sidebars are now wrapped in a
`card` (React's `<Card>`); log source tabs are now a segmented pill bar instead of
plain `selectable_label`s; Settings' theme picker is now an icon+label segmented
control (icons were missing, and raw array order rendered reversed once wrapped in
`right_to_left` — fixed by listing options already reversed); HTTPS card gained the
boxed icon, a "Trusted/Not trusted yet/Not created" pill badge, `Shield`'s warning
color (was muted), the full description sentence, a 2-column `Grid` for
Subject/SHA-256/Valid until/Certificate, and `button_sm` on its own buttons (the
Confirm/Cancel modal buttons stay default-sized, matching `ConfirmDialog`'s).

`node scripts/with-toolchain.mjs cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2 --test tools --test logs --test settings -- --include-ignored`
→ tools: 11 passed; logs: 10 passed; settings: 8 passed (29/29, incl. the 3 new
`render_*_png` tests).
`node scripts/with-toolchain.mjs cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native -j 2`
→ 0 warnings (forced a touch-rebuild of every owned file first).

### Not checked (round 2)
- Still not run in the live app.
- Left as acceptable gaps, out of scope (not owned, not asked): the router tile's
  top row (switch position, no dropdown menu, no version/zenv badges) vs React's
  `ServiceCard` chrome; the log filter's search icon sits left of the input instead of
  overlaid inside it; the Codex model-picker card uses the plain `card` background
  instead of React's `bg-muted/30`.
