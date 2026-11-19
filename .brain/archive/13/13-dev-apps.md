# 13-dev-apps: Apps page in egui (Part B, task 13)

## Goal
Port `src/features/apps/*` + `src/features/sites/*` (Herd-style grid + detail panel) to
`src-native/src/pages/apps.rs` + `src-native/src/pages/apps/*`, calling `zenv_core` directly
(no Tauri). Out of scope: "Open in Studio" button (left out). In scope: "Open in <editor>".

## Files (all owned, new unless noted)
- `pages/apps.rs` (stub, keep `AppsPage::ui`) — `AppsPage` struct, `Overlay`, Loader
  orchestration, delete-confirm modal, `reveal_path` (Explorer open/select, local Task job).
  Fields are `pub` (and `apps::form`/`apps::docker_picker` are `pub mod`) so `tests/apps.rs`
  can seed `Loader::ready(...)` fixtures and inspect `Overlay`/`FormState` directly — the
  same role dashboard's public `Data<'a>` plays, since this page mixes fetch+render instead
  of splitting into a pure `view()`. Every create/update/enable/duplicate/delete/hosts-apply
  mutation goes through `Action::Run(Task::new(key, ...))` (never a typed one-shot): errors
  surface as the existing toast, and `page.pending` tracks the in-flight key so the open
  dialog closes once `cx.busy` drops it (mirrors dashboard's service-toggle pattern).
- `pages/apps/support.rs` — pure helpers (labels, slug, resolveBackend-equivalent,
  relative_doc_root, `to_input`) + `support::name` (gives a ComboBox/empty-text Checkbox an
  accessible name; `WidgetInfo::labeled` is the only way egui/kittest name those two)
- `pages/apps/grid.rs` — search/pagination/empty states + card grid + context menu
- `pages/apps/panel.rs` — right detail panel (Status/Serving/Program-or-Environment/HTTPS/
  Tailscale/Paths groups, "…" menu, header)
- `pages/apps/program.rs` — AppProgram + AppContainer (docker) process widget
- `pages/apps/tailnet.rs` — TailnetPanel port
- `pages/apps/form.rs` — create/edit dialog (folder or docker source, 3-step create / 2-step edit)
- `pages/apps/docker_picker.rs` — Docker container picker (used by form.rs)
- `pages/apps/hosts_dialog.rs` — hosts-file plan + apply
- `pages/apps/details_dialog.rs` — diagnose checks + error/access/process log tail
- `tests/apps.rs` — kittest: grid renders, select shows panel, create/enable/delete emit the
  right Task key, empty/error states, menu actions

## Core calls used (read with Loader, mutate with Task)
`c.sites.{list,view,create,update,set_enabled,duplicate,delete,diagnose,log_sources,read_log,
hosts_status,hosts_apply}`, `c.detect_project`, `c.docker_containers`, `c.create_docker_app`,
`c.sync_app_process`, `c.app_process_status`, `c.start_app_process_by_user`, `c.stop_app_process`,
`c.set_app_command`, `zenv_core::app_process::{preferred_port,free_port}`, `c.tailnet_status`,
`c.tailnet_apps`, `c.tailnet_open`, `c.tailnet_close`, `c.editors`, `c.open_in_editor`,
`c.tls.status`, `runtime_manager::list`, `zenv_core::docker_apps::ensure_engine`.
Reveal (open folder / show config in Explorer) and "open in editor"/"open browser" run as
local Task jobs (no new `pages::Action` variant needed — owned by the lead).

## Test plan
`cargo test -p zenv-native --test apps`: a real (throwaway temp-dir) `Core` backs `Cx` so the
page's own `Loader::refresh_every` calls type-check; every seeded Loader uses `Loader::ready`
(not stale) so no background fetch actually runs against it, and no `Action::Run` closure is
ever invoked (only `.key` is inspected), so no test depends on the real DB having the fixture
sites. Covers: grid + panel render for the selected app, selecting another card switches the
panel, the enable switch / delete confirm / create-form submit each emit the expected
`site:<id>:enable` / `site:<id>:delete` / `site:create` Task key, empty states (no web server,
no apps yet), and the sites-list error state. Unit tests (`support.rs`, run via `--lib`) cover
slug, service-name validation, relative_doc_root, and resolve_backend's four branches.
`cargo build -p zenv-native` with no warnings from these files.

## Results
- `cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native`: clean, 0 warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native --test apps`: 8/8 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native --lib`: 16/16 passed
  (includes the 8 new `pages::apps::support::tests::*`).
- Ruling (superseded in round 2, see below): mutations use `Action::Run(Task)` + toast errors
  everywhere, including the create/edit form and the inline panel edits, instead of a typed
  one-shot that would show the core's validation error inline in the dialog. Round 2 switched
  the ones React shows inline to `.inline()` once that API existed; see below.
- Ruling: "Best free port" and the create wizard's step-1 "Continue" (`detect_project`) run
  synchronously on the UI thread on click, not through a Task. Why: both are quick local reads
  (port probing, marker-file scan) and the wizard needs the typed result back immediately to
  populate the next step, which `Task`'s `AppResult<()>` can't carry. Cost if wrong: a slow
  disk/port probe would stall the UI for that one click; no reports of this being slow in the
  React version either.
- Not covered by the kittest suite: the docker-source wizard path and
  `DockerContainerPicker` (needs a real/started Docker Desktop), the Tailscale open/close flow
  (shells out to the `tailscale` CLI), and thumbnail capture (spawns a real headless browser).
  These are exercised by hand only; their UI code paths are typed and compiled, not behavior-tested.

## Round 2 (visual match to React)

Fixed the base mock crash first: `hosts_status` in the shared `mock-ipc.js` is `[]`
(array), so `hosts.data?.plan.changed` threw ("Cannot read properties of undefined
(reading 'changed')") in `AppsPage.tsx`. Own override in `S/mock-apps.js` (and
`mock-apps-empty.js` / `mock-apps-hosts.js` for the other states) gives it the real
`{path, plan}` shape, plus `tls_status`, `editors_list`, `docker_containers`,
`site_diagnose`, `site_log_read`, `site_detect`, `app_port_suggest` and a rejecting
`site_thumbnail_capture` (no headless browser in either mock, so both UIs fall back to
the placeholder tile instead of one showing a broken `<img>`).

Shot pairs saved to `src-tauri/target/native-shots/` (`react-Apps-*.png` /
`native-Apps-*.png`): grid+panel (shop.test selected), empty (no apps), hosts banner+dialog
(changed plan with add/remove/conflicts), wizard step 1 (folder source), step 2 and step 3
(docker source, since clicks can't type into the folder path field), diagnose dialog, delete
confirm. All match closely: page header, card grid (image placeholder, domain/PHP
version/Open), the right panel's every section (Status/Serving/Environment/HTTPS/Tailscale/
Paths), the hosts and delete dialogs, and the 3-step wizard.

Two real bugs found and fixed via the screenshots, not just cosmetics:
1. **Grid cards were badly mis-laid-out** (thumbnail collapsed to almost nothing, domain/Open
   row drawn inside it instead of below). Root cause: `thumbnail()` in `grid.rs` finishes with
   several `ui.put(...)` calls (image, spinner, refresh button) for absolute-positioned
   decoration inside the already-reserved 100px rect; `Ui::put`/`scope_builder` each
   re-advance the outer cursor to *their own* small rect, which overrides the original
   `allocate_exact_size(100px)` reservation and can leave the cursor short. Fixed with one
   `ui.advance_cursor_after_rect(rect)` at the end of `thumbnail()` to restore the full
   reservation. Confirmed via a one-off geometry probe (`node.rect()` on the queried labels)
   before and after.
2. **Grid column count didn't match the Tailwind grid.** `widgets::columns_for` (shared,
   gap-less `floor(width/min)`) gave 4 narrow columns for 3 apps where React's
   `auto-fill minmax(210px,1fr) gap-3` gives 3 full-width ones (its track-fitting formula is
   gap-aware: `floor((width+gap)/(min+gap))`). Added a local `grid_cols()` in `grid.rs`
   (owned file) instead of changing the shared `columns_for` (also used by `settings.rs`),
   and set `item_spacing` to 12px (`gap-3`) before `ui.columns()`.

Also: three `widgets::mono(...)` strings in `hosts_dialog.rs`/`program.rs`/`form.rs` used a
literal "→"; the bundled Monospace/Proportional fonts have no glyph for it and `egui_kittest`
panics (`No glyph for "→"`) instead of silently showing tofu. Switched them to ASCII "->"
(JetBrains Mono's ligature renders it as an arrow anyway, confirmed in the hosts screenshot).
Two `TextEdit`s in the step-2 form (`Name`, `Domain`) and three more (`Project folder` in
edit mode, `App port`, `Document root`) had no accessible name, caught by
`with_accessibility_check` (on by default) once the wizard's step-2 render test ran; gave
each a `support::name(..., Role::TextInput, ...)` to go with their visible `ui.label`.

Per the coordinator's note mid-task, switched the mutations React shows inline
(`ErrorPanel`) to `Task::new(...).inline()` + `cx.shared.errors`: the panel's save/enable
(`site:{id}:save` / `:enable`, shown under the Paths section), the create/edit/docker-create
form (`site:create` / `site:{id}:save`, shown above the footer — the dialog now also only
auto-closes on success, not on a failed create/save), the delete confirm
(`site:{id}:delete`, shown below the grid once the dialog closes, matching
`{remove.error && <ErrorPanel .../>}`), and hosts-apply (`hosts:apply`, dialog no longer
auto-closes at all — it never did on success in React either, since no `Result` success
panel is modeled here). Added one `destructive_button()` in `apps.rs` for the delete
confirm's red "Remove app" (no destructive `Kind` in the shared `button_sized` yet; named
here per the brief instead of editing `widgets.rs`).

### Round 2 results
- `cargo build --manifest-path src-tauri/Cargo.toml -p zenv-native`: clean, 0 warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native --test apps`: 12/12 passed
  (8 original + 3 new inline-error tests + 1 `#[ignore]` PNG render test, run with
  `-- --include-ignored`).
