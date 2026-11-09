# STATUS (edit the lines that change, max 40 lines)

Updated: 2026-11-09

## Current state
- Adopted into the kit 2026-11-09: ARCHITECTURE, BRIEF, MODULES, FLOWS, DECISIONS, DESIGN written from the code; approved by the boss 2026-11-09.
- Verify (2026-11-09): typecheck ok, Vitest 43/43, `npm run test:rust` 237 passed / 5 ignored, `cargo check` ok, `tailnet_e2e` 2/2.
- Installed build in `C:\zenv`: 2026-11-09 14:55 (task 3: tailnet names followed in the background).
- Git: the `[setup] initial commit` holds all code as of 2026-11-09; earlier history is not in git.

## Next steps
- Vaultwarden on the tailnet: done 2026-11-09 through the Tailscale API (the boss allowed it, "aku izinkan"): policy has `tag:server`,
  autoApprovers `svc:vaultwarden`, grant members -> svc:vaultwarden:443; Service `svc:vaultwarden` (tcp:443) defined; this PC tagged `tag:server`;
  `D:\Hanif\vaultwarden\docker-compose.yml` DOMAIN = `https://vaultwarden.${TAILNET}`, TAILNET in `.env` written by `sync-domain.ps1` there
  (follows a re-rolled tailnet name; task 3). Its scheduled task "Vaultwarden tailnet domain" is registered by the boss (auto mode refused it).
  Opened in Zenv 2026-11-09 (loopback 9001; Service host `approved:auto`, `ready`); the boss opened it from another device ("oke bisa").
  From this PC the URL times out (tagged, so not `autogroup:member` in the grant); use `vaultwarden.localhost` here. `SIGNUPS_ALLOWED` false (1 account).
- Studio chat: an end-to-end run in the Studio window (palette terminal screens, paste from Explorer, MCP toggle).

## Open issues
- Possible bug (found by qa 2026-11-09, unfixed): `ServiceManager.running_rivals` also returns non-web services, so `Core.start_service`
  can pick a database on the same port as a web server's front (`rivals.first()`); the front then forwards nothing. Only when a database shares the web port.
- No automated test for the tray panel (M18).
- 2026-11-09: Start MySQL 9.7 failed "port 3306 in use by mysqld.exe": Zenv's own mysqld (same `--defaults-file`, started 2026-11-06,
  parent process gone) outlived its Zenv; shut down with `mysqladmin shutdown`. Likely cause: started by hand from a Claude shell, not by Zenv:
  a second leftover, MySQL 8.0 on 3307 (`eda3640f…` my.ini), was started 2026-11-06 13:28 by a Claude Code bash command (its parent). Still running.
- Code signing / self-update for a public release undecided (BRIEF open questions).
