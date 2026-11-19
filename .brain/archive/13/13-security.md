# Task 13 (M20 native egui UI): security review

## Plan
- Scope: working tree vs `main` (`src-tauri/**` diff) + untracked `src-native/**`, only the 5 surfaces in the brief.
- References: `supply-chain-ci.md` (deps, lockfile), `availability.md` (image parsing limits). No earlier security reports exist for these files (`.brain/archive/*/*-security.md`: none).
- Checks:
  1. `src-native/Cargo.toml`, lockfile delta: crate sources, versions, feature unification of `image`; `cargo audit` on HEAD and on `main` lockfile, compared.
  2. Untrusted input: `widgets::data_url_bytes`, every `Image::from_bytes` caller, where thumbnails/icons come from (`core/thumbnails.rs`, `core/icons.rs`), egui_extras/image decode limits; log viewer (`log_panel.rs`) vs `log_manager::tail` caps; hosts dialog vs core `hosts_apply`.
  3. Process/path sinks: `os::reveal` and every caller, `Action::OpenUrl` and every producer, `file_url`, eframe -> `webbrowser` 1.2.7 Windows launch path, editor/terminal calls (ids only), rfd pickers, clipboard.
  4. Core: `Core::spawn_background` vs removed Tauri code, `icons.rs` + `atomic_write`, `storage::retry_transient_io`.
  5. eframe persistence: what `save` writes, the live `%APPDATA%\Zenv\data\app.ron`, egui `TextEditState` serde.

## Evidence
- Lockfile delta: 213 new packages, all `registry+https://github.com/rust-lang/crates.io-index` (206 source lines, no git/path sources). eframe/egui/egui_extras/egui_kittest 0.37.0, image 0.25.10, rfd 0.18.0, webbrowser 1.2.7, arboard 3.7.0, ron 0.12.3.
- `cargo tree -i image` for zenv-native: features `bmp,png` only (no jpeg/tiff/webp decoders in the shipped binary).
- `cargo audit` (fresh DB, 1533 advisories): HEAD 8 vulnerabilities + 8 warnings; `main` lockfile: the same 8 IDs (zip 2.6.1 x6, libsqlite3-sys 0.38.2, webpki-root-certs 1.0.9) and the same warnings. The delta adds no advisory.
- Image decode: egui_extras 0.37 uses `ImageReader` with image's default `Limits` (`max_alloc` 512 MiB). Inputs are Zenv's own PNG writer (`png_rgba`), Edge `--screenshot` PNGs, and toolchain-shipped logos; no path for page or network content to supply PNG bytes.
- Logs: `log_manager::tail` caps 512 KiB / 2000 lines; native renders each line as a plain `ui.label` (no markup, links or escape handling). Project app log = newest `*.log` under `storage/logs`, canonicalized and confined to the project root (`local_sites/logs.rs:109-120`).
- OpenUrl producers: `site.url`, `status.url`, tailnet `url`, `core.router_url`, `file_url(<log or Zenv path>)`. eframe -> `webbrowser::open` -> `AssocQueryStringW(http)` and `Command::new(browser).args(..)` with the URL as one percent-encoded argument (no shell, no ShellExecute on the file): a file URL opens in the browser, not in its file association.
- `os::reveal`: `explorer [/select,]<path>` via `Command` (no shell), only after `path.exists()`; paths are Zenv/home paths, project root, document root, data dir, installation root, config file.
- Persistence: `save` stores only `zenv.route`; live app.ron (2979 bytes) holds egui options, scroll/area geometry and the route; 0 hits for password/token/key; egui `TextEditState.undoer` is `serde(skip)`, so typed text is not persisted.
- `spawn_background` is the removed Tauri code moved 1:1 (router thread now holds a `Weak` and drops the `Arc` before sleeping). `atomic_write` creates the parent dir, writes a unique temp file and renames. `retry_transient_io` retries only `SystemIoFailure`, 30 x 100 ms, then returns the error.

## Findings
None.

Rejected:
- PNG/data-URL decompression bomb: inputs are produced by Zenv, Edge's screenshot or installed toolchains, and image's 512 MiB `max_alloc` applies; no less-trusted actor controls the bytes.
- Log text injection (spoofed UI, escape sequences, giant lines): egui labels render text only; the tail is capped at 512 KiB / 2000 lines by core.
- "Open file" launching an executable via `file://`: webbrowser hands the URL to the default browser exe as one argument; log sources are `*.log` only and the app log is confined to the project root.
- Argument injection into the browser or explorer: `Command` args, URL percent-encoded by `url`; explorer path must exist and comes from Zenv's own records.
- `os::reveal(path, false)` running a file: every caller passes a directory Zenv records (project/document root, data dir, install root); turning one into a file needs the same user who already controls the machine.
- Hosts dialog shows a plan that may differ from what `hosts_apply` writes at click time: both come from the user's own sites in the local DB; no other actor changes it, and core recomputes and validates (core unchanged).
- Persisted secrets in app.ron: only route, egui options and geometry (checked on the live file).
- spawn_background / icon cache / DB open retry: same code or narrower; no input crosses a trust boundary; retry is bounded (3 s).
- New crate advisories: none; the 8 open ones exist identically on `main`.

Hardening (not counted):
- `os::reveal`: when `select` is false require `path.is_dir()` (else fall back to `/select,`), so a path that became a file is never opened by explorer.
- The native binary has no single-instance guard (the Tauri shell uses `tauri-plugin-single-instance`): two instances run autostart and router auto-update twice on the same home. Reliability rather than security; worth a named mutex before the step-9 rename.
- `file_url` does not percent-encode (`#`, `%`, `?` in a path open the wrong URL); `url::Url::from_file_path` or passing the path to webbrowser fixes it.
- Pre-existing, not from this task: zip 2.6.1 (RUSTSEC-2026-0367/0368/0369/0429/0545/0548, symlink escape in `extract`), libsqlite3-sys 0.38.2 (RUSTSEC-2026-0459), webpki-root-certs 1.0.9 (RUSTSEC-2026-0383): a separate task should check whether core's archive extraction reaches the zip paths.

Findings: high 0, medium 0, low 0
Safe to proceed: yes
Not checked: unchanged core functions behind the UI (`hosts_apply` elevation, `open_in_editor`, `open_terminal`, `router_url`, site/domain validation) were not re-audited; explorer/browser launches were traced in source (eframe -> webbrowser 1.2.7) but not run live; non-Windows rfd/arboard backends; egui_kittest (dev-only) deps beyond the audit.
