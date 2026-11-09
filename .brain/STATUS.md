# STATUS (edit the lines that change, max 40 lines)

Updated: 2026-11-09

## Current state
- Adopted into the kit 2026-11-09: ARCHITECTURE, BRIEF, MODULES, FLOWS, DECISIONS, DESIGN written from the code; approved by the boss 2026-11-09.
- Verify (2026-11-09): typecheck ok, Vitest 43/43, `npm run test:rust` 237 passed / 5 ignored, `cargo check` ok, `tailnet_e2e` 2/2.
- Installed build in `C:\zenv`: 2026-11-09 08:53 (Tailscale Services, file icons, Studio chat palette, history).
- Git: the `[setup] initial commit` holds all code as of 2026-11-09; earlier history is not in git.

## Next steps
- Vaultwarden on the tailnet: the boss sets the policy (`tag:server`, autoApprovers, grant), defines `svc:vaultwarden`, tags this PC;
  then Open on tailnet with Own name, set Vaultwarden `DOMAIN` to `https://vaultwarden.caracal-alnair.ts.net`, restart it, set `SIGNUPS_ALLOWED` false after signup.
- Studio chat: an end-to-end run in the Studio window (palette terminal screens, paste from Explorer, MCP toggle).

## Open issues
- Tailscale Services not yet proven against the real tailnet (M13).
- Possible bug (found by qa 2026-11-09, unfixed): `ServiceManager.running_rivals` also returns non-web services, so `Core.start_service`
  can pick a database on the same port as a web server's front (`rivals.first()`); the front then forwards nothing. Only when a database shares the web port.
- No automated test for the tray panel (M18).
- Code signing / self-update for a public release undecided (BRIEF open questions).
