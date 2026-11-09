# Core attack classes (every server surface)

Each item: what to look for, then how to trace it here. Every hit still needs rules 1-4 of `SKILL.md`.

## Injection, including second order
- **SQL.** Values go through bindings; identifiers do not. Look for `DB::raw`, `whereRaw`, `orderByRaw`, `selectRaw`,
  `havingRaw` with interpolated input, and column or direction names from the request (`orderBy($request->sort)`,
  `where("meta->$key")`): they need an allowlist. Kotlin: Room `@RawQuery`, `SupportSQLiteQuery`, `rawQuery` built by
  string concatenation; a ContentProvider `selection` passed through.
- **Command.** `exec`, `shell_exec`, `system`, `proc_open`, `Process::run("... $x")` (the array form avoids the shell),
  `Runtime.exec`. Trace whether any part comes from a user, a file name or a stored value.
- **HTML and templates.** Blade `{!! !!}`, `Blade::render` or `compileString` on user strings, mail templates with raw
  output, Markdown rendered to HTML (check the renderer's raw-HTML and unsafe-link options, e.g. `Str::markdown`).
- **Paths.** `Storage::get/put/delete`, `response()->download/file`, `File::` with a path or name from input, the client
  file name (`getClientOriginalName()`) used as a path, zip extraction that does not reject `../` entries.
- **Deserialization.** `unserialize` on cookies, input or cache values others can write; Kotlin `ObjectInputStream`;
  polymorphic JSON that lets input pick the class.
- **Second order.** A value stored safely, later used in a riskier place by other code: a display name printed raw in
  an admin mail, a slug that becomes a file path, a saved URL fetched by the server, a field name used as a column or
  JSON path, text used as a regex. Follow stored fields to every reader (`memory.mjs uses "<field or accessor>"`).

## Access control
- **Authenticated is not authorized.** A route under `auth` with no policy or gate check; `authorize()` in a FormRequest
  returning `true`; `Gate::before` granting too much; route model binding that loads any id with no ownership check.
  Safe forms: `$this->authorize('update', $post)` or `Gate::authorize`, a query through the owner
  (`$request->user()->posts()->findOrFail($id)`), `scopeBindings()` for nested routes.
- **The same effect through a weaker door.** List every path to the state change: web and API controllers,
  Inertia/Livewire actions, jobs, console commands, imports, admin screens. Each must apply the same rule.
- **Request fields that pick authority.** `$request->all()` or `validated()` with fields the user must not set reaching
  `create`, `update`, `fill` or `forceFill`; `$guarded = []`. Watch `role`, `is_admin`, `user_id`, `team_id`,
  `status`, `price`, `approved_at`.
- **Bulk operations.** `whereIn('id', $ids)` or loops over submitted ids without scoping each one to the user; exports
  and batch actions that skip the per-item policy.
- **Hidden in the UI only.** An Inertia page or Android screen that hides a button while the route still accepts the
  call; a role or "premium" flag the app keeps locally. The server must decide; trace the call to the backend.
- **Signed links.** The `signed` middleware actually on the route; expiry on `temporarySignedRoute`; a signed link is a
  bearer link: what does it grant if forwarded?

## Files and server-side fetching
- **Uploads.** Validation of type by content (`mimetypes`, `image`), size (`max`), and the stored name (generated, not
  the client's). A file stored on the public disk with a servable type (`.php`, `.html`, `.svg`) becomes code or stored
  XSS. Private files need a private disk and a download route with an authorization check.
- **SSRF.** `Http::get($url)` or Guzzle with a user URL, webhook or callback URL settings, import-from-URL, PDF or image
  renderers fed user HTML or URLs (they can read `file://` and internal hosts). Check the target after redirects and
  against loopback, private ranges, the cloud metadata address and compose service names (`mysql`, `redis`).
- **Check then use.** A file or record checked, then used after it could have changed (temp files, renames, status read
  before an update without a lock).

## Secrets and cryptography
- **Randomness.** `rand`, `mt_rand`, `uniqid`, `java.util.Random` for tokens or codes are guessable; `Str::random`,
  `random_int`, `random_bytes`, `SecureRandom` are fine.
- **Hardcoded secrets.** In code, as `env('KEY', 'real-value')` defaults in `config/*.php`, in `BuildConfig` fields,
  `strings.xml`, committed `local.properties`. Decide whether the value grants anything (a public client id does not).
- **Secrets where others read them.** `Log::info($request->all())`, exception context, Inertia props, API responses of
  models without `$hidden` (`password`, `remember_token`, `two_factor_secret`), URLs with tokens (they land in logs and
  Referer headers).
- **Comparison and signatures.** Token or signature compared with `==` instead of `hash_equals`; an HMAC computed but
  its result not enforced; a decrypt or verify failure that falls back to accepting the data.
- **Own crypto.** Home-made encryption instead of `encrypt()`/`Crypt`; Android `Cipher.getInstance("AES")` (provider
  default mode), a fixed IV, a key stored next to the data it protects.

## Business logic
- **Steps out of order.** Reaching a later state without the earlier one (paid without payment, approved without
  review), replaying a finished flow, going backwards. Find the status checks in the transition code, not the UI.
- **Half-done writes.** A multi-step change without `DB::transaction`; an external call between two writes.
- **Races.** Check-then-act on balance, stock, coupon or invite use, double approval. Look for `lockForUpdate`, an
  atomic `where('status', 'pending')->update(...)` with an affected-rows check, or a unique index.
- **Numbers.** Negative or zero quantities and prices, floats for money, very large values, numeric strings, rounding
  that favours the user, currency mixups.
- **Time.** Expiry at the exact boundary, app vs database timezone, client-supplied timestamps.
- **Fail-open defaults.** A missing config value, a feature flag off, a failed dependency or a `catch` that returns
  "allowed". Trust in stored data that another path (import, admin, API) wrote without the same validation.

## Features used against their purpose
- **Export, backup, report.** Includes other users', hidden or deleted data, or is downloadable by anyone with the link.
- **Import and restore.** Overwrites existing records, sets ids or owners, skips the form validation.
- **Search, filter, sort.** Filters or sorting by a field the user cannot see reveal its value; counts reveal what
  exists.
- **Enumeration.** Different answers for "no such account" and "wrong password" in login, reset or register, only when
  the existence itself is sensitive; 403 vs 404 on private objects.
- **Preview and draft links.** Tokens that open more than one item, or drafts reachable through lists, feeds or search.
- **Server-fetched URLs.** Notification, avatar or webhook URLs set by users (see SSRF).

## Chains
Two allowed steps can make one forbidden result: a user-set field that an admin page renders raw; an upload later
included or served from the app's origin; a role change that leaves old tokens working; an undo or restore that skips
today's authorization. Confirm each link with its own trace before reporting the chain.
