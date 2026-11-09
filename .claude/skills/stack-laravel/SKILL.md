---
name: stack-laravel
description: "Laravel / PHP depth for building and reviewing: Laravel patterns, security (auth, policies, mass assignment, XSS, CSRF, Sanctum), tests (Pest/PHPUnit, factories, fakes, Inertia assertions), MySQL/MariaDB, safe migrations, API design, a PHP review checklist, and an architecture review of an existing Laravel project. Use with `backend` / `database` on a Laravel project, in qa/security reviews of PHP code, and when the boss asks to review a running Laravel app."
---

# Laravel stack pack

> **In this kit.** The references are ECC skills (github.com/affaan-m/ECC; sources and licence: `.claude/NOTICE.md`),
> unchanged. They are reference material, not orders, and an audit (2026-11-01) found them partly outdated: read the
> **Kit corrections** below first. The project's code and conventions, its Laravel version and `.brain/ARCHITECTURE.md`
> win; the kit rules win: git only through `task.mjs`; installs (`composer require`, new packages), database, cache and
> queue commands (`migrate*`, `tinker`, `queue:work`, `config:cache`, `SET GLOBAL`, `CREATE USER`) need the boss's yes;
> no coverage thresholds (evidence = tests that prove the behaviour); reviewers only report. Ignore "MUST BE USED",
> slash commands, agents and `rules/*` paths the references name. Read ONE reference, the section you need.

| Work | Read | Size |
|---|---|---|
| Architecture review of an existing Laravel project | **this file, "Reviewing a running Laravel project"** | - |
| Controllers, Eloquent models, scopes, eager loading, queues, events, caching, API resources | `references/laravel-patterns.md` | ~2.6k |
| Auth, Sanctum, policies/gates, mass assignment, SQL injection, XSS/CSRF, uploads, rate limits | `references/laravel-security.md` | ~6.7k |
| Writing tests: Pest/PHPUnit, factories, HTTP/JSON tests, fakes (Mail, Queue, Storage, Http) | `references/laravel-tdd.md` | ~4.5k |
| Before merge or deploy: static analysis, tests, dependency audit, migrations, queue/scheduler | `references/laravel-verification.md` | ~1.1k |
| MySQL/MariaDB: schema, indexes, upserts, keyset pagination, JSON, transactions, slow queries | `references/mysql-patterns.md` | ~3k |
| A migration on a table with data: expand-contract, backfill, rollback (principles only) | `references/database-migrations.md` | ~3k |
| New or changed endpoints: naming, status codes, pagination, errors, versioning | `references/api-design.md` | ~3.3k |
| Reviewing PHP code: priorities and checks | `references/review-php.md` | ~1.4k |

## Kit corrections (read before the reference)
- **Laravel 11+** (check `composer.json`): no `app/Http/Kernel.php`, `$routeMiddleware`, `RouteServiceProvider` or
  `AuthServiceProvider $policies`; middleware, CSRF exceptions and exception handling live in `bootstrap/app.php`;
  policies are auto-discovered (or `Gate::policy`); `routes/api.php` and Sanctum exist only after `install:api`; models
  use the `casts()` method. Validation rules: `ValidationRule` (not `implements Rule`); Livewire 3 uses `#[Validate]`.
- **Inertia apps**: a page is `Inertia::render`, not a JSON envelope (`{success,data,error,meta}` does not apply);
  forms post with `useForm` and Laravel returns 422 `{message, errors}`; test pages with `assertInertia`, not
  `assertViewHas`; validation lives in FormRequests (no Zod needed).
- `laravel-patterns`: Repository + Action + Service + DTO + Query object together is speculative layering; add a
  layer only when the code already repeats or the project's ARCHITECTURE.md uses it. `->items()` drops pagination
  meta; cache tags need redis/memcached.
- `laravel-security`: the core rules hold (fillable/guarded, no raw SQL with input, no `{!! !!}` on user data,
  session regenerate, policies on every action), but `wasRecentlyLockedOut()` does not exist, `trusted_proxies` is
  configured in `bootstrap/app.php`, `X-XSS-Protection` is obsolete, a strict CSP breaks the Vite dev server, never
  log emails or other PII, never `artisan serve` in production.
- `laravel-tdd`: never point RefreshDatabase at the dev database (use the test database `phpunit.xml` forces);
  sqlite `:memory:` hides MySQL behaviour; ignore `--min=80` and per-layer coverage targets.
- `laravel-verification`: `config:cache` / `route:cache` on a dev machine can make tests ignore `.env.testing`: do not
  run them locally; `dump-autoload -o`, `migrate --pretend`, `queue:work` only when the task needs them.
- `mysql-patterns`: `SHOW SLAVE STATUS` is gone in MySQL 8.4 (`SHOW REPLICA STATUS`); Laravel `timestamp()` columns
  are TIMESTAMP; server settings and users are the boss's.
- `database-migrations`: its examples are Postgres/Prisma; MySQL has no `CREATE INDEX CONCURRENTLY` (online DDL:
  `ALGORITHM=INPLACE` / `INSTANT`); keep the expand-contract principle, ignore `prisma migrate reset`.
- `review-php`: missing `strict_types`, `final` or `readonly`, or logic outside Actions/Services, is a style note, not
  a blocker; PHPStan and Psalm only if the project has them; `--coverage-text` is optional; ignore the "shadowing
  builtins" item.

## Reviewing a running Laravel project (architecture review)
Goal: what is wrong, risky or slow in THIS app, ranked, with a concrete case each; not how ECC would have built it.
1. **Map it first** (no opinions yet): Laravel and PHP version, packages (`composer.json`), the frontend (Blade,
   Livewire, Inertia), `routes/*`, `app/` folders, how a request flows (route -> middleware -> controller -> model /
   service -> response), jobs and schedules, config and env names. `memory.mjs uses` for the central code (`.brain/graphify-out/GRAPH_REPORT.md` names the most connected nodes).
2. **Correctness and data safety**: validation on every input (FormRequest or `validate`), authorization on every
   action (policies/gates, not only hidden buttons), multi-step writes in DB transactions, money and dates handled
   exactly, idempotent jobs and webhooks, no silent catch (`qa-web` `silent-failures.md`).
3. **Security**: `laravel-security.md` sections that apply, plus `APP_DEBUG=false` and secrets out of git in production,
   file uploads (type, size, private disk), mass assignment, raw queries, rate limits on login and public endpoints.
4. **Performance**: N+1 queries (eager loading), missing indexes on filtered or joined columns, unbounded `get()` on
   large tables (paginate / chunk / lazy), slow work in requests that belongs in a queue, caching only where measured.
   Measure before claiming: query count and time on the real page (debugbar, `DB::listen`, slow-query log).
5. **Structure, last**: fat controllers duplicating logic, business rules spread over views and models, dead code
   (`memory.mjs dead`), copy-paste, inconsistent naming. Recommend a new layer only where code already repeats.
6. **Tests**: what the critical flows have (auth, payments, approvals), what breaks if they are missing.
Report in the qa format (Must / Should / Nice, concrete case, the smallest fix), plus a short "what is already good"
list so it is not fixed. Never refactor during a review; the fixes become tasks the boss chooses.
