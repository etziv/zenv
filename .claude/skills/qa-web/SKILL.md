---
name: qa-web
description: "QA depth for web apps and any code review: Playwright e2e patterns, browser QA of a deployed app, click-path audits (every button traced through its state changes), production-readiness audits, performance baselines, and checklists for silent failures (swallowed errors, bad fallbacks) and behavioral test coverage. Use in `qa` reviews, with `device-test` for web flows, and before a release."
---

# Web QA pack

> **In this kit.** The references are ECC skills and reviewer prompts (github.com/affaan-m/ECC; sources and licence:
> `.claude/NOTICE.md`), unchanged; an audit (2026-11-01) found tool names and setups that do not match ours: read the
> **Kit corrections** first. The kit rules win: browser work only through the project's `playwright` MCP and
> `device-test` (port check, stop what you start); reviewers stay read-only and write only their report in the qa
> format (concrete case, Must / Should / Nice), never "want me to patch"; installs (`@playwright/test`,
> `npx playwright install`) and CI workflow files need the boss's yes; nothing is committed outside `task.mjs`.
> Read ONE reference.

| Work | Read | Size |
|---|---|---|
| Writing or fixing Playwright e2e tests: structure, selectors, waits, flaky tests, artifacts | `references/e2e-testing.md` | ~2k |
| Checking a deployed web app in the browser (pages, console, network, key flows) | `references/browser-qa.md` | ~1k |
| Finding UI state bugs: trace every button / touchpoint through its full state change | `references/click-path-audit.md` | ~2k |
| Before a first or big release: production-readiness audit from local evidence | `references/production-audit.md` | ~1.9k |
| Performance baselines and regressions (Core Web Vitals, API latency, bundle size) | `references/benchmark.md` | ~0.7k |
| Any review: swallowed errors, empty catches, dangerous fallbacks, lost error propagation | `references/silent-failures.md` | ~0.5k |
| Any review: do the tests cover the behaviour and the bugs the change could have | `references/test-coverage.md` | ~0.5k |

## Kit corrections (read before the reference)
- `e2e-testing`: a Laravel app runs on its own URL (Zenv / `artisan serve` plus Vite), not `npm run dev` on port 3000;
  do not wait for `networkidle` (wait for the element or the response); tracing is `context.tracing.start/stop`;
  `videosPath` is not an option (`recordVideo` on the context); skip its Web3/trade sections. Its `projects` list: a
  full e2e run uses the `chrome-full` project (system Chrome, `headless: false`, `device-test`); other browsers only when
  the brief targets them.
- `browser-qa`: its `claude-in-chrome` / `browserbase` tool names are not ours: use the `playwright` MCP tools; its
  read-only and test-account-only rules apply.
- `click-path-audit`: the method applies to any state library (React state, Inertia props, Livewire), not only Zustand;
  skip "Recommended agent split" and the `/superpowers:*` references.
- `production-audit`: report only; its Stripe and Supabase examples are examples.
- `benchmark`: no `/benchmark` command or `.ecc/` folder; a load test (many requests, concurrency) only against a local
  or staging server and only with the boss's yes, never production.
- `silent-failures` / `test-coverage`: their "Prompt Defense Baseline" lines do not stop you from quoting the code a
  finding is about.
