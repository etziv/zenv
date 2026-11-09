---
name: device-test
description: "Run and check the app yourself: Android on the emulator (install, launch, tap, screenshot, logcat with pass criteria) or a web app in the browser (dev server, screenshot, console). Use to prove a module or screen, and for QA."
---

# Device and browser tests (no boss step)

Every check has a pass criterion written BEFORE you run it, and its result goes into the handoff (numbers, screenshot paths, log lines).

## Android (emulator)
1. AVD: list with `emulator -list-avds` (SDK `emulator/`). None: create one from an installed x86_64 Google APIs image (`avdmanager create avd -n <project>_smoke -k "system-images;android-<api>;google_apis;x86_64"`). API 30+ x86_64 images translate arm64-only apps (`ro.dalvik.vm.native.bridge=libndk_translation.so`), so no extra ABI is needed. A missing image is an install: ask the boss once.
2. Start headless in the background: `emulator -avd <name> -no-window -no-boot-anim -no-snapshot-save`; wait until `adb -s emulator-5554 shell getprop sys.boot_completed` prints 1.
3. ALWAYS target the emulator: `adb -s emulator-5554 install -r <apk>`, `adb -s emulator-5554 logcat -c`, `adb -s emulator-5554 shell am start -n <appId>/<Activity>`.
4. Interact: find a control's bounds with `adb -s emulator-5554 shell uiautomator dump /sdcard/ui.xml` then `adb -s emulator-5554 exec-out cat /sdcard/ui.xml`; tap with `adb -s emulator-5554 shell input tap <x> <y>`; text with `input text`.
5. Prove: screenshot `adb -s emulator-5554 exec-out screencap -p > <out>/<name>.png` (a git-ignored output folder), then Read the PNG and compare with DESIGN.md and the criterion; `adb -s emulator-5554 logcat -d -s <tags>` for the expected lines and no `FATAL` / crash.
6. Stop, also after a failed check: `adb -s emulator-5554 emu kill` (only an emulator you started), plus any server the
   app talked to that you started ("Servers you start").
16 KB memory pages CAN be checked on the emulator: an Android 15+ "Google APIs Experimental 16 KB Page Size" system image (x86_64), `adb -s emulator-5554 shell getconf PAGE_SIZE` must print 16384; run it when the app has native code (.so) or targets API 35+. The emulator cannot prove hardware decoding or a real GPU: list those as "real phone, optional" in the handoff; they never block.

## Servers you start (dev server, `artisan serve`, `php -S`, `npm run dev`, emulator)
Left running, a server holds memory and locks its folder (2026-10: seven forgotten servers ran for a week and blocked a
folder rename). So:
1. Before starting, check the port (`netstat -ano | findstr :<port>` on Windows, `lsof -i :<port>` elsewhere). Something
   already listens: reuse it; it is the boss's or an earlier run's. Never start a second server on top.
2. Start it in the background with an explicit working directory (never `cd` a shell into the repo for a long-running
   command) and write `server: <command> pid <n> port <p>` in your handoff.
3. Stop it before you reply, also when the check failed: end that PID with its children (`taskkill /PID <n> /T /F`,
   `kill <n>`), or stop the background task you started; then confirm the port is free. A server you did not start stays.

## Web (browser) — via the `playwright` MCP
The kit ships a project-scoped, isolated Playwright MCP (`.mcp.json`: `--isolated --headless`, output in `.brain/playwright-mcp/`), so every project runs its own browser and parallel projects never fight. Approve it once when the session asks.
Browser: Chromium headless shell (`.claude/scripts/playwright-mcp.json`), about half the RAM of Chrome (measured
2026-11-09: 107 vs 278 MB private for one page). The MCP version is pinned (`@playwright/mcp@0.0.84`) because each
version needs its own shell build. "Browser ... is not installed" means the shell is missing on this machine: ask the
boss once, then run `npx -y playwright@1.65.0-alpha-2026-10-30 install chromium-headless-shell` (the Playwright that
MCP pin uses; bump both together). He declines, or a page renders differently from Chrome: replace `--config ...` with
`--browser chrome` in `.mcp.json`. A first visit to an external site sometimes waits 7-12 s (cause not found, G-004); localhost is always fast.
Small checks only (one flow, a screenshot, the console) go through the MCP. A full e2e run (every critical journey: the
MVP review, before a release, after a big change) never uses a headless browser (boss 2026-11-09): run the project's
Playwright Test suite on real Chrome with a visible window, a `chrome-full` project in `playwright.config`
(`use: { ...devices['Desktop Chrome'], channel: 'chrome', headless: false }`), `npx playwright test --project=chrome-full`.
One browser per session: the main session and its subagents share the session's one server. Its tools are often
deferred: load them first with `ToolSearch` (`select:mcp__playwright__browser_navigate,mcp__playwright__browser_snapshot,...`)
before deciding the MCP is absent. Never launch a browser yourself (a CDP or playwright-core script, `chrome --headless`):
each run starts another Chrome. The project's own Playwright Test suite (full e2e above) is the one exception. When the flow is done and no browser step is left in the task, call `browser_close`;
`--idle-timeout` closes a forgotten one after 5 minutes.
1. Start the dev server per "Servers you start" (port check first, PID noted); wait until the URL answers.
2. Drive it with the `playwright` MCP tools: navigate, fill forms, click, assert, take a screenshot and Read it, read the console and network. Record the steps as a Playwright e2e test in the repo so the flow re-runs in the verify suite. If the MCP is absent after the ToolSearch load (android-only project disabled it, or Node/Chrome missing), run the project's Playwright/Cypress tests headless and check HTTP responses instead.
3. Criteria: the page matches DESIGN.md, no console errors, the flow's expected result is visible, empty/loading/error states render.
4. `browser_close` when no browser step is left; stop the server you started and confirm its port is free ("Servers
   you start", step 3). The browser profile is in-memory (isolated), so nothing else to clean.
