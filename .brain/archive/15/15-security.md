# 15 security review: native Studio (file tree + editor), M20 step 8a

## Plan
Scope: uncommitted change on task/15 (`git diff` + untracked `src-native/src/studio/`, `src-native/tests/studio.rs`, `src-native/assets/`).
References: `supply-chain-ci.md` (new crate), `availability.md` items via direct trace (regex over untrusted file text), core-classes file-path rules via trace.
Checks:
1. syntect 5.3.0 / fancy-regex 0.16.2 / bincode 1.3.3: `cargo audit` on `src-tauri/Cargo.lock` (read-only), build scripts, where the syntax dump comes from, syntect's regex error and panic paths, our caller's error handling.
2. Every file path in `studio/*.rs`: only through `zenv_core::project_files` with the root from the site record (same as Tauri's `Core::project_root`).
3. Core actions from Studio: `run` (reveal folder, open in editor), `open_url`, `format_code` / `install_formatter` threads: what input reaches them.
4. Panic surfaces in new code on hostile file content (byte/char slicing, find, highlighter offsets).
5. Icon atlas: embedded, licence present.

## Evidence
- `cargo audit --file src-tauri/Cargo.lock`: no vulnerability in syntect, fancy-regex or bincode; bincode 1.3.3 has only the "unmaintained" warning RUSTSEC-2025-0141. The other hits (zip, url, libsqlite3-sys, webpki-root-certs) were in the tree before this change.
- syntect `Cargo.toml` has `build = false`; `SyntaxSet::load_defaults_newlines` decodes `include_bytes!("../assets/default_newlines.packdump")`, the crate's own data compiled in. bincode never sees runtime input.
- syntect `parsing/regex.rs` (fancy backend): search errors, catastrophic backtracking included (the fancy-regex backtrack limit), count as "no match", not a panic. The `unwrap()`s on `regions.pos(0)` in `parser.rs` run only after a successful match, where group 0 is always set. Our `highlight.rs:172` uses `parse_line(..).unwrap_or_default()`, so a parse error leaves the line plain.
- Paths: `editor.rs` (read/write/stat), `tree.rs` (list/create), `quick_open.rs` (search) all call `project_files::*` with `site.root` = `SiteView.site.project_root`, the same value `Core::project_root` returns to the Tauri commands. No `std::fs` and no path joins outside project_files in `studio/`. The new-file name goes through `trim`, `\`->`/` and leading-`/` strip, then `project_files::create`, which checks it (the dev's test refuses `..`).
- Find: `to_ascii_lowercase` keeps byte lengths, so the offsets match the original text and `text[byte..b]` stays on char boundaries. `byte_at` and `char_indices` are boundary-safe. Mutex poisoning is handled (`unwrap_or_else(into_inner)`).
- Core actions get only values that do not come from files: the site id, the editor id from the detected list, the site URL from the site record, and the formatter name from Core's own `AppError::NotFound` id.
- Atlas: `include_bytes!` of our own PNG, index from a static table; the MIT licence text is in `assets/file-icons-LICENSE`.

## Findings
None.

Rejected:
- Path escape through New file/folder, Go to file or save: every path goes through `project_files`, and its root confinement did not change.
- bincode deserialization of hostile data: only the crate's embedded dump is decoded.
- Regex backtracking panic killing the UI: fancy-regex errors become no-match in syntect, and our caller ignores parse errors.
- Command or argument injection through the formatter install: the tool name comes from Core's error id, not from file text.
- Char-boundary panic in find or the match rectangles: ASCII-only lowercasing keeps the offsets valid.

Hardening (not counted):
- Colouring runs on the UI thread. A hostile file at or under 100 KB (for example one minified line built to make a Sublime grammar regex hit its backtrack limit at many offsets) could freeze the Studio window for seconds. Only the user's own window is hit and it ends without data loss, so this is not counted. Option: skip colouring of lines over ~10-20k chars (VS Code does this), or colour on a worker thread.
- bincode 1.3.3 is unmaintained (RUSTSEC-2025-0141). It only reads syntect's embedded data. Track syntect's move off it.
- Advisories that were already in the lockfile (zip 2.6.1 extraction issues, url, libsqlite3-sys, webpki-root-certs) are worth a separate upgrade task.

Findings: high 0, medium 0, low 0
Safe to proceed: yes
Not checked: how long a crafted line actually freezes the window (needs a build plus a timing run; no build was run here, as this review is read-only); `project_files` internals (unchanged, out of scope); the atlas generator script `scripts/file-icons-atlas.mjs` (not in the untracked list); the handoff does not record the bos's chat approval for adding syntect, which the project rules require for a new dependency.
