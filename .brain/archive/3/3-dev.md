# 3 Follow a renamed tailnet automatically
## Plan
Goal: after the tailnet name is re-rolled (or the PC renamed), the boss asked that what can follow by itself does ("yang bisa aja", 2026-11-09):
Zenv serves the new names without opening the tailnet panel, and Vaultwarden's DOMAIN follows the new name.
Steps: 1) `Core.follow_tailnet_names` (the name check that `tailnet_apps` runs today, extracted) and a background thread in `lib.rs`
that calls it every 60 s. 2) e2e test: stale saved names -> the real Service name refused; after the call it answers.
3) Vaultwarden (outside the repo): compose reads `TAILNET` from `.env`; `sync-domain.ps1` writes it from `tailscale status` and
recreates the container on a change; a Task Scheduler entry runs it at logon and every 15 min.
Files: src-tauri/core/src/tailnet.rs, src-tauri/src/lib.rs, src-tauri/core/tests/tailnet_e2e.rs; D:\Hanif\vaultwarden\{docker-compose.yml,.env,sync-domain.ps1}.
Risks: `tailscale status` every 60 s (cheap, only while an app is open on the tailnet); offline Tailscale must not wipe names (status().online guard kept).
## Test plan
- e2e `renamed_tailnet_names_are_followed` (Nginx + real Tailscale): saved names stale -> `svc.<real tailnet>` gets 421/403;
  after `follow_tailnet_names` -> the proxy app answers that name.
- unit: `npm run test:rust` stays green; `cargo check`.
- review focus: no app open on the tailnet -> no `tailscale` call; Tailscale offline -> names kept; script with Tailscale stopped -> no change;
  script run twice -> second run changes nothing (no container restart).
- device: run sync-domain.ps1 against the live container: .env written, DOMAIN unchanged value, container not recreated on the 2nd run.
## Test results
- step 1-2: `cargo test -p zenv-core --test tailnet_e2e renamed_tailnet_names_are_followed -- --ignored` (Nginx 1.22.0, real Tailscale)
  -> 1 passed: `svcrename.caracal-alnair.ts.net` refused with stale names, answers after `follow_tailnet_names`, old name refused.
- step 3: sync-domain.ps1 run 1 -> .env `TAILNET=caracal-alnair.ts.net` (LF), container kept; run 2 -> no change, container Created unchanged;
  rename simulated (.env old-name, DOMAIN old) -> script restored .env, container recreated with DOMAIN caracal-alnair, /alive 200.
- final: first `task.mjs check` failed to compile untouched `nginx_e2e.rs` ("cannot find type String", E0463) right after the e2e build;
  `npm run test:rust` then 237 passed / 0 failed, and `task.mjs check --fresh`: build, build-rust, lint, test, test-rust all OK.
- qa round 1 (3-qa.md): PASS WITH NOTES. Fixes: qa's offline test kept as `tests/tailnet_follow.rs` (default suite); e2e "before" now
  requires 421/403. Re-run: tailnet_e2e rename 1 passed, tailnet_follow 1 passed.
- scheduled task: registration denied by the auto-mode classifier (persistence); handed to the boss as a command.
## Rulings
- Re-check every 60 s in a thread, not a Tailscale event hook | `tailscale status` is cheap and only runs while an app is open | a rename is served up to 60 s late
- No qa round 2 | the Should fix was committing qa's own test unchanged, proven by the run above | a missed issue in a 2-line test edit
- qa Nice-to-have 2 (reload rollback restarts a server stopped at the same instant) not fixed | behaviour predates task 3, window tiny | a stopped server comes back once
- Vaultwarden DOMAIN from a script + scheduled task, not Zenv | Zenv does not own the app's Docker config; generic hook would be new API | the boss runs one command to register it
