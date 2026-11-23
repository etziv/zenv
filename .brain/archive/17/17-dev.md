# 17 M20 step 8c: native Studio chat

## Plan
Goal: the Claude Code chat tab in the native Studio window, matching the React chat (D-022): header (LED, model, pending-settings note,
History, New chat), conversation (user bubbles, streamed Markdown, tool cards with permission answers, results, notices, "Show N earlier"),
composer (text, @ file and / command menus, attachments by picker / drop / paste incl. files copied in Explorer and images, attach menu,
"/" palette with models, effort, thinking, fast, output styles, MCP status/toggle/reconnect, account and usage, terminal screens,
usage chip, model + effort menu, permission-mode menu, Send / Stop). Core already has the whole API (F-004, F-005); this is UI only.

Steps:
1) core: `ChatOptions`, `Effort`, `PermissionMode` also `Serialize` + `PartialEq` (persisting the composer's choice, `sameOptions`).
2) `studio/chat/model.rs`: port of model.ts (`ChatState`, `Item`, `apply_lines`, reducer actions) on `serde_json::Value`; unit tests port model.test.ts.
3) `studio/chat/options.rs`: MODES, EFFORTS, fallback models/commands, `pretty_model`, `until_text`, `ago`, attachment from a path or a pasted image, persisted options + catalog (egui persisted data). Tests.
4) `studio/chat/markdown.rs`: port of Markdown.tsx's parser to blocks + inline spans (fancy-regex, already locked via syntect), drawn with egui. Tests on the parse.
5) `studio/chat/conn.rs`: one worker thread per chat running ops in order (open lazily with options/resume, send, answer, control, interrupt, end); lines come back tagged with a generation; control answers routed by request id.
6) `studio/chat/mod.rs` (Chat: header, items, tool cards), `composer.rs`, `palette.rs`.
7) `terminals.rs`: a tab is a chat or a terminal; "New tab" menu gets "Claude Code chat"; first tab is the chat when Claude Code is installed (else PowerShell), like TerminalPanel.tsx; palette terminal screens open a Claude Code terminal tab; the chat gets the editor's active file and can open files in the editor (Studio wiring in studio/mod.rs).
8) kittest UI tests with fed lines (no Claude Code): user/streamed/tool/permission/result/notice, permission buttons send the answer, slash and @ menus, palette filter, send clears the box. One live run in the real window with Claude Code (haiku, tiny prompt).

