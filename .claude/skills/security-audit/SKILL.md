---
name: security-audit
description: "Security review method and attack-class checklists for the `security` agent: the rules every finding must pass (trust boundary + concrete result, disproof, verdicts, severity anchors), an MVP coverage method (trust-boundary map, coverage list, self-critic pass), and per-surface checklists traced in Laravel / Inertia React / Android Kotlin: core classes, web auth and sessions, browser side, Android components and WebViews, supply chain and CI, deploy and config, data isolation and lifecycle, AI/LLM features, resource exhaustion. Use in every security review; read the one reference the change's surface needs."
---

# Security audit: method and checklists

> **In this kit.** Ideas adapted from cloudflare/security-audit-skill (MIT, `.claude/NOTICE.md`), rewritten for our
> stacks: one reviewer (no hunter/verifier fan-out), no JSON output; the report format is the one in
> `agents/security.md`. Framework detail stays in the stack packs (`stack-laravel` `laravel-security.md`, `stack-react`
> `review-react.md`, `platform-android`); this skill gives the method and the attack classes. Read ONE reference at a
> time, only the sections the change touches.

## Which reference
| Review or surface | Read |
|---|---|
| MVP review (the whole range) | `references/mvp-coverage.md` first, then each reference its map selects, one at a time |
| Server code: input handling, queries, files, permissions, money or state flows | `references/core-classes.md` |
| Login, sessions, cookies, CSRF, Sanctum/JWT tokens, OAuth/Socialite, reset, 2FA, API keys, proxies, HTTP caching | `references/web-auth.md` |
| React/Inertia pages: user content rendering, shared props, redirects, browser storage, postMessage, CORS, realtime channels | `references/client-side.md` |
| Android: manifest, exported components, intents, PendingIntents, deep links, WebView, providers, local data, network config | `references/android.md` |
| New or changed dependency, lockfile, build script, GitHub Actions, Dockerfile, release signing | `references/supply-chain-ci.md` |
| `.env`/config, Docker/compose, web server, deploy scripts, storage buckets, webhooks, queues, broadcast, schedules | `references/deploy-config.md` |
| Data of many users or tenants, caches, search, exports/imports, soft delete, account deletion, backups, data migrations | `references/data-lifecycle.md` |
| A feature that calls an LLM or lets a model act (chat, RAG, tool calls, MCP) | `references/ai-llm.md` |
| Uploads, parsers, regexes, pagination and filters, queues, paid API calls (SMS, mail, AI), rate limits | `references/availability.md` |
| Native C/C++ (NDK, JNI) parsing untrusted input | no reference: check lengths and bounds, integer sizes, buffer lifetime, and what the caller controls; list what you could not settle under "Not checked" |

A change on several surfaces: read the matching references in turn, never all at once.

## Rules every finding passes
1. **Boundary and result.** Name four things: the less-trusted actor (guest, another user, another tenant, another app
   on the phone, a web page inside a WebView, a model that read attacker text), what it sends or does, the control that
   should stop it, and what it ends up with (data read or changed, an action done, an account taken, money or quota
   spent). Missing one of the four: not a finding.
2. **Disprove first** (the rule in `agents/security.md`). Framework defaults are real controls: Blade `{{ }}` and React
   text escaping, Eloquent parameter bindings, the CSRF middleware on the `web` group, the Android defaults of the
   project's `targetSdk`. Check the version before you lean on a default.
3. **Facts outside the repository** (server and proxy config, CDN, real env values, Play Console and signing, bucket
   policy, branch protection) prove neither safety nor danger. When one decides the case, the verdict is **needs
   validation**: name the fact and a safe way to settle it (a read-only command, a local test, or what the owner looks
   at). Never send requests to a live or production system.
4. **Hardening is not a finding.** A missing header, cookie flag, rate limit or best practice with no concrete case goes
   in one line under "Hardening (not counted)". Do not overstate: a crash is not code execution, an action on one's own
   data is not a privilege gain, abuse by volume needs a shared victim or the operator's money.
5. **Severity.** high = a control fully defeated with real damage (acting as another user, reading or changing another
   user's or tenant's data, stored script that runs for other users, code execution, a leaked secret that grants access,
   an unauthenticated way to stop the service; what others call critical is high here). medium = a real crossing with a
   narrow reach or unusual preconditions. low = non-secret internals exposed, or much effort for little gain. If you
   cannot say what the damage is, rate it lower than your first instinct. A needs-validation finding carries the severity
   it would have if the missing fact turns out the bad way; the dev treats it as open until it is settled.
6. **Fix.** State the invariant the code must hold and the smallest change where the code makes the decision (policy,
   FormRequest, middleware, query scope, manifest attribute), plus the regression test that proves it. Never edit code.
7. **Variants.** The root cause of a high finding usually repeats: look for siblings in scope (`memory.mjs uses`, a
   Grep for the same pattern) and list them under the same finding number, each with its own trace.

## Earlier reviews
Before a review that is not the MVP review, Grep the earlier reports for the files in scope: `.brain/archive/*/*-security.md` (shipped tasks)
and `.brain/handoff/*-security.md` (the open one). A
claim rejected on unchanged code stays rejected: do not raise it again. An open or needs-validation finding on the same
code is checked first. A changed file reopens its earlier claims, rejected ones included.

## Mistakes that waste the boss's time
- A checklist item reported as a vulnerability without a case.
- Guessing how the server, proxy, CDN, bucket or phone is set up instead of marking needs validation.
- Counting what users can already do to their own data as an attack.
- Claiming a stronger effect than the trace shows, or a severity on a hunch.
- A finding line, its text and its severity that disagree with each other.
