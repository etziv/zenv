# Resource exhaustion and the operator's money

Discipline: a finding needs a path from input to cost, no effective limit on it, and a victim beyond the requester:
other users, the shared server, or the operator's paid quota. A missing rate limit alone is hardening: first look for
body and upload limits, validation maxima, pagination caps, queue limits, timeouts and quotas. Never test by sending
load; argue from the code and the numbers (input size allowed, work per unit, who pays).

## Paid and abusable sends
- Endpoints that send SMS, WhatsApp, OTP, mails or call paid APIs (maps, AI, payment checks) without a throttle keyed
  per account and per target number or address: a bot can spend the operator's money or flood a victim. Guest
  endpoints (register, OTP request, contact form, password reset) first.

## Unbounded work per request
- `paginate($request->per_page)` or `take($request->limit)` without a maximum; `->get()` or `->all()` on a table users
  can grow; `include`/`with` relations chosen by the client; synchronous exports of everything.
- Validation `max:` on strings, arrays (`array|max:100`) and nested arrays; loops or queries per submitted item.
- User-supplied regex, or a costly pattern applied to long input. In PHP, `preg_match` returns `false` (not 0) when it
  hits the backtrack limit: code that treats "not 1" as "valid" fails open.

## Uploads and parsing
- Upload size: the `max:` rule plus PHP and web server limits (`upload_max_filesize`, `client_max_body_size`, the last
  two outside source).
- Images: pixel dimensions, not only bytes (a small PNG can decode to gigabytes in Intervention/GD/Imagick); a
  `dimensions` rule or a check before resizing.
- Archives and xlsx (a zip): total uncompressed size and entry count checked before extracting or reading.
- XML from users: entity expansion and external entities in the parser settings.

## Queues and workers
- One user can enqueue many heavy jobs onto the queue everyone shares; a job that always fails retried forever or
  blocking a queue (`tries`, `timeout`, `retryUntil`, separate queues for heavy work).
- Outbound HTTP calls without a timeout, or long ones inside a web request, hold PHP-FPM workers; `Cache::lock` without
  an expiry; `lockForUpdate` held across an external call.
- Long-running processes (Octane, Reverb, queue workers): data added to static arrays or singletons on every request.

## Rate-limit keys
Limits keyed by IP behind a proxy: if the proxy headers are not trusted, every user shares one IP and one user can
lock out all; if they are trusted from anyone, the limit is bypassed (`web-auth.md` proxies). Prefer the user id or
the target (phone number, email) plus IP.

## Android client
The app is rarely the shared victim; check that its retries (WorkManager, sync loops, polling) use backoff and stop,
so one bug cannot hammer the backend from every installed phone at once.
