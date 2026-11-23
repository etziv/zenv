# 16 security review (round 1): native Studio terminals

## Plan
Scope: uncommitted task/16 changes: src-native/src/studio/terminal/{mod.rs,keys.rs,colors.rs}, src-native/src/studio/terminals.rs,
src-native/src/theme.rs (font read), src-native/Cargo.toml + src-tauri/Cargo.lock (alacritty_terminal 0.26 and new transitive crates).
Surfaces: supply chain (`references/supply-chain-ci.md` Dependencies), untrusted parser + resource use (`references/availability.md`).
Checks:
1. New crates: what is added to the lock, build scripts, `cargo audit --no-fetch` (read-only).
2. alacritty_terminal 0.26 / vte 0.15 source in the cargo registry: every producer of `PtyWrite`, `ColorRequest`,
   `TextAreaSizeRequest`, `ClipboardStore/Load`, `Title`; whether any reply echoes program-controlled bytes; OSC 52 config; buffer limits.
3. `Listener`/`Outbox` threading: who writes into the PTY, on which thread, under which locks (core `studio.rs` write/read path).
4. `keys::paste` bracketed-paste stripping; key/mouse encoders (bounds); error text fed to the parser.
5. theme.rs `%SystemRoot%\Fonts\seguisym.ttf`.
Proof: source traces with file:line; no builds.

## Findings

1. [low] [needs validation] Terminal answers are written into the PTY synchronously from the output reader thread, under the Screen lock and the core sessions lock (`src-native/src/studio/terminal/mod.rs:L41-47, L71-85, L98-100`; `src-native/src/studio/terminals.rs:L304-336`; core `src-tauri/core/src/studio.rs:L170-181`)
   - What this is: the reader thread (`terminal-read`, studio.rs:L154-165) calls the sink, which locks the tab's `Screen` and runs the vte parser; each query in the output (DSR `ESC[6n`, DA `ESC[c`, DECRQM, OSC 4/10/11 `?`, CSI 14t/18t) makes `Listener` call `Outbox::send` -> `core.studio.write`, which takes the global `sessions` mutex and does a blocking `write_all` + `flush` on the ConPTY input pipe. Microsoft's ConPTY guidance is to service input and output on separate threads for this reason.
   - Problem: a file from an untrusted repository that the boss prints (`cat README.md`, `type`, a build log) can hold thousands of `ESC[c`/`ESC[6n`. Each 3-4 byte query becomes a 6-10 byte reply. If conhost passes these queries through and stops draining its input pipe while its own output write is blocked (our reader is the thread that would drain it, and it is stuck in `write_all`), the reader blocks forever holding the `Screen` mutex and the global `sessions` mutex. Then the egui thread blocks in `Terminal::ui` (`shared.lock()`, mod.rs:L194) and in `studio.write/resize/close`, so every Zenv window (main window, service panel, every Studio window) freezes. Unsaved editor buffers in Studio are lost when the boss kills the process. Typed input has the same root cause: `session_ui` (terminals.rs:L291) writes on the UI thread.
   - Missing fact: whether app-issued queries reach Zenv through the Windows 11 conhost on this build, and whether the 4 KB pipe fills. Settle it locally: in a Studio PowerShell tab run `[Console]::Write("$([char]27)[c" * 200000)` (and the same with `[6n`), then check that the window still repaints and accepts input.
   - Fix: never write to the PTY from the reader thread or the UI thread, and never while holding the Screen or sessions lock. Have `Outbox` push bytes into a channel served by a per-session writer thread (or make `Outbox::send` only enqueue, and let a writer thread call `studio.write`). In core, take the session's writer out of the `sessions` mutex (e.g. an `Arc<Mutex<writer>>` per session) so a stuck pipe cannot lock every terminal. Test: feed a Screen 1 MB of `ESC[6n` with a sink that blocks, and check that `feed` returns and that `lock()` on the Screen succeeds from another thread.
   - If we skip it: printed untrusted content may hang the whole app (all windows), which needs a kill and loses unsaved Studio edits.

