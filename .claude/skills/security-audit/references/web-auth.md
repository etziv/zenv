# Web authentication, sessions and HTTP

Question for this file: can a request be credited to the wrong person, session, assurance level or token? Endpoint
permissions themselves are `core-classes.md` "Access control". Framework detail: `stack-laravel`
`laravel-security.md`.

Discipline: for every credential, find where it is verified and every binding it needs (user, session, audience,
expiry, single use). Headers such as Host, X-Forwarded-*, Origin and Referer are trust decisions: find who can set
them. A missing cookie flag, header or prompt alone is hardening; a finding needs an accepted bad request, another
user's impact, a weaker login than intended, or a leaked credential.

## CSRF and cookie sessions
- Every state change authenticated by a cookie needs the CSRF check. The `web` group has it; read the exceptions in
  `bootstrap/app.php` (`validateCsrfTokens(except: ...)`): each excluded route must authenticate some other way (a
  provider signature). Look for state changes on GET routes (`Route::get('.../delete')`, logout by GET).
- Sanctum SPA cookie auth: `SANCTUM_STATEFUL_DOMAINS` and `config/cors.php` together decide which origins may call
  with cookies; a broad list or pattern widens who can send credentialed requests.
- Inertia sends the `XSRF-TOKEN` cookie back as a header through its HTTP client: that is the expected setup.

## Session lifecycle
- `session()->regenerate()` after login and after a privilege change (2FA done, impersonation start or stop);
  `invalidate()` and `regenerateToken()` on logout.
- After a password change, account disable or role removal: other sessions and API tokens must stop working
  (`Auth::logoutOtherDevices`, the `AuthenticateSession` middleware, deleting Sanctum tokens). A banned user who stays
  logged in is a finding when the ban is meant to cut access.
- Cookie scope in `config/session.php`: a `domain` like `.example.com` shared with subdomains run by others, `secure`
  off on an HTTPS site, `same_site` `none`. Only a finding with a realistic attacker origin or network position.

## Tokens and API keys
- Sanctum personal access tokens: abilities checked where they matter (`tokenCan`, `abilities` middleware); tokens
  deleted on logout and password change; `sanctum.expiration` null means they never expire; tokens never in URLs.
- JWT (any library): algorithm fixed by the server, not read from the token; keys never chosen by token header fields
  (`kid`, `jku`, `x5u`); `exp`, `nbf`, `iss`, `aud` checked; no "decode without verify" path used for identity.
- API keys: bound on the server to a tenant, scope and environment; looked up by a hash of the whole secret; never in a
  frontend bundle. Every `VITE_*` variable is public in the built JavaScript.

## OAuth, Socialite, SSO
- `state` checked on callback (Socialite `stateless()` turns it off: acceptable only with another binding such as
  PKCE).
- Account linking by email: a social login that attaches to an existing account because the emails match, when the
  provider does not guarantee a verified email, is an account takeover. Linking a new provider needs the current
  session and, where policy says so, a password confirmation.
- If the app is the OAuth server (Passport): exact redirect URI match, PKCE for public clients, scopes enforced.
- Android sign-in: authorization code with PKCE; a redirect through a custom scheme can be caught by another app (see
  `android.md` deep links).
- SAML, if present: the signed element must be the one identity is read from, and the XML parser must refuse external
  entities. mTLS, if present: the backend must accept certificate identity headers only from its own proxy.

## Reset, verification, 2FA, account changes
- Custom reset or magic-link tokens: random, single use, short expiry, bound to one user and purpose, earlier tokens and
  sessions invalidated after use. The Laravel password broker already does most of this: check what was customized.
- Links in mails built from the request host: without trusted-host checks (`TrustHosts`, a fixed `APP_URL`) a forged
  Host header can make the reset link point to an attacker domain.
- Email or phone change, 2FA disable, recovery-code regeneration, API token creation: behind `password.confirm` or an
  equivalent fresh check when the product means them to be protected.
- 2FA (Fortify or custom): the second step cannot be skipped by calling another route; the code is bound to the
  pending login; codes and OTPs are throttled (a 6-digit code without a limit is a high finding).
- `verified` middleware on the routes that need a verified email.

## Proxies, hosts and HTTP caching
- `trustProxies(at: '*')` while the app is reachable without the proxy lets clients forge X-Forwarded-For (rate-limit
  keys, IP allowlists, logs) and the scheme. Whether the app is reachable directly is usually outside source: needs
  validation.
- Responses with personal data that a CDN, proxy or full-page cache could store for everyone: look at `Cache-Control`
  on those routes, response-cache packages and their cache keys, `cache.headers` middleware. Edge config outside the
  repo: needs validation.
- Request smuggling and parser differences matter only when the project ships its own proxy or HTTP parser: then
  compare how each side splits the same request.

## Rate limits as an authentication control
Login, reset, OTP and 2FA endpoints: a `throttle` or `RateLimiter::for` keyed by account and IP; the key must not be
spoofable (see proxies). A missing limit on a non-guessing endpoint is hardening; on a code-guessing one it is a finding.