- `cargo test --manifest-path src-tauri/Cargo.toml -p zenv-native --lib`: 19/19 passed.
- Not re-verified pixel-for-pixel: the docker wizard's container list styling (needs Docker
  Desktop to shoot the React side with real containers; mocked instead) and the Tailscale
  panel's open/closed states (no site in either fixture has it open). Both are typed/compiled,
  not screenshot-matched, same as round 1.

## Round 2b (lead's pixel review of grid.png)

Adopted the new shared widgets in all owned files (deleted local duplicates):
`widgets::toggle` for the plain switches (Serve this app, HTTPS, Redirect HTTP to HTTPS in
both `panel.rs` and `form.rs` — `status_switch`/`SwitchState` no longer used here, that stays
for dashboard's ON/OFF-labelled switches), `widgets::badge` for the status pill (`Active` etc,
`panel.rs`'s `status_badge` now returns `(&str, Badge)`), `widgets::text_input` for the search
box (256px, search icon, `apps.rs`), `widgets::select` for every `ComboBox` in `panel.rs` and
`form.rs` (Web server/PHP version/Runs on/Project type).

Fixed: (1) test-fixture bug, not production — `php_site()` set every site's `name` to its `id`
("s1","s2","s3", all monogram "S"); now derived from the domain so shop/blog/api show S/B/A.
(2) thumbnail height now `width / 1.6` (React's `aspect-[16/10]`) instead of a flat 100px, and
the refresh icon only paints when `ui.rect_contains_pointer(rect)` (was unconditional). (3)
`apps.rs`'s grid/panel split now uses the exact `352px panel + 20px gap` React uses (`22rem` /
`gap-5`) instead of ad-hoc 340/12, so grid+gap+panel sums to the available width with no
slack. (4) added two local helpers in `panel.rs` (`outline_icon_button` for the folder button,
`ghost_menu_button` for the "…" menu and the editor split button's chevron) since
`widgets.rs` has no destructive/outline-icon/ghost-menu variants yet.

Re-rendered and re-compared all 8 pairs; grid/panel, wizard and delete now match closely
(monograms, toggle switches, select chevrons, Active badge, bordered folder icon). Build
clean, 0 warnings; `--test apps -- --include-ignored`: 12/12 (transient
"paging file too small" on the first attempt while another worker's concurrent build was
running — passed clean on retry, not a real failure).

Not done (time-boxed): "Open in browser" isn't `flex-1` (still auto-sized by text); the
editor row is an approximation of React's split button (outline button + separate ghost
chevron opening a menu of other editors, not one fused control).

## Round 2c (panel overflow + stretch/groups/split button)

Root cause of the overflow: `ui.horizontal_top` already adds `item_spacing.x` between its two
`ui.vertical()` children; an extra `ui.add_space(20)` between them was its OWN item, picking up
that spacing on both sides (8+20+8=36 vs the intended 20) — 16px over. Fixed by setting
`item_spacing.x = 20.0` once and dropping the `add_space`. Also added: `stretch_primary_button`
(flex-1 "Open in browser"), local `group()` (border-t above + mono title, no trailing hairline,
replacing `widgets::section` here only), `editor_split_button` (joined outline+chevron, one
`egui::Button` each with half corner-radius). Build clean 0 warnings; `--test apps
--include-ignored` 12/12; `--lib` 19/19.
