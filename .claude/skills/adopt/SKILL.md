---
name: adopt
description: "Adopt an existing codebase: build the graph, detect platform and stack, write .brain (ARCHITECTURE, MODULES, BRIEF, FLOWS, STATUS, verify commands) from verified facts only. Use when the project has code but .brain has no BRIEF or ARCHITECTURE."
---

# Adopt an existing project

You do not understand the system until this is done. Never guess.

1. Graph: `node .claude/skills/memory/scripts/memory.mjs graph` if `.brain/graphify-out/` is missing (multi-repo safe). Start with `.brain/graphify-out/GRAPH_REPORT.md` (the most connected nodes) and `node .claude/skills/memory/scripts/memory.mjs recall "<area>"` (plus `dead` for the health picture), then the README, package or build files and recent git history (`git log --oneline -30`). Do not read the whole codebase.
2. Detect the platform (web, android, other) and the stack with versions. Write only facts you verified; load the matching platform skill.
3. Write from verifiable facts only:
   - `ARCHITECTURE.md`: platform, stack, structure, data model, conformance checklist.
   - `project.json` `verify`: the real build, lint and test commands, each confirmed by running it.
   - `MODULES.md`: the modules you can identify, status "done" or "doing" only with evidence.
   - `BRIEF.md`: goal and scope as far as the README and the code show; the rest as open questions.
   - `FLOWS.md`: the 3-6 main user flows, each traced with `memory.mjs flow "<entry Class.method>"` and written as `F-` entries (steps name their code, `History: - <today> adopted`).
   - `STATUS.md`; `DECISIONS.md` only for decisions and gotchas stated in the repo (comments, ADRs, README) or by the boss.
   - `DESIGN.md` from the current UI (tokens, components) when a UI exists.
4. Check it before anyone relies on it: run `memory.mjs verify --all`, fix every `[!]`, then hand the checklist to an independent `qa` subagent (memory accuracy only: it reads each listed span and reports every claim the code contradicts) and fix what it finds. Memory an agent wrote is a claim until someone else checked it against the code.
5. Show the boss a short summary as ONE card (what you recorded, open questions) and treat it as the source of truth after his APPROVE.
