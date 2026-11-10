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

## D-016 Private GitHub repo etziv/zenv, release mode merge, version 1.0.0 [superseded by D-019] 2026-11-09
Decision: code lives in private `etziv/zenv`; every ship pushes, opens a PR and squash-merges into `main`; the app is version 1.0.0.
Source: the boss, 2026-11-09 ("buat dan versi sekarang adalah versi 1"); task 6.

## D-017 Self-update with an encrypted read-only token [superseded by D-018] 2026-11-09
Decision: Zenv updates itself from a private GitHub source using a read-only token stored encrypted in the app (not yet built; design
in a PROPOSAL card). Source: the boss, 2026-11-09 ("gunakan token read untuk update tpi tokennya dienkripsi").

## D-018 No self-update for now [active] 2026-11-09
Decision: Zenv has no self-update; builds are installed by hand (D-009). Options weighed for later: token in the exe (only obscured),
DPAPI token pasted once, a public releases repo. Source: the boss, 2026-11-09 ("self update tidak perlu dulu deh, pikir nanti").

## D-019 Public GitHub repo etziv/zenv, release mode merge, version 1.0.0 [active] 2026-11-09
Decision: `etziv/zenv` is public (full history; scanned for secrets first: none); every ship pushes, opens a PR, squash-merges into main.
Source: the boss, 2026-11-09 ("repo buat publik aja dulu"), after D-016. No license file yet (all rights reserved by default).
Installers are GitHub Releases (`v1.0.0` = Zenv-1.0.0-setup.exe, 2026-11-09, the boss: "izinkan").

## D-020 Own private network (Noirlynx): a separate app; Zenv integrates it later like Tailscale [active] 2026-11-09
Decision: the boss's own Tailscale-like network (control server + public console on his Linux server, WireGuard clients) is its own app,
designed without Zenv (`docs/jaringan-privat.md`). Its client only joins the device to the network (no "open app" menu); opening a local
app stays in Zenv, which calls the client's CLI/JSON interface as it calls `tailscale serve` today. Source: the boss, 2026-11-09
("di rancangan tidak usah kamu masukan zenv", "membuka app local tetap lewat zenv, client hanya menghubunkan device dengan jaringan").
Name: Noirlynx, CLI `noirlynx` (the boss, 2026-11-10: "kasih nama sistem jaringan privat noirlynx").
Stack (the boss, same day): all Rust; console = Rust backend + React; desktop client native without a webview ("base codenya pakai rust").

## Gotchas (max 30 lines; delete when no longer true)
G-001 2026-11-04 Do not put `/D=` before other NSIS flags: it must be last (`/S /D=C:\zenv`); the wrong order created a stray `C:\zenv R`. Leave out `/R`: it relaunches Zenv as the shell's child (G-003).
G-002 2026-10-19 Do not expect to rewrite the hosts file: Kaspersky refuses even elevated; appending works, so the helper only appends missing lines.
G-003 2026-11-06 Do not launch Zenv from a Claude Code shell: children inherit `CLAUDE_CODE_CHILD_SESSION` (transcripts off). Terminals and chats strip it; open Zenv from the Start menu.
G-004 2026-11-06 Do not close the boss's running Zenv to install a build: check `zenv.exe` first and ask (the boss's rule).
G-005 2026-11-09 Do not filter Claude transcript entries by type before walking `parentUuid`: `attachment` entries are links in the chain (`transcript_lines`).
G-006 2026-11-06 Do not run Vitest while a cargo build runs (also right after a Cargo.toml edit, likely rust-analyzer's rebuild): workers time out ("Timeout waiting for worker"); rerun once it settles.
G-007 2026-11-06 The windows-gnu link prints `.rsrc merge failure: multiple non-default manifests`; the app still builds and runs.
G-011 2026-11-09 Do not debug a one-off rustc "crate `X` required in rlib format" or "can't find crate for `std`" as code: rerun (passed each time; cause unproven, likely antivirus file locks).
G-010 2026-11-09 Do not start Zenv's servers (mysqld from `C:\zenv\bin`) from a shell: they outlive the session and Zenv's own Start fails "port in use".
G-009 2026-11-09 Do not register Windows scheduled tasks yourself: auto mode refuses it as persistence (task 3); hand the boss the command.
G-008 2026-11-06 Do not hardcode Claude model ids: the alias `haiku` already resolved to `claude-haiku-5-5`; use the catalog (D-013).
