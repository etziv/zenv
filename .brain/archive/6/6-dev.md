# 6 Version 1.0.0 and GitHub repo
## Plan
Goal: the boss: "buat dan versi sekarang adalah versi 1" (2026-11-09): a GitHub repo for Zenv and the app at version 1.0.0.
Steps: 1) version 0.1.0 -> 1.0.0 in package.json, package-lock.json (root), tauri.conf.json, src-tauri/Cargo.toml, core/Cargo.toml,
Cargo.lock (cargo check). 2) private `etziv/zenv`, origin, main pushed once. 3) releaseMode merge (D-016).
Files: the manifests above, .brain/project.json. Risks: line endings (three manifests are CRLF in git); a secret pushed to GitHub.
## Test plan
- unit: none (no logic); `task.mjs check` on ship.
- review focus: diff is one line per manifest (CRLF kept); no secret file or token in the tracked tree before the first push;
  no leftover "0.1.0" in code or docs.
## Test results
- step 1: `git diff --stat` 6 lines changed in 5 manifests after restoring CRLF (`git ls-files --eol` i/crlf w/crlf); `cargo check` Finished,
  Cargo.lock 2 lines; `git grep 0.1.0` in src, src-tauri/src, core/src, README, docs, installer, scripts: none.
- step 2: tracked-file scan for .env/keys/tokens: none (only the redaction regexes in memory.mjs); `gh repo view etziv/zenv`: PRIVATE main.
- final: `task.mjs check` on ship.
## Rulings
- `gh repo create` + one `git push -u origin main` by hand | task.mjs creates only the local repo; the boss asked for the GitHub repo | none, one-time bootstrap
- releaseMode merge, not push | push mode never updates main on GitHub | a ship waits for the "no checks" grace period (no CI yet)
