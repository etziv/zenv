# Agents working in this repo (any tool)

This project keeps its long-term memory as plain markdown in `.brain/`, so the baton passes between
Claude Code, Codex, Cursor, Gemini CLI and any other agent, across sessions and machines, with no server.
If you are not Claude Code reading `.claude/CLAUDE.md`, read this first.

## Before you start, read in order
1. `.claude/CLAUDE.md` — the team rules: who does what, what the boss approves, how to work.
   If you act as the main dev: `.claude/agents/dev.md` (task flow, reviews, cards).
2. `.brain/STATUS.md` — current state, next steps, open issues.
3. `.brain/DECISIONS.md` — decisions (`D-*`) and gotchas / dead-ends (`G-*`). Do NOT redo an approach marked abandoned.
4. The open task's plan `.brain/handoff/<id>-dev.md`, and `.brain/contracts/` for the interfaces between parts.

Ask memory and the code graph in one call before reading many files:
`node .claude/skills/memory/scripts/memory.mjs recall "<topic>"` (no LLM, no server; `mem` = memory only,
`find` = code only). Who uses a symbol: `memory.mjs uses "X"` (complete, graph-certain callers
first; `--depth 2` before a change), not grep. `.brain/infra.md` answers "where is X deployed".
The full memory rules are in `.claude/skills/memory/SKILL.md`.

## As you work, write back to `.brain/`
So the next agent — in any tool — continues without re-explaining:
- A changed state or next step -> a `STATUS.md` line.
- A decision and why -> `## D-<id> <title> [active] <date>` in `DECISIONS.md`.
- An approach you TRIED and abandoned -> a gotcha `G-<id> <date> Tried <X>, abandoned because <Y>` — this is what
  stops the next agent repeating a dead end.
- An interface change -> the matching file in `.brain/contracts/`.
Record only what the code and the graph cannot tell; one fact, one place; relative paths only.

## Hard rules
- Git writes go ONLY through `node .claude/scripts/task.mjs` (start / ship / finish / abort / deploy). Never run git or gh
  write commands by hand.
- The boss approves only: the brief, the architecture proposal, the design proposal, a new/destructive DB schema, the QA + security
  reports, and a deploy. Ask in chat before installs, DB commands, or anything destructive outside the project.
- Keep secret values out of git, memory, handoffs and chat.
