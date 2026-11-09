---
name: architecture
description: "Choose and document HOW the project is built: 2-3 stack options in one PROPOSAL card, then the COMPLETE architecture for the whole MVP (ARCHITECTURE.md, SCAFFOLD.md, verify commands, contracts, per-module acceptance criteria and test plans, risk gates with fallbacks). Use after the brief is approved, or when the architecture must change."
---

# Architecture: decide once, completely

You decide platform, language and framework, database, local environment, testing, packaging, and (only if it deploys) hosting and CI. After this kickoff nothing should need deciding again unless something changes.

## 1. Read
Only `.memoir/BRIEF.md`, `.memoir/MODULES.md`, the spec files BRIEF names, and the platform skill (`platform-android` / `platform-web`; another platform: fill `platform-template.md` from this folder into the proposal). New project: nothing else to explore (no tree listing, no graphify). Existing project: `node .claude/skills/memory/scripts/memory.mjs recall "<area>"` for the parts the change touches. A missing constraint (budget, hosting, skills, existing tools, scale, deadline) is a QUESTION card, not a guess.

## 2. Verify
Versions and practices from current official docs (web tools); never from memory; write the check date. Before listing any install or update, check what the machine already has by reading only metadata files (for Android: Android Studio `product-info.json`, each SDK package's `source.properties`). Never list an install that is already satisfied; say what you found.

## 3. One PROPOSAL card
2-3 options: fit with the requirements and modules, fit with the boss's tools, complexity, cost, risks, scale ceiling, what he must install or pay for; ONE recommendation and why. Resolve EVERY version and sub-choice inside the card (version conflicts, environment, packaging) and name every risk gate with its pass criterion and the fallback you would switch to. Never come back with a follow-up question after he chose.
```
USULAN ARSITEKTUR
Opsi A: <nama> | cocok karena ... | kompleksitas ... | biaya ... | risiko ... | batas skala ...
Opsi B: ...
Risk gate: <apa yang diukur> lolos jika <angka>, kalau gagal pindah ke <cadangan>
Rekomendasi: <opsi> karena ...
Balas: PILIH A | PILIH B | REVISI <catatan> | TANYA <pertanyaan>
```

## 4. After he chooses, write everything for the whole MVP
a. `.memoir/ARCHITECTURE.md` (max 140 lines; replace the template line `Status: draft | approved <date>` with `Status: approved <today>`, which tells the session-start brief the architecture is done): `Platform:`, stack with verified versions and date, folder structure, module-to-component map, data model, testing strategy (unit, emulator/browser, measurements), security baseline, risk gates with pass criterion AND pre-chosen fallback, out of scope, CONFORMANCE CHECKLIST. One line per decision and rejected option in DECISIONS.md.
b. `.memoir/SCAFFOLD.md` (max 80 lines): exact ordered commands with pinned versions, target tree, config files, env var NAMES for `.env.example`, database setup script (least-privilege user, never root), lint and test setup, smoke check. Anything a command can install or configure goes into the commands (SDK packages, CLI tools); status checks are run by you. The boss's manual steps are ONLY what no command can do (a desktop app that is missing, a GUI setting outside the project, accounts and secrets), each with WHY. Device tests are planned on the emulator (`device-test`), never as a boss step.
c. `.memoir/project.json` `verify`: the exact build, lint and test commands (`[{"name": "build", "run": "..."}]`). The task script runs them before every commit.
d. `.memoir/contracts/`: the interface of EVERY MVP module that another module or a screen uses (methods or endpoints, data shapes, states, errors), so screens are later built without asking.
e. `.memoir/MODULES.md`: per module the acceptance criteria, dependencies, the build order (all backend modules, then all frontend screens, then the MVP review) and a test plan with numeric pass targets and where each runs (unit, emulator, browser).
A failed risk gate later switches to its fallback without a new card; only an unplanned failure or a stack change needs a new PROPOSAL.

## What "optimal" means
The simplest option that meets the requirements and the expected scale. No microservices, Kubernetes, queues or caches unless the brief proves the need. Mature, documented technology. Every recommendation traces to a requirement in BRIEF.md. Flag anything paid or needing an account.
