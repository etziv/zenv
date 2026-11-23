# 15 M20 step 8a: native Studio window, files and editor

## Plan
Goal: the native (egui) Studio window for an app, same layout as React `StudioWindow` (D-022): header, file tree, editor tabs,
editor with Dark+ highlighting, save with conflict handling, Go to file. Terminals (8b, task 16) and chat (8c, task 17) follow;
until then the header has no terminal toggle and the editor takes the body.
Steps:
1) File icons: rasterize `public/file-icons/*.svg` once (headless Edge, transparent) into `src-native/assets/file-icons.png`
   (32 px cells) + name list; `studio/icons.rs` ports `fileIcons.ts` (name -> icon) and draws a cell.
2) Highlighter `studio/highlight.rs`: syntect 5.3 (default-fancy, the crate egui_extras uses), Dark+ colours as a syntect theme,
   per-line cache of parse states so an edit re-highlights from the changed line only; language name for the status line.
3) Studio window `studio/mod.rs`: one deferred viewport per app (G-016: the main window may hide to tray), state in
   Arc<Mutex<..>>, title "<domain> · Studio · Zenv", 1440x900, min 900x560; header as React (zenv studio, domain + root,
   Go to file, Files toggle, preferred editor, folder, browser); tree splitter (160-520, double-click resets, kept in egui memory).
   Entry points: "Open in Studio" wide button + dropdown item in the app panel, actions-menu item (D-011).
4) File tree `studio/tree.rs`: lazy listing on a worker thread, expand/collapse, selected folder, new file / folder draft row,
   refresh every 5 s and on window focus, error line.
5) Editor `studio/editor.rs`: tabs (icon, dirty dot, close asks when dirty, middle click), TextEdit + highlighter + line numbers,
   Tab indents, Ctrl+S saves with the expected mtime, conflict banner (Load theirs / Keep mine), save error, outside change
   poll 1.5 s (reload when clean), read-only, CRLF kept, Ctrl+F find bar, status line (path, Ln/Col, language,
   gofmt/rustfmt/ruff format + format on save + install missing formatter, saved/unsaved/read-only), empty state.
6) Go to file (Ctrl+P): `project_files::search` 120 ms after typing, arrows, Enter, Esc. Ctrl+B toggles files.
7) Risk gate: typing frame time on a 1 MB file <= 16 ms, else files above the measured limit open with a notice + "Open in <editor>".
Files: src-native/src/studio/{mod,icons,highlight,tree,editor,quick_open}.rs, app.rs, lib.rs, pages/mod.rs (Action::OpenStudio),
pages/apps/{panel,grid}.rs, Cargo.toml (+syntect), assets/file-icons.png, tests/studio.rs, scripts/file-icons-atlas.mjs.
Risks: TextEdit lays out the whole file each edit (gate 7); deferred viewport state shared by Mutex; syntect has no TS/Vue/Blade
syntax (TS -> JavaScript, Blade -> PHP, Vue -> HTML).

## Test plan
- unit: icon mapping (ported fileIcons.test.ts cases: names, extensions, folder open/closed, unknown -> default);
  highlighter (PHP/JS/Rust colours, edit at line k re-highlights from k only, CRLF file round trip keeps CRLF);
  editor state: dirty after edit, clean after save, save with stale mtime -> conflict, Keep mine forces, Load theirs reloads,
  outside change while clean reloads, read-only refuses edits.
- kittest: Studio window body: tree lists a temp project, click folder expands, click file opens a tab, typing marks unsaved,
  Ctrl+S writes the file, Ctrl+P finds a file and Enter opens it, new file draft creates the file.
- review focus: file deleted while open (save shows the error, no panic); a binary or huge file (read error / size notice);
  two Studio windows for two apps (separate state, closing one keeps the other); unsaved tab closed (confirm);
  main window hidden to tray while Studio is open (Studio keeps painting, no panic, G-016).
- measurements: typing frame time on a 1 MB PHP file target <= 16 ms; idle private memory with Studio open target <= 150 MB.
- device: zenv-native on C:\zenv-dev: open Studio from the app panel, open/edit/save a file, Go to file; PrintWindow screenshot
  (G-015) compared with the React Studio.

