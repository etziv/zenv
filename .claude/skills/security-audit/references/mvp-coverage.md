# MVP coverage method (one reviewer, the whole range)

Goal: every entry surface, crossed with its trust boundary and each attack class that applies, ends as checked, a
finding, not applicable (with the fact why) or explicitly not checked. One pass never covers everything; the report
says what it did not cover. Write the map and the coverage table into the report before the findings.

## 1. Map (in the report as `## Map`, about 20-40 lines, facts with `file:line` only)
- **Principals**: who acts and what each may do by design: guest, user, staff and admin roles, teams or tenants, other
  apps on the device, web content inside a WebView, an LLM if the product has one.
- **Assets**: accounts and credentials, other users' data, money, stock, approvals and other state that matters,
  admin functions, secrets, deploy and release authority, the operator's paid quota (SMS, mail, AI, maps).
- **Entry surfaces**, read from source:
  - Laravel: `routes/web.php`, `api.php`, `channels.php`, `console.php`; the effective list with middleware from
    `php artisan route:list --json --except-vendor` (read-only); FormRequests; webhook routes and the CSRF exceptions
    in `bootstrap/app.php`; jobs, listeners and schedules; uploads; signed links in mails and notifications; artisan
    commands run by cron.
  - React/Inertia: what `HandleInertiaRequests::share()` sends to every page, the props of each page, every
    `dangerouslySetInnerHTML`, external links and client-side storage.
  - Android: the merged manifest (`app/build/intermediates/merged_manifests/**/AndroidManifest.xml` when a build
    exists, else `src/main/AndroidManifest.xml` plus library manifests): activities, services, receivers, providers,
    intent filters and deep links, permissions; WebViews; the API hosts it calls; local storage.
  - Build and deploy: `composer.json`/lock, `package.json`/lock, `build.gradle*` and `settings.gradle*`
    repositories, `.github/workflows`, Dockerfile and compose, deploy scripts, `.env.example`, `config/*.php`.
- **Boundaries**: for each crossing from a principal to an asset, the control that holds it (middleware group,
  policy or gate, global scope, signed URL, `exported` flag or permission, CSRF, token ability), with `file:line`. A
  control that lives outside the repository (proxy, server, Play, bucket policy) is marked "outside source".

## 2. Coverage list (in the report as `## Coverage`)
One row per meaningful combination of surface group, boundary and class. Group alike routes ("all `/admin/*` x admin
gate x access control"); a row is something one focused pass can finish. The classes come from the table in
`SKILL.md`: `core-classes.md` applies to every server surface, the other references where the map shows their surface.

```
| # | Surface | Boundary (control) | Class (reference) | Status | Evidence |
```
Status: `checked` (with the files you read) | `finding N` | `needs validation N` | `n/a` (the fact that rules it out) |
`not checked` (why). A row is `checked` only with the paths you actually read: "auth reviewed" is not coverage.

Order: surfaces open to guests and to other apps first; then the ones guarding the most valuable assets (accounts,
other users' data, money, admin, secrets, release); then the rest. Rows you never reach stay `not checked`; never drop
a row silently.

## 3. Work each row
- Read the reference section for the row's class. Follow the input through validation, authentication, authorization,
  transformation, storage, later readers and the sink.
- Compare the row's control with every sibling path to the same effect: web vs API route, single vs bulk, controller
  vs job or console command, user screen vs admin screen, new vs legacy endpoint, normal vs retry or error path.
- Try the edge values the input accepts: missing, empty, zero, negative, very large, duplicated, another user's id, a
  stale or revoked token, two submits at once, a failure halfway.
- Stop when the row's invariant is settled either way; write its status and paths. A new surface found on the way
  becomes a new row.

## 4. Critic pass (yourself, after all rows)
Re-read the map and the table cold. Which surface has no row (console commands, jobs, broadcast channels, signed-link
routes, exports, admin, debug or legacy routes, API calls the UI never makes, a provider or receiver)? Which class did
a row skip? Add those rows and work them. Then two sweeps:
- **Literal sweep**, every item, each hit only a lead until traced:
  - secrets in tracked files (`git grep -nIE "password|secret|api[_-]?key|token|BEGIN [A-Z ]*PRIVATE"`), tracked
    `.env`, keystores, `*.pem` (`git ls-files`), secrets removed from the tree but still in history
    (`git log --all --diff-filter=D --name-only`), a `.gitignore` that misses `.env`, keys, uploads;
  - `APP_DEBUG`, debug builds, `android:debuggable`; Telescope, Horizon, Debugbar, log viewers reachable without a
    gate; seed or test accounts that would work in production; `TODO`/`FIXME` near auth or validation;
  - redirects built from input (`redirect($request->...)`, `next`, `return`, `url` parameters); `config/cors.php`;
    `config/session.php` cookie settings; stack traces or SQL in production errors;
  - `eval`, `exec`, `shell_exec`, `Process::run` with input, `unserialize` on input, `Runtime.exec`;
    `usesCleartextTraffic`, `allowBackup`, user CAs in the network security config.
- **Odd code**: read what looks strange, half-built, legacy or "temporary"; when a comment says why something is safe,
  check the claim; try features together that were not designed together; note what the tests never exercise.

## 5. Verify, then write
Re-open every candidate cold, as if someone else had written it: re-read the lines, rebuild the trace, look once more
for a control. Then mark it confirmed, needs validation (the missing fact and the safe check) or rejected (one line
why). Write the findings only after this pass. "Not checked" lists every row still `not checked` plus the facts
outside source that the review depended on.
