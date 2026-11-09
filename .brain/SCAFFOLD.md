# SCAFFOLD (one-time setup blueprint, max 80 lines; archived once the setup is proven)

Versions verified: 2026-11-09 (adopted project: the setup already exists; see README "Requirements" and "Commands")

## Manual steps for the boss (ONLY what no command can do)
- None for development. Running real e2e tests needs installed servers (paths in `ZENV_TEST_*`).

## Ordered commands (exact, pinned versions)
1. `npm install`
2. `node scripts/with-toolchain.mjs cargo check --manifest-path src-tauri/Cargo.toml` (Rust 1.99.0 windows-gnu + WinLibs GCC)
3. `npm run tauri dev` (run) or `npm run installer` (installer/Zenv-setup.exe)

## Verification checklist (dev proves it: build, test, smoke run)
- [x] build passes (`npm run build`, `cargo check`, 2026-11-09)
- [x] first test runs (Vitest 43/43, Rust 237 passed, 2026-11-09)
- [x] data connection works (`storage` tests; the installed app uses `C:\zenv\zenv.db`)
- [x] structure as in ARCHITECTURE.md "Folder structure"