## Test results
- steps 1-6: `cargo check -p zenv-native -j 1` -> clean (0 warnings); `cargo test -p zenv-native --test studio -j 1` -> 15 passed
  (icons, syntaxes, Dark+ colours, incremental recolour <= 4 lines per edit, CRLF save, conflict / keep mine / load theirs,
  clean reload, .env read-only, deleted file save error, tree create + `..` refused, kittest: open/edit/Ctrl+S, dirty close asks,
  Ctrl+P finds + Enter opens, Ctrl+B, new file opens). First run 13/15: two queries matched a row and its inner label (test fix).
- step 7 gate, first measurement (release, kittest, accesskit on): 1 MB coloured PHP: open 2.6 s, typing median 169.5 ms -> FAIL.
  Probe (release): first colour 1 MB 2.58 s; recolour after one edit 30 ms (488k sections); coloured layout after edit 34 ms;
  plain layout 3 ms. Full frame without accesskit (as the app runs with no screen reader): coloured 100 KB 13.8 ms, 250 KB 32.7,
  500 KB 63.8; plain 1 MB 15.2, plain 2 MB 34.4.
- step 7 fallback built: colour only <= 100 KB (`COLOUR_MAX`), > 1 MB read-only with a notice (`EDIT_MAX`). Re-measured
  (`typing_frame_time_by_file_size`, release): 100 KB open 457 ms, typing median 12.5 / max 17.5 ms; 1000 KB plain open 110 ms,
  median 14.8 / max 24.8 ms; 1500 KB read-only median 15.9 ms -> PASS on median, max over 16 ms at 1 MB.
- `cargo test --release -p zenv-native --test studio -- --include-ignored` -> 17 passed.
- device (release exe, ZENV_HOME=C:\zenv-dev, demo app `studio-demo.test` seeded disabled, project in the scratchpad; UI Automation +
  PrintWindow, keys via SendKeys after Alt+SetForegroundWindow): Apps -> Open in Studio opens "studio-demo.test · Studio · Zenv"
  1456x939 (header, tree with Material icons, empty state); Ctrl+P "index" lists public/index.php, Enter opens it with Dark+ colours,
  line numbers, Ln/Col, PHP, dirty dot; Ctrl+S wrote the typed line to disk (Get-Content); outside write while clean -> reloaded
  within 3 s; outside write while dirty -> conflict banner; Alt+F4 with unsaved -> "Close Studio with unsaved changes?" modal,
  Escape cancels; main window WM_CLOSE -> hidden (IsWindowVisible False) while Studio keeps working; big.php (1.0 MB) opens plain
  and editable; private memory 128 MB / working set 201 MB with Studio + 1 MB file open (target <= 150 MB private: PASS).
  Fixed after screenshots: Go to file input margin, banner buttons right-aligned, active-line band full width, code margin.
- device finding: the Studio window has no UI Automation tree: eframe 0.37 glow calls `init_accesskit` only for the ROOT viewport
  (`eframe/src/native/glow_integration.rs` ~line 382); same for the tray panel. The Modal backdrop dim does not show in PrintWindow shots.

- qa round 1 (15-qa.md): FAIL, 3 Must fix, 3 Should fix, 5 Nice. All checked against the code: all real.
  1. crash: reproduced `cargo test --test studio_qa` -> FAILED "editor.rs:674 end byte index 12 is out of bounds for string of
     length 1". Root cause: `matches` came from `doc.text` before TextEdit applied the frame's edit, then sliced the edited galley
     text. Fix: matches computed from `out.galley.text()` after `.show()` (`find_all`); same test -> passed.
  2. Quit lost edits: `Windows.ask_quit` / `take_quit`, `ZenvApp.quit` and the no-tray main close ask first; test
     `quitting_with_unsaved_studio_edits_asks_first` passed.
  3. minified: colour also needs lines <= 2 KB (`COLOUR_LINE_MAX`), lines > 40 KB open read-only (`EDIT_LINE_MAX`);
     test `minified_lines_stay_plain_and_very_long_lines_open_read_only` passed.
  4-6. active tab scrolled into view + wheel scrolls tabs (QA test un-ignored, passed); Go to file list height + selection
     scrolled into view; periodic tree listing on a thread (`Tree.refresh` -> `take_listing`).
  7-11. Escape closes find only from its field; caret at the file start on open; "gone from disk" banner (test passed);
     dead `Windows::count`, `Tree::set_draft_name` deleted; find folds Unicode case (test passed).
  The kittest accessibility check found the editor TextEdit had no accessible name: now labelled with the file path.