Rejected:
- Shell input injection through terminal replies: every `PtyWrite` in alacritty_terminal 0.26 is a fixed format with numbers only (term/mod.rs:L1256-1284 DA/kitty mode, L1325-1342 DSR, L2090/L2147 DECRQM, L2270 CSI 18t; L2260 pixel size from our `WindowSize`). The colour reply prefix comes from a parsed number (vte ansi.rs:L1383-1385, L1439) and the index is bounded (u8 or <= Cursor 258, which `colors::default_rgb` covers with no panic). There is no title report (CSI 21t), no DECRQSS and no XTGETTCAP. Program-controlled text never comes back as input.
- Clipboard write or read by the program: `Osc52::Disabled` (mod.rs:L94). The store and load paths both return early (term/mod.rs:L1711-1714, L1731-1734). `ClipboardStore/Load` would also fall into `_ => {}` in `Listener`.
- Title, file or URL changes: `Title`/`ResetTitle` events are dropped, OSC 8 hyperlinks are stored but never opened (no open or URL call in the terminal code), and no sequence touches files.
- Bracketed-paste breakout: `keys::paste` removes every ESC (and ETX) before it wraps the text (keys.rs:L77-84), so pasted `ESC[201~` cannot close the bracket. A C1 CSI (U+009B) arrives as a 2-byte UTF-8 character, not ESC. Unbracketed paste is the program's own choice (it did not enable mode 2004), the same as any terminal.
- Hidden/concealed text in a copied selection (SGR 8 is copied): an attacker gets the same result with fg = bg truecolor, which no terminal can stop, and the boss chooses what he selects.
- Error text fed to the parser (terminals.rs:L342): the messages hold fixed strings and Windows paths, and NTFS forbids control characters in names, so there is no escape-sequence path. The queued answers flushed to the next program are fixed-format too.
- Key/mouse encoders: `TILDE[n-5]` is only reached for F5-F12. X10 mouse bytes stay <= 255 (`v.min(222)`, code <= 93+32). `cols-1`/`rows-1` cannot underflow (MIN_SIZE 20x5).
- theme.rs font read: `%SystemRoot%` is the user's own environment, and anyone who can change it already runs code as the user. The file is a system font read only, not a trust crossing.
- New crates (alacritty_terminal 0.26.0, vte 0.15.0, home 0.5.12, miow 0.6.1, rustix-openpty 0.2.0, signal-hook 0.4.6, windows-sys 0.59.0) all come from crates.io and are pinned with checksums in the lock. Only signal-hook has a build.rs, and it compiles C only with feature `extended-siginfo-raw` on non-Windows. `cargo audit --no-fetch` lists no advisory for any added crate (the 9 it reports are older crates: zip, url, libsqlite3-sys, webpki-root-certs and others, not in scope). Lock lock-order check: Screen -> Wire -> sessions and State -> Wire -> sessions, never reversed, so the locks cannot deadlock each other.

Hardening (not counted):
- vte 0.15 with `std` keeps an unbounded OSC buffer (lib.rs:L64 `Vec<u8>`), and alacritty stores unbounded zero-width characters per cell (cell.rs:L164). Memory grows with the bytes one program prints: no victim beyond the boss's own session, same as Alacritty.
- Scrollback is 10000 lines x up to 1000 cols per tab (16 tabs max). Consider `scrolling_history` sized to the real width, or a lower cap.
- Multi-line paste with no bracketed mode runs each line at once. A confirmation for multi-line paste (as Windows Terminal does) would blunt clipboard pastejacking from web pages.

Findings: high 0, medium 0, low 1
Safe to proceed: yes
Not checked: no build or live run (the finding 1 test above was not run, so whether conhost forwards app queries and the pipe fills is open); `cargo audit` ran with the local advisory DB (`--no-fetch`, age not checked); non-Windows targets (rustix-openpty, signal-hook paths); alacritty/vte parser fuzz robustness beyond reading the code; core `studio_env`/launch (unchanged) not re-audited.
