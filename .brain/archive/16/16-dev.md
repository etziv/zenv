# 16 M20 step 8b: native Studio terminals
## Plan
Goal: the native Studio window gets the React terminal panel (TerminalPanel.tsx + Terminal.tsx): tabs of Claude Code / Codex / PowerShell
terminals in the project folder, `+` menu, close ends the program, Enter restarts after exit, Ctrl+` and the PanelRight toggle, the
splitter with its share persisted, the "Empty editor" close; closing the window ends its terminals. Chat tab is step 8c.
Steps:
1) `alacritty_terminal` 0.26 (M20 plan crate, D-021) + `studio/terminal/`: Screen (Term + vte Processor fed by core's PTY sink),
   colours (xterm.js THEME + Tango + 256 cube), key/mouse encoding, painter widget (runs of cells, wide chars, cursor, selection,
   scrollback, copy/paste, bracketed paste, SGR mouse), outbox that answers queries (ConPTY's DSR) from the reader thread.
2) `studio/terminals.rs`: the panel (tabs with LED, close, `+` menu from `studio_agents`, empty state), sessions via
   `Core::studio_open_terminal` / `studio.write` / `resize` / `close`, exit line and restart.
3) `studio/mod.rs`: layout editor | splitter | terminal (share % persisted), header toggle, Ctrl+`, hide chevron, close_owned_by on close.
4) Tests + gate run: claude, vim, truecolor, resize, paste in a real ConPTY; screenshot of the Studio window.
Files: src-native/Cargo.toml, src-native/src/studio/{mod.rs, editor.rs, terminals.rs, terminal/*}, src-native/tests/terminal.rs.
Risks: ConPTY blocks on its cursor query until answered; key mapping gaps for vim/claude; paint cost of a 200x60 grid per frame.

## Test plan
- unit: key encoding (Enter, Shift+Enter, arrows normal/app-cursor, modifiers, Ctrl+letters, F-keys, Home/End/PgUp, Tab/Shift+Tab);
  colours (named, bright-on-bold, 256 cube, grey ramp, truecolor, inverse, dim); paste (newlines to CR, bracketed mode, ESC stripped
  inside bracketed paste); Screen feed -> visible text; wide chars; resize keeps content; exit line text.
- review focus: output before the id is known (queued and flushed); a query answered while the window is hidden (reader thread);
  closing a tab while output streams; restart after exit with an un-installed agent (error line, Enter retries); 16-terminal limit error
  shown in the terminal; Ctrl+C with/without selection; wheel in alt screen vs scrollback; typing snaps the view to the bottom.
- measurements: frame time of a full 200x60 coloured screen target <= 4 ms (release, `Context::run_ui`, not a kittest Harness: G-018).
- device: real ConPTY e2e test (PowerShell echo, resize -> `$Host.UI.RawUI.WindowSize`, exit + restart); gate run in the real window:
  claude, vim, a truecolor test line, resize, paste; screenshot via PrintWindow (G-015).

## Test results
- steps 1-3: `cargo check -p zenv-native -j 1` -> ok, no warnings
- step 4: `cargo test -p zenv-native -j 1 --test terminal` -> 10 passed, 1 failed (e2e: test site had no document root: "choose the document root"); fixed the test input
- step 4: `--test terminal powershell` -> 1 passed (real ConPTY: prompt, echo zenv-42, width = grid cols before and after resize, exit line, Enter restarts on a clean screen)
- builds hit "paging file is too small (os error 1455)" repeatedly (1.2-2.4 GB commit free: VS Code 8 GB, Android Studio/Gradle 3.7 GB, Chrome, WSL, Ollama); reruns via a retry script passed (G-011)
- device (debug build, `ZENV_HOME=C:\zenv-dev`, demo app `term-demo.test`, PrintWindow screenshots):
  - Claude Code opens as the first tab, trust prompt answered with Down+Enter, Esc closes its Chrome prompt; colours, bold, dim and its mouse selection (SGR reports) show
  - found: Claude Code drew no colours -> cause: the Zenv I started inherited `NO_COLOR=1` from this Claude Code session's shell; relaunched without it: colours show (not a product bug)
  - found: `+` menu looked empty -> cause: the popup took the panel's width (586 px, logged) and my screenshot crop cut its text; fixed with a 220 px menu aligned under `+`; test `plus_menu_is_compact_and_adds_a_tab`
  - found: ⏵ ◐ ✻ ⎿ ⧉ ⏸ ⏺ drawn as boxes (JetBrains Mono lacks them) -> Segoe UI Symbol from `%SystemRoot%\Fonts` as monospace fallback; test `claude_code_symbols_have_glyphs`
  - found: fast typing lost characters in the first runs -> cause: my driver's Alt tap (to get focus) made `.` arrive as Alt+. (PSReadLine yank-last-arg inserted `exit`); a logged run showed every byte arriving in order (`DBG input` lines, log removed)
  - PowerShell tab from the `+` menu: `.	ruecolor.ps1` shows the 24-bit gradient, bold red, underline, inverse, 256-colour orange
  - vim (Git's) on notes.txt: alt screen, `A` + Esc, status line; window maximized -> vim redrawn to the full size; clipboard "pasted one / pasted two" pasted with Ctrl+V in insert mode, `:wq` -> file is `first linehello from zenv
pasted one
pasted two
`
  - closing the Studio window: claude.exe and powershell.exe ended, no orphans; Zenv private memory 60-77 MB during the run (watchdog log)
- final `--test terminal`: 15 passed, 1 ignored (measurement); `task.mjs check`: build-rust, lint, test, test-rust, test-native all OK
- security round 1: high 0, medium 0, low 1 (replies written to the PTY from the reader thread under the Screen and sessions locks could hang every window if a printed file floods queries). Reproduction `a_flood_of_terminal_queries_does_not_freeze_the_window`: PowerShell in a real ConPTY prints 100000x `ESC[c ESC[6n`, then `flood-42` -> passed in 0.71 s; the prompt afterwards held `[?61;6;7;21;22;23;24;28;32;42c`, conhost's own DA answer (alacritty answers `?6c`), so conhost answers app queries itself and they never reach Zenv's reader
- qa round 1: FAIL, Must fix 2 (terminal panel over the file tree at wide shares; a resize while the program starts never reaches the PTY), Nice to have 2 (popup clicks reported to a mouse-mode program; Alt flag outliving its key). QA's `--test terminal_qa`: 0 passed, 2 failed (its run)
- fixes: terminal edge clamped to the tree's edge (`body`), pending size flushed with `studio.resize` when the open returns (`start`), presses/moves reported only when the pointer is on the terminal (`contains_pointer`), any encoded key clears the Alt flag; new tests `an_alt_chord_without_text_...`, `clicks_on_a_popup_over_the_terminal_...`
- after fixes: `--test terminal --test terminal_qa` -> 17 passed + 1 ignored, 2 passed
- qa round 2: PASS WITH NOTES, findings 1-4 confirmed fixed; new Nice 5 (the start flush could overwrite a newer UI resize) fixed by resending until the grid size is stable; `--test terminal --test terminal_qa` -> 17 passed + 1 ignored, 2 passed

## Rulings
- `alacritty_terminal` 0.26 added without a new question | it is in the M20 crate list the boss approved with D-021; only its grid and parser are used, the PTY stays portable-pty in core | one crate (+polling, miow, signal-hook transitively) to remove if he objects
- Until 8c the first tab is a Claude Code terminal (else PowerShell) and the `+` menu has no chat item | the React first tab is the chat, which is step 8c | the boss sees a terminal where React shows the chat for one step
- Steady cursor, no blink | blinking repaints every Studio window twice a second; Claude Code draws its own cursor | looks slightly different from xterm.js
- Paste is Ctrl+V and Shift+Insert, not Ctrl+Shift+V | egui hands the clipboard only to its own paste shortcuts; reading it otherwise needs a clipboard crate (new dependency) | Ctrl+Shift+V sends ^V
- Answers to the program's queries (DSR, colour, size) go straight from the reader thread through `Outbox` | ConPTY waits for its cursor query; a hidden or minimised window must not stall the program | none known
- measurement (`cargo test --release --test terminal full_screen -- --ignored`, `Context::run_ui` + tessellate, 203x62): first painter (a galley per colour run): debug 3.2 / 8.1 / 26.4 ms median (plain / runs of 5 / every cell coloured); one galley per ASCII stretch with colour sections: release 0.61 / 0.72 / 0.91 ms median, max (first frame of a new screen) 3.5 / 11.4 / 45.4 ms -> target <= 4 ms met
- Studio terminals keep the user's `NO_COLOR` | it is the user's own setting; only Claude Code session variables are removed (as before) | a user with NO_COLOR set sees plain terminals, as in any terminal
- Missing symbols come from Segoe UI Symbol read from the Windows fonts folder at start, not bundled | present on every Windows 10/11; bundling a symbol font adds MBs | on a system without it those symbols stay boxes
- Programs in a terminal cannot set the clipboard (OSC 52 off) | xterm.js in the React UI ignored OSC 52; alacritty_terminal allows copy by default, which lets any printed text replace what the boss pastes next | a tool that copies through OSC 52 (over SSH) cannot copy; Ctrl+C on a selection still works
- Security finding 1 (blocking replies on the reader thread) not changed | unreachable through ConPTY as Zenv opens it: conhost answers the program's queries itself (flood test above, its DA reply came back as PowerShell input); Zenv only answers conhost's one start-up cursor query | if a later Windows build forwards app queries (passthrough mode), a flood could hang the windows: then move PTY writes to a writer thread per session and take the writer out of core's sessions lock; the flood test stays as the regression check