Files: src-tauri/core/src/studio_chat.rs (derives), src-native/Cargo.toml (arboard, fancy-regex, windows-sys: all already in Cargo.lock),
src-native/src/studio/{mod.rs,terminals.rs,chat/*}, src-native/tests/{chat.rs,terminal.rs,terminal_qa.rs}.
Risks: egui TextEdit caret handling for menus (Enter/Tab/arrows consumed before the TextEdit); Explorer file paste has no egui event (egui-winit
swallows Ctrl+V when the clipboard has no text) -> read Ctrl+V with GetKeyState in the composer and take `arboard` file_list; long sessions
(200 shown, "Show N earlier"); streamed text re-parsed per frame -> parse cached per item and only re-parsed when its text changes.

## Test plan
- unit (model): init sets session/model/commands; stream start+deltas build one streaming text, assistant message ends it with the final text;
  tool_use then tool_result fills the result; duplicate tool_use ignored; control_request attaches a pending permission (setMode and addRules
  suggestions -> labels), cancel removes it, result clears pending ones and sets busy false with seconds; zenv_prompt, zenv_truncated, zenv_exit
  (code 0 silent, non-zero notice with stderr); history resets and keeps the session id; answered sets allowed/denied and "always" switches mode.
- unit (options): pretty_model ("claude-opus-5-5[1m]" -> "Opus 5.5 (1M)", dated ids, unknown kept), until_text, ago, valid_model, attachment kinds
  (png -> image, .rs <= 256 KB -> text, big text -> file, > 20 MB refused), options round-trip through persistence.
- unit (markdown): fences with language, headings, rule, table (separator row dropped), quote nesting, ordered/unordered lists with continuation,
  paragraphs; inline code/bold/italic/link spans; `snake_case_name` stays plain.
- review focus: (1) a message typed while Claude is busy waits its turn (sent, shown) -> UI test; (2) settings changed between turns restart on the
  next message and show the pending note -> unit on `needs_restart`; (3) Enter in the @ menu picks the file instead of sending -> UI test;
  (4) a permission answered after the program ended does not panic and shows a notice -> UI test on answer with no connection;
  (5) "New chat" while streaming ignores late lines of the old program -> unit on the generation filter.
- measurements: a 1,000-item conversation (200 shown) frame time target <= 16 ms median (release), streaming 5,000 deltas applied without frame > 16 ms.
- device: real Studio window (`ZENV_HOME=C:\zenv-dev`): chat tab first, ask "reply with the word pong" on haiku, see streaming text, done in Ns,
  History lists the session, New chat clears, paste a file copied in Explorer shows a chip, palette opens, MCP view answers. Screenshots read.

## Test results
- step 1: core derives (4 lines); `cargo check -p zenv-native --tests` -> ok after steps 2-7.
- steps 2-6 unit: `cargo test -p zenv-native --lib -- studio::chat` -> 28 passed (model 8, options 7, markdown 6, composer 4, palette 1, tools 1, conn 1) first run;
  + `studio::chat::tests` 2 passed (old-generation lines ignored after New chat; MCP status failure shows its error).
- step 8 UI (kittest, `--test chat`): first run 4 passed / 2 failed. Cause (debugging, logged per frame): an `egui::Area` that changes size
  is laid out one frame at the old pivot position (y=350, then 649); kittest kept the stale node rect for a few frames, so the click landed
  on nothing. Not a product bug (the row is hovered at its real place). Fix in the test: `AREA_SETTLE = 6` frames before querying a
  resized Area. Rerun: 6 passed, 1 ignored (measurement).
  Covered: empty state + catalog used (no Claude Code started); conversation draws, permission "Allow" answered, no-program answer -> notice
  (review focus 4); message typed while busy waits its turn + failed start shown, busy ends (focus 1); Enter/Tab in / and @ menus pick, nothing
  sent (focus 3); palette filter -> Output styles -> Explanatory saved and closes, Esc closes; New chat clears, export text.
  Focus 2 (restart on new settings) and 5 (late lines) are unit tests in conn.rs and chat/mod.rs.
- own review, bug: Shift+Enter sent the message (egui `consume_key(NONE, Enter)` matches with Shift held: `Modifiers::matches_logically`
  ignores Shift when the pattern lacks it). Test `shift_enter_is_a_new_line_and_enter_sends` FAILED (sent "line one"), fix `plain = !shift`
  in composer.rs, test passes.
- own review, gap: editor Ctrl+S / Ctrl+F / Shift+Alt+F still fired while typing in the chat (only terminals counted as focus). Now
  `Terminals::has_focus` counts the active chat's composer/palette; test `a_focused_chat_keeps_the_editor_shortcuts_away` passes. `--test chat`: 8 passed.
- measurements (release, `--test chat -- --ignored`): 1,000 items (200 shown) median 1.10 ms, max 1.61 ms (target <= 16 ms: met);
  streaming 5,100 deltas into ONE ~60 KB message: median 8.21 ms, max 17.41 ms (target max <= 16 ms: missed by 1.4 ms at the end of the message).

- device (debug build, `ZENV_HOME=C:\zenv-dev`, app `term-demo.test`, PrintWindow screenshots, windows picked by process id so the
  installed Zenv is never touched):
  - Studio opens with the Claude chat tab first; empty state, header (History, New chat), composer toolbar as in React.
  - model menu lists Claude Code's own catalog (Default, Opus 5.5, Fable 5.2, Sonnet 5.5, Haiku 5.5, ...); Haiku 5.5 chosen and kept after a restart.
  - "Reply with just the word pong." -> streamed "pong", "done in 19.2s" (Claude Code's own duration), tab LED off after, header LED on (program open).
  - History lists "Pong reply · 1m ago"; loading it shows the transcript; next message resumes it: "ping", done in 6.5s.
  - Ctrl+V of a file copied like Explorer (`Set-Clipboard -Path`, CF_HDROP) -> "notes.txt" chip; sent: bubble with the file chip, Claude read it.
  - Markdown reply drawn: bullets, bold, inline code chips, paragraphs. Palette: sections, switches, effort dots, model name on the right.
  - found + fixed: composer text touched the box edge (egui ignores `TextEdit::margin` when a frame is given; margin moved into the frame);
    user bubble spanned the panel (now sized to its text, max 90%). Both seen fixed in the next run.
  - NOT reproduced: in one instance the "/" chip neither hovered nor clicked twice (the click selected the last label instead). After a
    rebuild with hit-test logging, 4 tries reproducing every step before it (focused composer, a sent message, a pasted file, a zero-delay
    and a 60 ms click) all opened the palette (`hovered=true`, one `clicked=true` frame each). Kittest
    `composer_buttons_work_under_a_long_conversation` passes. Left open for qa.
- own review, risk: the inline Markdown regex runs on Claude's text every streaming frame. Hostile 58 KB inputs (release, `hostile`
  measurement): `[a](` x15,000 took 793 ms (each `[` scanned to the end: label and url could hold `[`). Bounding lengths alone: 640 ms.
  Fix: label `[^\[\]\n]{1,500}`, url `[^)\s\[]{1,2048}` -> 10 ms; every other case (`**`, `*`, `_`, backticks, open brackets, a 60 KB url,
  italic/bold without a close) 8-31 ms. Guard test `links_without_an_end_stay_linear` (< 1 s in debug). React has the same regex (not changed here).
- security round 1 (`17-security.md`, 1 high / 2 medium / 1 low), each checked against the code and fixed with a test that failed first:
  - High: opening Studio ran `claude -p` in the project folder after 1.5 s (catalog) with no user action (React does the same). Now the catalog
    is fetched only on a send, the model menu or the palette (`Chat.want_catalog`). Test `opening_studio_starts_no_claude_code_until_the_user_acts`.
  - Medium: 3,000 `>` in a reply overflowed the stack (test aborted: "has overflowed its stack"). Quote nesting and inline markers capped at 8
    (`MAX_QUOTE_DEPTH`, `MAX_INLINE_DEPTH`). Test `deep_quotes_and_markers_do_not_overflow_the_stack` passes.
  - Medium: the permission card cut Edit at 80 lines with no marker while Allow approves all. Now "… N more lines not shown" / "… N more
    characters not shown". Test in `tools::tests` (failed first on the missing count).
  - Low: "Always allow" named one rule but applies all. Label now adds "and N more" (failed first: "Always allow Bash ls:*").
- security round 2: the four fixes confirmed (quotes: 3,000 and 100,000 `>` parse in 30 / 144 ms). Remaining medium: the first model-menu /
  palette click or first message runs `claude -p` in an untrusted repo with no trust check. Catalog part fixed in core: `studio_chat_catalog`
  now adds `--setting-sources user --strict-mcp-config` (flags checked in `claude --help`, 2.1.330). Proof in a temp repo with a project
  SessionStart hook: plain catalog run -> hook ran=yes; with the flags -> hook ran=no, the initialize answer still lists models. The first
  message itself still runs project hooks by design (D-012, React too). Boss on the REVIEW card 2026-11-23: "percayai langsung aja, kan
  sudah dijadikan app juga" + APPROVE -> no trust question (D-023).
- qa round 1 (`17-qa.md`, FAIL: 1 must, 2 should, 1 nice), each checked against the code:
  - Must: a mode picked between turns was undone after the restart (the old program's "plan" was still in `state.mode`; `follow_mode` applied
    it). Test `a_mode_picked_between_turns_survives_the_restart` failed (Plan vs Manual); fix: follow only a change of the reported mode, as React. Passes.
  - Should: the Studio window (1440x900 from step 8a) opened partly below a 1080 px screen, hiding the composer; likely my dead "/" clicks
    (clicks below the screen). Now `placement`: at most 90% x 80% of the monitor, centred on the main window, fixed at open. Test
    `the_studio_window_fits_a_1080_screen_and_opens_over_the_main_window`.
  - Should: F-008 said the terminal is the first tab; updated (and F-007 for the placement).
  - Nice: History menu first drawn cut off: not changed (ruling below).
  - qa added `tests/chat_qa.rs` (mode chip after a conversation; pasted screenshot cost, ignored): kept.
- chat tests after all fixes: `--lib -- chat:: hostile` 31 passed, 1 ignored; `--test chat` 9 passed, 1 ignored.

## Rulings
- Markdown: port Markdown.tsx's own parser instead of adding egui_commonmark (named in the M20 plan) | same output as React, no new dependency | a Markdown case the parser misses (it already misses the same in React)
- Success messages (export copied): a short-lived note in the chat header, not a toast | Studio windows have no toast stack, only the error banner | low: a message seen for 3 s
- Streaming frame target missed (17.4 ms max at the end of ONE 60 KB streamed message; median 8.2 ms; a 1,000-item chat is 1.1 ms): accepted | a reply that long in one text block is rare, the cost is one slow frame at the end, and the fix (re-parsing only the tail block while streaming) is new code | a visible hitch on very long answers; upgrade: parse incrementally from the last block
- Link pattern now excludes `[` and caps label 500 / url 2,048 chars (React keeps the old one) | keeps the per-frame parse linear on hostile text (793 ms -> 10 ms) | a link with `[` in its url or label, or longer than the caps, shows as plain text
- Explorer file paste read with `GetKeyState` (Ctrl+V) + `arboard` file list in the composer, not a winit message hook | egui-winit swallows Ctrl+V without an event when the clipboard holds no text; this is a few lines in one place and worked in the real window | a Ctrl+V released before the frame runs is missed (paste again); upgrade: `with_msg_hook` in main.rs
- Studio placement not clamped to the screen edge (qa round 2 Nice): left | egui gives the monitor size but not its origin before the window exists, so a clamp is wrong on a second monitor; centring on the main window covers the usual case | a main window parked at the taskbar puts the Studio composer under it; upgrade: clamp after creation from the viewport's own outer rect
- qa round 2: PASS WITH NOTES (`--lib -- chat::` 34 passed, `--test chat` 10, `--test chat_qa` 1). Studio opened at 0,24-1456,927 on 1080 px.
  Not proven either way: the first toolbar click after a sent message did nothing in 2 of 2 of qa's driven runs (0 in kittest, my 1 try worked);
  suspected driver artifact: every driven action taps Alt to win the foreground, which puts a Windows window in menu mode that eats the next click.
- History menu drawn cut off for its first frame (qa Nice): left as is | an egui popup sizes itself one frame late when its content arrives; it corrects itself on the next frame | a one-frame flicker
- Catalog no longer fetched on open, only after a send / model menu / palette (security High) | `claude -p` in a cloned repo can run its hooks and MCP servers; nothing should start without the user | the model menu shows the built-in aliases until first opened. React keeps the old behaviour until it is removed in step 9
- First tab: the chat when Claude Code is installed (as React), no longer a Claude Code terminal | D-022 parity; the terminal stays one click away | boss may prefer the terminal first: a one-line change
