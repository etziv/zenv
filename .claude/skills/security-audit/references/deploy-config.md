# Deploy, config, storage, webhooks and queues

Discipline: a config file states intent, not what runs in production. Find which environment uses it and what
overrides it (`.env`, compose overrides, CI variables). A secret's name in a file is not a leak; a leak needs a reader
with less trust (a log, a response, an image layer, a public bucket). Real env values, firewall, DNS and bucket
policies are outside source: needs validation with the exact thing the owner checks. Deploy design itself: `web-deploy`.

## Environment and config
- Production expectations: `APP_DEBUG=false` (the debug page shows env values and code), `APP_ENV=production`, an
  `APP_KEY` that is not the one from `.env.example` or a tutorial.
- `config/*.php` defaults that fail open when a variable is missing (`env('REQUIRE_2FA', false)`, an empty allowlist
  read as "allow all"); `.env.example` with real credentials.
- `env()` called outside `config/` returns null once config is cached in production: a check like
  `$request->token == env('ADMIN_TOKEN')` then passes with an empty token. Grep `env(` outside `config/`.
- Debug and admin tools: Telescope, Horizon and Pulse gates (their `viewX` gate or `gate()` method) and whether they
  are enabled in production; Debugbar tied to `APP_DEBUG`; log viewers; health or metrics routes that print config.

## Containers and server
- Compose `ports:` that publish MySQL, Redis, Meilisearch or the queue dashboard on all interfaces (`"3306:3306"`);
  Docker's published ports are reachable even when a host firewall like ufw blocks them. Prefer no published port or
  `127.0.0.1:` binding. Redis or databases without a password on a reachable port.
- `privileged: true`, the Docker socket mounted into an app container, host paths mounted writable.
- Web server root at the project instead of `public/`; PHP execution allowed in upload folders; `.env` or `.git`
  servable. The server config may live outside the repo: needs validation.

## Storage and links
- Private files on the `public` disk or in `storage/app/public` (served after `storage:link`); S3 objects or buckets
  set public; `Storage::url` where `temporaryUrl` with a short expiry was meant.
- Signed and temporary URLs work for anyone who has them: check their lifetime and what they unlock.

## Webhooks and payment callbacks
- The provider's signature or callback token is verified (with `hash_equals`) before any effect, against the raw body
  where the provider signs the raw body.
- The amount, currency, status and order come from the verified payload or are fetched back from the provider, never
  from query parameters or unsigned fields; the order's expected amount is compared.
- Replays: the event or transaction id is recorded and a second delivery has no second effect; out-of-order deliveries
  cannot move a paid order back to pending or a refunded one to paid.
- The route is in the CSRF exceptions only because it is signed; a webhook that only checks a source IP depends on proxy
  headers (see `web-auth.md`).

## Queues, schedules and broadcast
- A job runs later with full access: authorization was checked at dispatch; check the job still holds when the user,
  record or permission changed in between (deleted user, revoked role).
- Retries: `tries`, `backoff`, `retryUntil` and idempotency for jobs that send money, mails or SMS.
- `failed_jobs` rows and the Horizon UI keep job payloads: secrets or personal data inside them are readable by whoever
  sees those.
- Scheduled commands that act on user-supplied data with no user context: same rules as the web path that produces it.
- Broadcast channel authorization: `client-side.md`.

## Outbound calls and secrets lifecycle
- TLS verification never disabled (`Http::withoutVerifying()`, `CURLOPT_SSL_VERIFYPEER => false`, `'verify' => false`).
- Secrets reaching logs, error trackers (Sentry with default PII sending on), process arguments or CI output; a secret
  committed once stays in git history and needs rotation, not only deletion.
- Cloud keys with wider rights than the feature needs (one S3 key for all buckets): a finding only when our input can
  steer that key to the wrong resource; otherwise Hardening or needs validation.
