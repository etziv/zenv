# Browser side: React / Inertia (and web content in a WebView)

Discipline: a finding needs a source an attacker controls and a sink that runs or leaks it, and the impact must reach
a different user's session, a different origin, or storage that others read. Injecting into your own page is not a
finding. React text escaping, the same-origin policy, CSP and cookie SameSite are real controls: check them before
rating impact. The
native side of a WebView is `android.md`; server CSRF and sessions are `web-auth.md`.

## Rendering user content
- `dangerouslySetInnerHTML`: where does the string come from, and which sanitizer runs with which config (DOMPurify
  options that allow `style`, `iframe` or event handlers weaken it). Markdown components with raw HTML enabled (e.g.
  `rehype-raw`) are the same sink.
- `href`, `src`, `action`, `formAction` from user data: allowlist the scheme (`https:`, `mailto:`); do not rely on the
  React version's handling of `javascript:` URLs without checking it.
- Direct DOM writes through refs (`innerHTML`, `insertAdjacentHTML`), `eval`, `new Function`, `setTimeout` with a
  string, third-party widgets fed user HTML.
- Blade parts of an Inertia app (the root view, mails, PDFs) follow the Blade rules in `core-classes.md`.

## Inertia data exposure
- Every prop is in the page source (the `data-page` attribute or the JSON of a visit), visible or not. Passing a whole
  model (`'user' => $user`, `'order' => $order->load('customer')`) sends every column not in `$hidden`, including
  relations. Look for API Resources or `->only([...])` on props that hold other people's or internal data.
- `HandleInertiaRequests::share()` runs on every page, guest pages included: check what it shares when the user is
  not logged in, and that nothing like tokens, full user rows or config secrets is shared.
- Partial reloads and lazy props (`only`, `Inertia::lazy`/`optional`, deferred props) run server code on request:
  the same authorization as the full page applies.
- Errors and flash messages rendered as HTML.

## Navigation and redirects
- Client-side redirects from input: `window.location = params.get('next')`, `router.visit(url)` or `Inertia::location`
  with an external URL. Allow only relative paths or a host allowlist.
- Links that open a new tab with `window.open` and keep `opener`: only matters for older browsers or explicit opener
  use.
- Clickjacking: a state-changing action that completes in one click inside a frame, with no `frame-ancestors` CSP or
  `X-Frame-Options`. Missing headers on pages without such an action are hardening.

## Cross-origin and realtime
- `postMessage` receivers: exact `event.origin` comparison (not `includes`, `startsWith` or an unanchored regex) before
  acting on `event.data`; senders never post secrets to `*`.
- CORS (`config/cors.php`): an origin pattern that reflects attacker origins together with `supports_credentials`
  true, on routes that return private data or change state.
- Broadcasting (Reverb, Pusher, Echo): private and presence channel callbacks in `routes/channels.php` return true only
  for members; presence channels return only the user fields others may see; a WebSocket accepting cookies from any
  origin.

## Browser storage and state after logout
- Tokens in `localStorage` or `sessionStorage` matter when an XSS path exists or another account uses the same browser.
- Logout or account switch must clear client state: persisted stores (zustand/redux persist), query caches (React
  Query, SWR), IndexedDB, service-worker caches. Otherwise the next user on the device sees the previous one's data.
- Service worker / PWA: authenticated responses cached without the user in the key and served after logout or offline;
  the worker script or its imports loaded from a place users can write.

## Less common
- Prototype pollution: a deep merge or path-set of user keys (`lodash.merge`, `set`, query-string parsing into nested
  objects) reaching `__proto__`, plus a gadget that reads the polluted property for a security decision. Both halves or
  no finding.
- DOM clobbering: user HTML that keeps `id`/`name` attributes and code that reads a global by that name.
- Cross-site leaks: an attacker page that can tell a private fact (an object exists, a role) from load errors, frame
  counts or timing. Report only with a concrete secret predicate.
- Built assets: `VITE_*` values and production source maps are public; a secret there is a finding, code visibility
  alone is not.
