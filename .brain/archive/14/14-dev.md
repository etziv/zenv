# 14 M20 step 7: native tray and windows
## Plan
Goal: the native (egui) Zenv behaves like the Tauri one around its window: closing keeps Zenv and its services running in the
notification area; the tray icon has the service panel (same layout as React `TrayPanel`, D-022) on left click, Open/Quit menu on
right click, double click opens the window, tooltip lists running services; a second launch shows the running window instead of
starting another Zenv; the window has Zenv's icon.
Steps:
1) core platform: `single_instance(name)` (named event: first owner gets it, a second launch signals it and gets None, with
   AllowSetForegroundWindow), `InstanceEvent::wait`, `work_area(x, y)`, `find_window(title)`; Windows + fallback; unit tests.
2) `src-native/src/tray.rs`: icon + menu, events -> channel + repaint; pure `tooltip(statuses)` and `place(icon, work, size)`; tests.
3) `src-native/src/panel.rs`: the panel UI (header, Running/Stopped groups with switches, error box, Start apps / Stop all,
   Open Zenv / Quit) as an immediate viewport; kittest of its content.
4) `app.rs`: `App::logic` handles close -> hide, tray/menu/instance messages, Quit; panel open/close (focus loss, Escape).
   `main.rs`: single instance before Core starts; window icon.
Files: src-tauri/core/src/platform/{windows,fallback,mod?}.rs, src-native/{Cargo.toml,src/main.rs,app.rs,lib.rs,tray.rs,panel.rs},
src-native/tests/tray.rs.
Risks: eframe does not run `ui` while the root is hidden (use `logic` + RequestPaintWhileHidden); multi-viewport on glow;
Windows foreground lock when a second launch asks the first to show.

## Test plan
- unit: single_instance first Some / second None / signal wakes wait; tooltip 0 / <=3 / >3 running; place: bottom taskbar
  (above icon, clamped to work area), top taskbar, left/right taskbar, icon near the screen edge.
- kittest panel: empty list text; running + stopped groups with counts; "Stop all" disabled with nothing running; "Start apps"
  disabled with no services; header "N running" / "all stopped"; error box shows the newest error and dismisses.
- review focus: close while a task runs (window hides, task finishes, toast waits); Quit from the tray while the window is hidden
  (Core.shutdown runs, process exits); second launch while the window is hidden (window shows and takes focus); tray click right
  after the panel lost focus (stays closed, no flicker reopen); a monitor with the taskbar on top/left.
- measurements: idle private memory with the window hidden <= the 150 MB gate (M20).
- device: ZENV_HOME=C:\zenv-dev run: close -> process alive + tray icon; second exe exits < 2 s and the window shows; panel
  screenshot; Quit -> process gone.

## Test results
- step 1: `cargo test -p zenv-core --lib platform::` -> 10 passed (new: a_second_instance_signals_the_first,
  work_area_of_the_primary_monitor_is_not_empty, find_window_ignores_missing_titles)
- steps 2-4: `cargo check -p zenv-native --tests` -> clean, no warnings; `cargo test -p zenv-native -j 1 --test tray` -> 12 passed
- build errors "invalid metadata"/"can't find crate" at -j default and -j 2: `failed to mmap ... paging file is too small (os error 1455)`;
  commit memory had 2.9 GB free of 47.6 (Chrome, WSL, llama-server, 2 mysqld). -j 1 builds.
- device run 1 (ZENV_HOME=C:\zenv-dev, debug exe): tray click while hidden -> panic "the user callback was never called"
  (show_viewport_immediate, zenv.log 09:28:21Z); process died. Fixed: deferred viewport + shared Feed (G-016).
- device run 2: start -> window visible, Zenv icon in the title bar, private 53 MB; WM_CLOSE -> process alive, window hidden,
  private 53 MB; tray click (posted 6002/WM_LBUTTONUP, Explorer's message) -> panel 300x184 above the taskbar, foreground,
  layout as TrayPanel (screenshot); Escape -> panel gone; second exe -> exited code 0 in 201 ms, main visible + foreground;
  panel open then focus moved to main -> panel gone; Quit clicked in the panel -> process exited; no panic in zenv.log.
- `task.mjs check` 1: comment gate (a `// SAFETY:` line) -> removed; 2: `sed -i` had turned CRLF windows.rs into LF (whole
  file "added") -> restored CRLF, diff 91 added lines; 3: build/lint/Vitest OK, test-rust E0463 "can't find crate" (G-011)
  -> `cargo test -p zenv-core -j 1`: 245 passed, 1 failed `large_collections_page_and_search_quickly` (wall-clock < 2 s,
  untouched local_sites code); alone 3/3 passed in 0.12 s -> machine load, not this change.
- final: `task.mjs check` (2026-11-20, commit 4.4 GB free) -> build-rust, lint, test, test-rust (-j 1), test-native (-j 1) all OK
- qa round 1 (14-qa.md): PASS WITH NOTES. Fixes: Should 2 single-instance name normalized (`os::instance_name`, canonical
  path, lowercase, no trailing slash): test one_home_gets_one_instance_name...; device: first C:\zenv-dev, second
  C:\ZENV-DEV\ -> exited code 0 in 179 ms, first window foreground (QA had a second Zenv start). Should 3 error text max
  3 rows: test a_long_error_is_cut... FAILED without the cap ("error box grew by 258"), passes with it; tray tests 14 passed.
- Quit -> Core.shutdown: eframe path read (run.rs CloseRequested -> save_and_destroy -> app.on_exit); not run with a live service.

## Rulings
- tray-icon 0.25.1 (already in Cargo.lock through Tauri) instead of 0.26.1 named in M20 | no second muda/tray-icon in the build,
  same API | recheck when Tauri is removed in step 9
- tray panel as a deferred egui viewport fed through Arc<Mutex<Feed>> | immediate viewports panic while the root is hidden (G-016)
  | none known; the panel repaints on every root pass while open
- no tray icon (Explorer missing) -> closing the window quits | otherwise a hidden Zenv could never be reached | none
- single-instance name `Local\zenv-native-<home path>` | a dev build (C:\zenv-dev) and an installed Zenv may run side by side,
  two Zenvs on one home may not | a same-home Tauri Zenv is not detected (until step 9 removes Tauri)
- panel errors come from toasts (switch tasks are not inline) | React's panel shows the newest error toast | none
- qa Should 4 (no automated test of app.rs panel open/close) not fixed | the logic drives real eframe viewports and focus,
  which kittest embeds instead of creating windows; proven on the device by dev (Escape, blur) and qa (toggle, Escape,
  blur, 300 ms guard, double click, Quit in 3 states) | a regression there shows only on a device run
- qa Must 1 (ARCHITECTURE.md:53 lists the web build) -> REVIEW card, boss APPROVE 2026-11-23 -> ARCHITECTURE verify lines match project.json | ARCHITECTURE needs his approval
- verify without `npm run build` (vite) | M20 work is all in `src-native/`, React is untouched until step 9 removes it;
  vite build failed on memory 3 times; the boss: "kenapa build web kan buat zenv native?" (2026-11-20) | a `src/` change
  is checked only by typecheck + Vitest; ARCHITECTURE.md:53 still lists it (needs the boss)
- verify `test-rust` npm script -> direct `cargo test -p zenv-core -j 1` | same memory limit (G-011) | slower core test build
- verify `test-native` -j 2 -> -j 1 | commit memory runs out at -j 2 on this PC (G-011) | slower native test build