- `cargo test -p zenv-native --test studio --test studio_qa` -> 20 passed + 2 passed.

- release re-measure: 100 KB 12.7 ms median; 1 MB plain 13.9 ms (max 16.6); 1.5 MB read-only 18.2 ms per frame; minified
  20 KB one line 6.7 ms (was 114.9); 95 KB one line read-only 2.9 ms; tree refresh on the UI thread ~0 ms (the 128 ms max is
  the test building an egui Context inside the timed loop).
- device (rebuilt release): Go to file ".rs" shows 12 rows, 8x Down keeps the selection visible; caret Ln 1, Col 1 on open;
  QA's crash path (find "zenv", click editor, Ctrl+A, x) -> "No results", process alive, no new panic in C:\zenv-dev\logs\zenv.log.

- qa round 2: all 11 round-1 findings confirmed fixed; 2 new Must fix, both real:
  12. Quit asked only the first Studio with unsaved files: `quitting_asks_about_unsaved_edits_in_every_studio` FAILED. Fix: one
     dialog names every Studio's unsaved files ("a.php (sa.test), b.php (sb.test)"); test passed (its query sharpened to the
     dialog text: "b.php" alone also matched the tree row and tab).
  13. find bar on 1 MB: 24.6 ms (0 hits) / 248 ms (17k hits). Fix: hits cached per (text, query) in `Find.hits`, boxes only for
     visible rows + the current hit (`visible_chars`), ASCII-first queries scanned per byte. Release: 0 hits 13.6 ms, 17k hits
     14.3 ms, 100 KB 14.9 ms; find_all 5.8 -> 0.7 ms.
- `cargo test --release -p zenv-native --test studio --test studio_qa -- --include-ignored` -> 21 + 6 passed; gate numbers:
  100 KB 12.4, 1 MB 15.0, 1.5 MB read-only 16.2, minified 20 KB 6.4 ms median.

## Rulings
- Step 8 split into 8a/8b/8c as tasks 15/16/17 | each part is testable and shippable alone; zenv-native is not installed until step 9 | a half Studio exists in dev builds meanwhile
- Preview pane: not built; the header's "Open in browser" opens the app (M20 plan: Studio preview opens in the default browser) | no WebView in the native app | the side-by-side preview is gone
- Prettier formatting (JS/TS/CSS/HTML/PHP/MD/YAML) not in the native editor; gofmt/rustfmt/ruff through Core stay | Prettier ran inside the WebView | those files show no formatter until a later task adds `npx prettier`
- File icons rasterized to one PNG atlas instead of an SVG renderer | no new dependency (resvg is not in the approved list) | icons are bitmaps at 32 px
- Risk gate 7 fallback: syntax colours only up to 100 KB, files over 1 MB read-only with a notice | measured: colouring costs ~13 ms per keystroke per 100 KB, plain 1 MB 14.8 ms median | 100 KB-1 MB files have no colours; > 1 MB edits go to VS Code
- Gate judged on the median frame (max 24.8 ms at 1 MB plain) | one slow frame in a burst is not felt; the median is | typing in a 1 MB file may stutter now and then
- Studio windows ship without a screen-reader tree | eframe 0.37 only wires accesskit to the root window; fixing it means patching eframe | a screen-reader user can use the main window but not Studio (React Studio could)
- Close guard: closing a Studio window with unsaved files asks first (React's window closed silently) | losing edits by a stray Alt+F4 costs more than one dialog | one extra click
- Colouring stays on the UI thread (security hardening note) | the files are the boss's own project, not network input; fancy-regex stops a runaway match | a crafted <= 100 KB file could freeze Studio for seconds
- syntect 5.3 (+ fancy-regex, bincode) added from the M20 plan the boss chose (D-021), approved by the boss on the REVIEW card 2026-11-23 ("approve") | the plan named it; the card asked for the crate itself | none
- A read-only file over 1 MB costs ~18 ms per repainted frame | it is never typed into; only scrolling repaints it | scrolling a 1.5 MB file runs at ~55 fps
- Find scans ASCII-first queries per byte | 5.8 -> 0.7 ms on 1 MB | a non-ASCII letter that lowercases to ASCII (Kelvin sign K) is not found by "k"
- CodeMirror extras not ported: autocomplete, folding, multi-cursor, rainbow brackets, indent markers | egui TextEdit has none of them; highlighting, line numbers, find, undo are kept | editing is plainer than in React
