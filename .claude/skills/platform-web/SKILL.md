---
name: platform-web
description: "Web (frontend + server) defaults: stack candidates, layer mapping, verify commands, scaffold notes, browser testing, web security and QA checklists. Load when ARCHITECTURE.md says Platform: web or when proposing a web stack."
---

# Profile: Web (frontend + server)

Used when ARCHITECTURE.md says `Platform: web`. In this kit dev plays every role below (architect, backend, frontend, devops; the `designer` agent proposes the look and dev writes DESIGN.md from the chosen direction); qa and security are the review agents. Everything here is a default or a candidate; dev verifies versions from current docs and decides.

## Role mapping
- architect: framework, rendering model (server-rendered, SPA, hybrid), database, auth approach, hosting only if the project deploys.
- backend: server code, API, data access, auth, background jobs, API contract.
- frontend: pages, components, client state, routing, forms, accessibility.
- designer: visual system, responsive layouts, dark/light, motion.

## Verification commands (examples; the architect writes the real ones)
Package scripts (`build`, `lint`, `test`), a type check, and for PHP stacks the framework's test runner. Always include one smoke check (the app starts and a health route or main page responds).

## Scaffold notes
Use the framework's official generator with a pinned, verified version. Create `.env.example` with names only. Database and user creation through a script the boss approves; never root. Local web stacks (for example a bundled Apache/MySQL manager) must be running before the connection check; start it yourself when it has a command; only a GUI-only start is a manual step for the boss.

## Self-check additions (builders)
Build and lint clean, own tests pass, no console errors on the main flow, forms validate input on both sides, no secrets in code or logs.

## QA checklist
Acceptance criteria one by one; API contract vs implementation; negative cases (invalid input, empty data, expired auth, duplicate submit); responsive layout at phone and desktop widths; keyboard navigation and basic accessibility; error and loading states; browser matrix per the brief; concurrency and idempotency where relevant.

## Security checklist
Authentication and session handling; authorization on every endpoint (no ID guessing); input validation and output encoding (SQL injection, XSS); CSRF where cookies are used; CORS not wildcarded with credentials; security headers; file upload type, size and storage; rate limiting on login and costly endpoints; secrets only in environment variables; dependency audit; error messages without stack traces or PII.

## Release notes (devops, later)
CI on pull requests first; staging before production; migrations reviewed; health check and rollback plan in the deploy plan; secrets only in the CI secret store.

## Defaults to avoid over-engineering
Monolith first. No queues, caches, microservices or extra infrastructure until the brief proves the need.
