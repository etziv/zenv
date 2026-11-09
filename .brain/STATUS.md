# STATUS (edit the lines that change, max 40 lines)

Updated: 2026-11-09

## Current state
- Adopted into the kit 2026-11-09: ARCHITECTURE, BRIEF, MODULES, FLOWS, DECISIONS, DESIGN written from the code; approved by the boss 2026-11-09.
- Verify (2026-11-09): typecheck ok, Vitest 43/43, `npm run test:rust` 237 passed / 5 ignored, `cargo check` ok, `tailnet_e2e` 2/2.
- Installed build in `C:\zenv`: 2026-11-09 16:59, version 1.0.0 (task 6), installed without `/R`.
- Git: GitHub `etziv/zenv` (public since 2026-11-09), release mode merge (D-019); history before 2026-11-09 is not in git.

## Next steps
- Vaultwarden on the tailnet: live 2026-11-09 (`svc:vaultwarden`, this PC `tag:server`; reached from another device). From this PC use
  `vaultwarden.localhost` (tagged, not `autogroup:member`). DOMAIN follows the tailnet name via `D:\Hanif\vaultwarden\sync-domain.ps1` and the
  scheduled task "Vaultwarden tailnet domain" (logon + 15 min, proven 2026-11-09). `SIGNUPS_ALLOWED` false.
- Studio chat: an end-to-end run in the Studio window (palette terminal screens, paste from Explorer, MCP toggle).

## Open issues
- Possible bug (found by qa 2026-11-09, unfixed): `ServiceManager.running_rivals` also returns non-web services, so `Core.start_service`
  can pick a database on the same port as a web server's front (`rivals.first()`); the front then forwards nothing. Only when a database shares the web port.
- No automated test for the tray panel (M18).
- 2026-11-09: Start MySQL 9.7 failed "port 3306 in use by mysqld.exe": Zenv's own mysqld (same `--defaults-file`, started 2026-11-06,
  parent process gone) outlived its Zenv; shut down with `mysqladmin shutdown`. Likely cause: started by hand from a Claude shell, not by Zenv:
  a second leftover, MySQL 8.0 on 3307 (`eda3640f…` my.ini), was started 2026-11-06 13:28 by a Claude Code bash command (its parent): the ebphtbv3 project's test database (`ebphtb_v3_*_test`).
  Shut down 2026-11-09 (`mysqladmin shutdown`, 0 clients); the boss: "nyalakan lagi lewat zenv" (Zenv service "MySQL 8.0").
- Code signing undecided; self-update postponed (D-018).
- 2026-11-09 16:14: MySQL 9.7 (3306) crashed, exception 0x80000003 inside `ALTER TABLE document_requirements ADD confirmation_id`
  (ebphtbv3 migration 2026_04_20_041149; its phpunit.xml sets no DB_PORT, so tests use 3306 while 3307 is down). A MySQL 9.7 bug in the
  in-place ALTER; InnoDB recovers on the next start. 45 "redo log ... increase innodb_redo_log_capacity" warnings that day (heavy test load).
