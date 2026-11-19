---
name: clean-code
description: "Clear, tidy code with no comments that explain code or record changes (section markers allowed), and an audit of existing code: banned comments (explanations, change notes, TODOs, commented-out dead code), deep nesting, long lines, oversized files, unclear names. Use when the boss asks to audit or tidy code, when the comment gate in task.mjs check/ship fails, and in qa reviews of changed code."
---

# Clean code

The code explains itself. A reader learns what it does from names, small functions and tests, and why it changed
from git and `.brain/`. A comment that restates the code goes stale the first time someone edits the code and
forgets the comment.

## Comment policy (enforced: `task.mjs check` and `ship` refuse banned comments on lines the task added)
Allowed:
- **Section markers**: a label of up to 8 words with a decoration, or a region keyword: `// ---------- auth`,
  `/* === Relations === */`, `# ---------- io`, `// MARK: - Helpers`, `//#region users`, `<!-- ===== footer ===== -->`.
  Also a box: a divider line, one short label line, and another divider line. In markup (HTML, Blade, Vue) and
  stylesheets, a plain label is a section marker too: up to 4 words starting with a capital (`{{-- Waktu --}}`,
  `<!-- Data Table -->`, `/* Grade badges */`) or one lowercase word (`<!-- header -->`).
- **Tool directives** that change how a tool behaves: `eslint-disable-next-line`, `@ts-expect-error`, `prettier-ignore`,
  `@phpstan-ignore-next-line`, `# noqa`, `# type: ignore`, `//go:build`, `/* webpackChunkName: "x" */`,
  `/*#__PURE__*/`, a shebang, encoding lines.
- **Type tags in a docblock**, with no prose, not even after the tag: `@param array<int, string> $ids`,
  `@return list<User>`, `@var`, `@property`, `@template TModel of Model`, JSDoc `@param {string} name`, a multi-line
  array shape. `@param int $id the user id` and KDoc `@param id the user id` are prose (banned).
- **A license header** at the top of a file (after `<?php` too): copyright, SPDX, or a named license.
- **`shortcut: <its limit>, <when to upgrade>`**: a deliberate deferral (`memory.mjs debt` lists them), on at most
  two lines.

Banned:
- **explain**: any prose about what or how the code does something, including KDoc/PHPDoc/JSDoc description lines
  and Python docstrings.
- **change-note**: history such as "fixed", "changed", "added for task 12", dates, versions, "previously", `@author`,
  `@since`. Git already keeps that history.
- **todo**: TODO / FIXME / HACK / XXX. Do the work now, or record it as a `shortcut:`.
- **dead-code**: commented-out code. Delete it; git keeps it.
- **empty**: a bare `//` or `/** */` (stubs from `artisan make:*` leave them). Delete it.

## Instead of a comment
| You wanted to write | Do this |
|---|---|
| what a block does | extract a function whose name says it |
| what a value means | a named constant or a well-named variable (`const retryLimit = 3`) |
| why a condition exists | a named boolean (`const isLegacyInvoice = ...`) or a guard function |
| the shape of data | a type, interface, data class or a docblock type tag |
| an edge case | a test named after the case |
| why a design was chosen | a ruling in the plan file or a `D-` decision in `.brain/`, and the commit/PR text |
| a known limit | `shortcut: <limit>, <when to upgrade>` |

## Writing rules
- Names say intent and unit: `invoiceTotalCents`, `fetchActiveUsers`, `isExpired`. No `data`, `tmp`, `handle2`, or
  abbreviations the domain does not use.
- One function, one job, one level of abstraction. When it needs scrolling (about 40 lines) or a section comment
  inside, split it.
- Return early and keep the happy path unindented. Nesting deeper than about 3 blocks inside a function means:
  extract a function, add a guard clause, or use a lookup table.
- No flag arguments that switch behavior (`render(true)`): use two functions or an options object with names.
- No magic numbers or strings: a named constant next to where it is used.
- No dead code, unused imports, unused parameters, or leftover debug output (`console.log`, `dd`, `print`).
- Follow the project's formatter and the conventions around the change. Do not reformat lines you did not change.
- Generated files the task adds (`artisan make:*`, `vendor:publish`, scaffolds) and files copied from a skill's
  assets or templates still go through the gate. Read their comments at the source, then strip them from the copy.

## Audit (when the boss asks: "audit", "rapikan", "kode berantakan", or before a cleanup task)
1. Run `node .claude/skills/clean-code/scripts/comments.mjs audit [paths...] [--all]`. With no path it audits the
   project and the code repos listed in `.brain/project.json`, using git's file list, so ignored, vendored and build
   folders are skipped. It prints counts per kind, the banned comments (`file:line kind text`, 60 by default) and
   the untidy files: over 600 lines, block nesting of 6 or deeper, lines over 140 characters.
2. `node .claude/skills/memory/scripts/memory.mjs dead` (unused functions and files) and `cycles`.
3. Read the worst files (most banned comments, deepest nesting, largest) against the writing rules: unclear names,
   long functions, duplication (name both places), mixed abstraction levels, flag arguments, magic values. Swallowed
   errors: the `qa-web` reference `silent-failures.md`.
4. Report in `.brain/handoff/<id>-dev.md`: the numbers from step 1, then findings ranked by impact, each with
   `file:line`, what is wrong, and the concrete fix. The audit is a report. A cleanup of existing code is its own
   task, with a scope the boss agrees to.

## Cleanup of existing code (its own task)
- Behavior stays the same: no logic change mixed into a cleanup. The tests are green before and after, and a
  touched area with no test gets one first.
- Work one module at a time. Dead code and change notes: delete them. Explanations: first make the code say it
  (rename, extract, constant, test), then delete the comment. A "why" worth keeping goes to `.brain/` as a decision.
- Many files of mechanical comment removal: a `worker` on sonnet per the Models rule in dev.md, with the audit list
  as its brief. Read its diff before building on it.

## The gate
`task.mjs check` and `ship` run `comments.mjs gate` over the task's code files. They look only at lines added
since the task started (and every line of a new or renamed file). Old comments the task did not touch pass. The
gate prints each `file:line kind`, and nothing is committed until those comments are fixed. A project whose code
must follow someone else's comment style (a contribution to another repository) sets `"comments": "allowed"` in
`.brain/project.json`, and only on the boss's word. The labels come from a heuristic (an explanation may show up
as `dead-code`), but every reported comment is banned whatever its label says. The scanner knows strings,
template literals, regex literals, heredocs, PHP attributes and shell expansions; it does not read comments
inside Blade `@php ... @endphp` blocks or inside Kotlin string templates.
