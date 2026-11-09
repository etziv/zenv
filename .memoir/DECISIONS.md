# DECISIONS and GOTCHAS (one file to search; edit in place)

## Decisions (max 3 lines per entry)

## D-001 Zenv never edits a server installation; it validates before activating [active] 2026-10-16
Decision: own config per service under `C:\zenv\etc\<service>`, staged, checked by the server's own checker, then swapped in;
every written file sits in the `managed_files` ledger with a backup. Source: docs/ARCHITECTURE.md "Key design decisions", README "Data and safety".

## D-002 No project profiles or Stacks [active] 2026-10-19
Decision: each service is switched on/off by the boss; the Projects/Stacks menu was removed. Do not reintroduce unless asked.
Source: the boss, 2026-10-19.

## D-003 A project's .env is read-only for Zenv [active] 2026-10-19
Decision: Zenv reads ports from `.env` and passes it to app programs with port vars overridden; it never writes it.
Source: the boss, 2026-10-19 ("biar baca env aja").

## D-004 Web servers on the same port run together [active] 2026-10-19
Decision: the first one started holds the port, later ones run behind it on an internal port (`Core.start_behind`); no rival swap.
Source: the boss, 2026-10-19 ("franken ya franken, apache ya apache").

## D-005 Tailscale instead of a public tunnel [active] 2026-10-21
Decision: the Cloudflare tunnel ("Share") was removed; apps open to the boss's devices through `tailscale serve` only.
Source: the boss, 2026-10-21 (the tunnel did not work for them).

## D-006 Zenv starts proxy apps' programs; detected commands need approval [active] 2026-10-31
Decision: start commands run with the web server / Start apps; a detected command waits for approval (migration 0013). Auto-start stays on
for wablast.test and retribusi-h2h.test. Source: the boss 2026-10-19, hardening 2026-10-31, "biarkan auto start aja" 2026-11-04.

## D-007 OmniRoute stays next to 9router [active] 2026-10-23
Decision: both AI routers remain installable Tools services (`ai_router.rs`), though the boss prefers 9router. Source: the boss, 2026-10-23.

## D-008 Windows first; macOS later [active] 2026-10-16
Decision: build and test for Windows only; a macOS port (not iOS) starts only when the boss asks. Source: the boss.

## D-009 One installer location [active] 2026-10-23
Decision: `npm run installer` copies the NSIS bundle to `installer/Zenv-setup.exe`; "pasang" means install that file into `C:\zenv`. Source: the boss, 2026-10-23.

## D-010 Studio layout: Claude to the right of the editor [active] 2026-11-06
Decision: Files | Editor | Claude | Preview with draggable splitters (saved in localStorage); the empty editor can be closed so Claude
fills the width until a file opens. Source: the boss, 2026-11-06.

## D-011 "Open in Studio" is the wide button in the app panel [active] 2026-11-06
Decision: the editor button (VS Code, ...) is the compact one beside it (`EditButtons`). Source: the boss, 2026-11-06.

## D-012 Studio chat runs the official Claude Code over pipes [active] 2026-11-06
Decision: `claude -p` in stream-json, started lazily, trimmed in Rust (`shape`), drawn by React; not a reimplementation (that would need an
API key instead of the boss's Claude login). Source: the boss asked for the VS Code-like UI "seringan mungkin", 2026-11-06.

## D-013 Model and command lists come from Claude Code itself [active] 2026-11-06
Decision: the `initialize` control request (no tokens) lists models with versions and commands (`studio_chat_catalog`); cached a day.
Source: the boss, 2026-11-06 ("opus 5.5 atau opus 5 jadi tidak jelas").

## D-014 File-type icons: a vendored subset of Material Icon Theme [active] 2026-11-06
Decision: only the SVGs mapped in `fileIcons.ts` are copied to `public/file-icons` (MIT license kept); no npm dependency. Source: the boss, 2026-11-06.

## D-015 Per-app names on the tailnet through Tailscale Services [active] 2026-11-09
Decision: an app may be a Service (`https://<svc>.<tailnet>.ts.net`, migration 0014); the port mode stays. Needs a tagged host, HTTPS
certificates and the Service defined in the admin console. Source: the boss, 2026-11-09 ("coba tailscale service tambahkan").

## Gotchas (max 30 lines; delete when no longer true)
G-001 2026-11-04 Do not put `/D=` before other NSIS flags: it must be last (`/S /R /D=C:\zenv`); the wrong order created a stray `C:\zenv R`.
G-002 2026-10-19 Do not expect to rewrite the hosts file: Kaspersky refuses even elevated; appending works, so the helper only appends missing lines.
G-003 2026-11-06 Do not launch Zenv from a Claude Code shell: children inherit `CLAUDE_CODE_CHILD_SESSION` (transcripts off). Terminals and chats strip it; open Zenv from the Start menu.
G-004 2026-11-06 Do not close the boss's running Zenv to install a build: check `zenv.exe` first and ask (the boss's rule).
G-005 2026-11-09 Do not filter Claude transcript entries by type before walking `parentUuid`: `attachment` entries are links in the chain (`transcript_lines`).
G-006 2026-11-06 Do not run Vitest while a cargo build runs: its workers time out ("Timeout waiting for worker"); run it afterwards.
G-007 2026-11-06 The windows-gnu link prints `.rsrc merge failure: multiple non-default manifests`; the app still builds and runs.
G-008 2026-11-06 Do not hardcode Claude model ids: the alias `haiku` already resolved to `claude-haiku-5-5`; use the catalog (D-013).
