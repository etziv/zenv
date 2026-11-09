---
name: backend
description: "Build server code, APIs, business logic, data access and (on mobile) the data and domain layers; execute SCAFFOLD.md for the setup; keep the contracts current; prove every step with your own tests and measurements. Use for any backend or setup task."
---

# Backend and data layer

Build APIs, business logic and data access in the existing style and ARCHITECTURE.md. Validate all input. On mobile you own the data and domain layers (see the platform skill).
Framework depth, one reference at a time: Laravel -> `stack-laravel`; Android data/domain layer -> `stack-android`.

## Plan first, then prove every step yourself
Plan at the top of `.memoir/handoff/<id>-dev.md` (goal, steps, files, the tests and measurements you will run) and begin at once. There is no QA per module before the MVP review: your own testing is the gate. At every step: unit tests for every function you add (including failure cases), the verify commands (`node .claude/scripts/task.mjs check`), every measurement the module's test plan names (performance, a risk gate) with numbers, and a run on the emulator or in the browser (`device-test`) whenever the code can run there. A measurement that misses its target: switch to the fallback ARCHITECTURE.md planned, or tell the boss with the numbers if none was planned. Never hide a failing number.

## Setup task
Execute `.memoir/SCAFFOLD.md` step by step, exactly. Status checks (versions, free disk, docker) are yours. Installs: ask the boss in chat once, then run them. Keep `.env` out of git and list its variable names in `.env.example`. Prove the setup yourself (build, first test, data connection, smoke run), then move SCAFFOLD.md to `.memoir/archive/`.

## Contracts
Keep `.memoir/contracts/` current: one entry per endpoint or interface (max 8 lines): `## <METHOD> <path> or <Interface.method> <date>`, then Auth, Request, Response, Errors. Edit in place, remove with the code, no history. Max 150 lines per file; split by domain and list the files in `contracts/index.md`. Generated OpenAPI replaces it when available.

## Close every module (your module review, in the handoff)
Each acceptance criterion from MODULES.md with its evidence, every flow the module adds or changes written or updated in `.memoir/FLOWS.md` (steps name their code; one History line; `memory.mjs flows` shows none OUT OF DATE; `memory.mjs verify` shows no `[!]` and you read its spans), the CONFORMANCE CHECKLIST items the module touches, the measurements against their targets, and the full test suite re-run green (no regression in earlier modules). Because all backend modules come before any screen, risks are settled before the UI is built.
