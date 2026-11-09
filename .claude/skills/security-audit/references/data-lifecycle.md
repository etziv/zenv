# Data isolation and lifecycle

Question for this file: follow one protected record through every copy and every state change. Who can reach it in
each place? A `user_id` or `team_id` column is not isolation; the query, key, path or policy that uses it is. Check each
read and write path, then each copy (cache, search, export, log, backup, local phone database), then deletion and
restore. A privacy wish is not a vulnerability; a finding needs a stated access rule or deletion promise and a reader
or action that breaks it.

## Owner and tenant scope
- Every read, update, delete, list, count and aggregate binds to the current user or team. Compare: the controller
  (`$team->projects()`), route model binding, the API twin, jobs and console commands (no logged-in user there: does a
  global scope then return everything?), admin screens, raw `DB::table` queries that skip model scopes.
- Global scopes and their escapes: `withoutGlobalScopes()`, `withoutGlobalScope(...)`, `withTrashed()`, queries on a
  pivot or a related model that has no scope of its own.
- Tenancy packages (stancl/tenancy, spatie/multitenancy): cache, queue, storage disk, search and broadcasting tenant
  aware, not only the database.
- Shared names: cache keys (`Cache::remember("report:$id")`), storage paths (`uploads/$originalName`), temp files and
  search document ids without the user or tenant, so two owners read or overwrite each other's entry. Unique rules
  (`unique:users,email`) scoped as the product intends.
- Counts, totals, "already taken" messages and dashboards that reveal other owners' data.

## Copies of the data
- Caches: permissions or menus cached per role and not cleared when the role changes; pages or API responses cached
  without the user in the key.
- Search (Scout, Meilisearch, Algolia): the index holds only fields the searchers may see, and the tenant or owner
  filter is applied at query time (tenant tokens or a server-side filter), not only when indexing.
- Logs, error trackers, analytics, mails and notifications carrying personal data or tokens to readers with broader
  access or longer retention; a mail or push sent to the wrong recipient after an ownership change.
- Generated files: PDFs, exports and thumbnails stored on a public disk or under guessable names.
- Android: Room tables and caches keyed per account, and wiped on logout or account switch (`android.md`).

## Export, import, backup
- Exports and reports: per-row authorization after selection, hidden columns (`$hidden` is not applied to `DB::table`
  rows or hand-built arrays), soft-deleted rows, and an authorized download of the finished file.
- CSV/Excel exports with user-controlled cells starting with `=`, `+`, `-`, `@` become formulas when opened: medium at
  most, and only where staff open the files.
- Imports (Laravel Excel and similar): the same validation as the form, no ids or owners taken from the file, no
  silent overwrite of other owners' rows (`upsert` keyed on an id from the file).
- Backups (`spatie/laravel-backup`, dumps): where they are stored, who can download them, whether `.env` is inside.

## Deletion, revocation, restore
- Soft deletes: route binding or relations that still return trashed rows, background jobs acting on deleted records,
  a deleted account's email reusable while old references still point to it.
- Account deletion: tokens, sessions, uploaded files, queued jobs, search entries and third-party copies the product
  promises to remove.
- Removing a member or downgrading a role: cached permissions, existing Sanctum tokens, open sessions, broadcast
  channel subscriptions and pending invites stop granting access.
- Undo, restore and "reopen": re-check today's ownership and permissions instead of restoring old ones.

## Data migrations
A new column's default that opens data (`is_public` default true), a backfill that assigns rows to the wrong owner, or
a nullable `tenant_id` that some query treats as "visible to all". Old and new code reading the same rows with
different rules during a rollout.
