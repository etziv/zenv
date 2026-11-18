# 12 Pure Rust UI without WebView

## Plan
Goal: the boss wants Zenv's UI in pure Rust, no WebView2 ("seperti figcraft / artcraft launcher"; both are egui/eframe + wgpu,
checked from their exe strings 2026-11-18). Measured now: Zenv = zenv.exe 23 MB + 8 WebView2 processes ~460 MB (+ go.exe 69 MB).
Facts: `src-tauri/core` (zenv-core, ~21.4k Rust lines) has no Tauri dependency and stays as is; Tauri layer ~1.3k lines,
117 commands; React UI 92 files / ~13k lines in 10 features (apps, dashboard, logs, runtimes, settings, sites, studio, tools,
tray, web-servers); Studio uses xterm.js, CodeMirror 6 and an iframe preview (`frame-src http:`).
Steps:
1) Verify current crate versions (eframe/egui, terminal, tray, editor, markdown, file dialog) from crates.io/docs.
2) PROPOSAL card (architecture change): options, migration order per module, risk gates with fallbacks, new dependencies.
3) After PILIH: ARCHITECTURE.md / DECISIONS / contracts / MODULES for the new UI, then build module by module
   (each step: plan + test plan here, tests, `task.mjs check`, run the app and measure RAM).
Files: new crate for the UI (name decided in the proposal); `src-tauri/core` untouched except where the UI needs a public API.
Risks: Studio preview needs a browser engine; terminal and code editor parity; DESIGN.md mapping to egui styles; regressions
in a 13k-line rewrite.

## Test plan
(written per module after the proposal is approved)

## Test results
- step 1 (crates.io API, 2026-11-18): eframe/egui/egui_extras/egui_kittest 0.37.0 (2026-10-15), alacritty_terminal 0.26.0,
  tray-icon 0.26.1, egui_commonmark 0.26.0, rfd 0.18.0, syntect 5.3.0, wry 0.57.2, slint 1.19.0 (GPL-3.0 | Slint
  Royalty-free 2.0 | commercial), iced 0.14.0 (2025-12-07). egui_term 0.1.0 needs egui ^0.31 / alacritty ^0.25: not usable.

- step 2: PROPOSAL card sent; the boss 2026-11-18: "A jangan kerjakan dulu, buat task dulu aja" -> recorded as D-021 + MODULES M20
  (planned) + STATUS line; no code written. Building M20 is a later task.

## Rulings
- Task 12 ships memory only (decision + plan), M20 is built in its own task later | the boss said not to build yet | none
- Studio preview in the default browser, not a wry WebView | the goal is zero WebView | the boss may want in-app preview: wry 0.57 child window
