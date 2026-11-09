# 3 Follow a renamed tailnet automatically: QA
## Plan
Scope: uncommitted diff of `src-tauri/core/src/tailnet.rs`, `src-tauri/src/lib.rs`, `src-tauri/core/tests/tailnet_e2e.rs`, FLOWS F-006 step 5.
Checks: entry gate (task status, `task.mjs check`); criteria 1-4 by running the e2e myself plus a new QA test for the offline guard;
code read of every caller of `SiteManager.reapply` / `known_hosts`; cost of the 60 s loop measured; test integrity; memory verify.
Charter (riskiest areas): 1) the thread's reapply racing UI reapply / `tailnet_open` / `tailnet_close` / service stop / app exit;
2) Weak<Core> upgrade and thread lifetime; 3) offline/logged-out Tailscale wiping names; 4) whether the e2e really proves stale -> followed.
Load assumed: one desktop user, 1-10 apps open on the tailnet, a few web servers; renames are rare (days to months apart).
Constraint: Zenv not launched from this shell (G-003); the boss's tailnet only read (`tailscale status`), never served.

## Evidence
- Gate: `task.mjs status` shows only task 3's 7 files; `task.mjs check` -> OK (reused, same code passed 05:39 UTC).
- C1: `ZENV_TEST_NGINX_ROOT=C:\zenv\bin\nginx\1.22.0 cargo test -p zenv-core --test tailnet_e2e renamed_tailnet_names_are_followed -- --ignored`
  -> 1 passed (2.69 s): `svcrename.caracal-alnair.ts.net` refused with stale names, answers after `follow_tailnet_names`, `svcrename.tail1234.ts.net` refused after.
  No nginx.exe left running afterwards (Win32_Process query empty).
- C2 (offline): new QA test `src-tauri/core/tests/tailnet_follow_qa.rs` (Tailscale hidden via PATH/ProgramFiles, stale names saved):
  no app open -> names unchanged; app open + offline -> `follow_tailnet_names` and `tailnet_apps` both keep the names -> 1 passed.
- C2 (no call without an app): code read `tailnet.rs:L310-315`: `COUNT(*) FROM tailnet_apps` > 0 before `status()`; only a SQLite count otherwise.
- C3: `tailnet_apps` (L295-306) still reads rows, one `status()`, follows names only with rows, maps with the same `st`: same behaviour as before.
- C4: thread body (`lib.rs:L40-49`) logs the `Err`; `follow_names` logs each failed reapply and continues; `SiteManager.guard` recovers a
  poisoned mutex. Weak upgrade always succeeds while Tauri state holds the Arc, so the loop lives until process exit. No panic path found (code read only).
- Concurrency: every `reapply` takes `SiteManager.guard` and `apply_service` reads names and `tailnet_apps` from the DB inside it
  (`virtual_host.rs:L216`), so the last apply always renders current DB state; a double reapply (thread + panel) renders identical files -> no reload.
  `tailnet_open`/`close` write the DB before their own reapply: no lost update. No lock is held across `status()` in the new code: no deadlock.
- Cost: `tailscale status --json` measured 147 / 545 / 175 ms (3 runs), hidden console (`run_capture` -> `hide_console_window`); at most once per 60 s, only with an app open. Negligible.
- Memory: `memory.mjs verify` -> F-006 step 5 matches the code; its `[!] follow_tailnet_names not in graph` is the stale graph (built at 1f08494), the fn is at `tailnet.rs:L310`; "60 s" is `lib.rs:L43`.

## Findings
**Should fix**
1. The new function has no test in the default suite (`src-tauri/core/tests/tailnet_e2e.rs:L245-281`)
   - What this is: the only test of `follow_tailnet_names` is `#[ignore]` and needs real Tailscale + Nginx, so `npm run test:rust` never runs it.
   - Problem: a change that drops the `COUNT > 0` skip or the offline guard stays green in CI; criterion 2 was only proven by QA's new test.
   - Fix: keep `src-tauri/core/tests/tailnet_follow_qa.rs` (written by QA, runs without Tailscale, 0.08 s) in the commit.
   - If we skip it: a regression that wipes saved names when Tailscale is off goes unnoticed until a tailnet app stops answering.

**Nice to have**
2. Background reapply can restart a server the user just stopped, or one stopped at exit (`src-tauri/core/src/virtual_host.rs:L282-289`)
   - What this is: when a reload fails, `apply_service` restores files and `services.start`s a server that is no longer running.
   - Problem: a rename detected by the thread in the same seconds the boss clicks Stop on Nginx (or quits Zenv, `shutdown_all`) -> reload hits a
     stopped process -> rollback starts it again. Pre-existing for every reapply; this change adds an unattended trigger, but only on a rename, so the window is tiny.
   - Fix: none now; if it ever shows up, skip the restart when the server was stopped on purpose (not in this task).
   - If we skip it: very rarely a web server comes back up after a stop, or keeps running after exit.
3. The e2e's "before" check accepts any non-app answer (`src-tauri/core/tests/tailnet_e2e.rs:L273`)
   - What this is: `!get(..).contains("program host=")` passes on a 502 or 500 as well as on the host filter's 421/403.
   - Problem: the stale state is not pinned to the host filter; the after-state still proves the follow, so the test is valid, only less precise.
   - Fix: also assert the status code is 421 or 403, as `assert_refused_elsewhere` does.
   - If we skip it: a broken proxy before the follow would be read as "refused".

Verdict: PASS WITH NOTES
Safe to proceed: yes
Not checked: the `tailnet-names` thread inside the running app (Zenv not launched, G-003) so the real ~60 s timing is code-read only; a real tailnet rename (simulated with stale saved names); "no tailscale call without an app" proven by code read, not by counting process spawns; Apache path of the rename (Nginx only); no-panic of the thread (code read).
