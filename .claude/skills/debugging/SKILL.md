---
name: debugging
description: "Root cause before any fix: a bug, failing test, crash, build failure, flaky test or unexpected behavior. Reproduce, trace to the source, one hypothesis at a time, a failing test first, stop after 3 failed fixes."
---

# Debugging

No fix before the root cause is found. A fix at the symptom is a second bug waiting. Systematic is faster than
guess-and-check, most of all under time pressure.

## 1. Find the cause (no edits to product code yet)
- Read the whole error: message, stack trace, file and line, error code. It often names the fix.
- Reproduce it on demand: the exact command or steps, every time. Not reproducible: gather more data (logs,
  `adb logcat`, a run under load), never guess.
- What changed: `git log --oneline -15`, `git diff`, a new dependency, config or environment. Check the memory for
  this area first: `memory.mjs mem "<area>"` (a `G-` dead end may already explain it).
- Several components (UI -> service -> engine -> native, or CI -> build -> deploy): log what enters and leaves each
  boundary once, run it, and let the evidence show WHERE it breaks before you look at HOW.
- Error deep in the call chain: trace the bad value backward (`memory.mjs uses "<fn>" --depth 2`, `find "<fn>\("`) to
  where it was first produced, and fix it there. Can't trace by reading: log the value with a stack trace.
- Find a similar thing that works in this codebase and list every difference, however small.

## 2. Prove it
- One hypothesis, written down: "X is the cause because Y". Test it with the smallest change, one variable at a
  time. Wrong: a new hypothesis, never a second change on top of the first.
- Don't know: say so in the plan file and gather more evidence; never pretend.

## 3. Fix it
- First a test that reproduces the bug (the smallest possible case; a one-off script if the stack has no test
  framework). Run it and record that it FAILS for the reason you found.
- One fix at the root cause, in the shared code, nothing bundled ("while I'm here" changes wait). Run the test:
  it passes. Then `task.mjs check` (the verify commands). Record both runs in the plan's Test results.
- Bad data caused it: make it impossible where it matters (check at the entry point and where the value is used;
  in tests, refuse dangerous operations outside temp folders).

## 4. When fixes fail
- After each failed fix: back to step 1 with what you learned, and add a line to the plan file.
- **3 failed fixes = stop.** Each fix revealing a new problem elsewhere means the design is wrong, not the fix.
  Record it as a dead end (`G-<id> <date> Tried <X>, abandoned because <Y>`), then rethink the approach. A new
  approach that changes the architecture, the brief or the scope is a HAMBATAN card; otherwise decide and go on.
- Truly external (network, device, timing): document what you checked, then handle it (retry with backoff, a
  timeout, a clear error message, a log line for next time). Most "no root cause" findings are unfinished work.

## Flaky tests and timing
- Never fix flakiness with a longer sleep. Wait for the condition itself (`waitFor { state.ready }`, Espresso
  idling resources, `adb shell` polling with a deadline), with a timeout that names what it waited for.
- A real delay is allowed only when the behavior under test is time-based (debounce, a 300 ms threshold); comment why.
- A test that leaves state behind (files, rows, settings): run the tests one by one to find the polluter.

## Stop signs: you are guessing
"Quick fix now, investigate later" · "just try X" · several changes before one run · "it's probably X" without
evidence · skipping the failing test · "one more fix" after two failed ones. Each means: back to step 1.
