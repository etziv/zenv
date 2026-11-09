# Android: components, intents, deep links, WebView, local data

Discipline: name the attacker's starting point, because "local" is not one thing: another installed app, a web page
(in a browser that opens a link, or inside our WebView), a received file or share, or someone holding an unlocked or
lost phone. A package name, an action string or a field the caller fills in is not identity; real identity is a
`signature`-level permission, `Binder.getCallingUid()` / `getCallingPackage()` checked against an allowlist, or the
platform's own grant. Read the **merged** manifest and the `targetSdk`: libraries add components, and defaults change
by API level. Packaging facts you cannot see (Play signing, App Links verification on the domain) are needs
validation. The checklist in `platform-android` and `stack-android` `review-kotlin.md` stay the base; this file adds the
attack view.

## Exported components
- Which components are reachable by other apps: `android:exported="true"`, or any component with an intent filter
  (since API 31 it must say `exported` explicitly; a component without filters defaults to not exported). For each,
  read what it does with the extras and data it receives.
- **Intent redirection.** An exported activity or service that takes an `Intent` (or a URI) from extras and starts it
  (`getParcelableExtra<Intent>(...)`, `startActivity(extra)`, `setResult` with a received intent): another app gains
  our app's access to non-exported components and URI grants. Look for nested intents forwarded without checking the
  target component.
- **Permissions on components.** A custom permission with `protectionLevel="normal"` or `dangerous` can be requested
  by any app; for app-to-own-app calls use `signature`.
- **Runtime receivers.** `registerReceiver` needs `RECEIVER_NOT_EXPORTED` unless outside broadcasts are intended (API 34
  targets must pass a flag). A receiver that changes state on an action any app can send is a finding.
- **Content providers.** `exported`, `grantUriPermissions`, read/write and path permissions; `query` building SQL from
  `selection` or `sortOrder`; `openFile` resolving a path from the URI without canonicalizing (`../` reads app files).
- **FileProvider.** `res/xml/*paths*.xml` with `root-path`, `path="."` or the whole files dir shares far more than the
  feature needs; a grant handed to an app the user did not choose.
- **PendingIntent.** One handed to a notification, widget, alarm or another app should be `FLAG_IMMUTABLE` with an
  explicit component. A mutable one wrapping an implicit intent lets the receiver redirect it under our identity.
- **Implicit intents and broadcasts with data.** Tokens, personal data or file grants sent in an implicit intent or a
  broadcast without a permission can be received by any app that declares the filter.

## Deep links and App Links
- Any app can claim a custom scheme (`myapp://`). `https` App Links with `autoVerify="true"` are bound to the domain,
  but the `assetlinks.json` on the server is outside source: needs validation.
- A deep link that acts without the user (logs in with a token from the URL, confirms a payment, changes settings,
  accepts an invite) must require the current session and a confirmation, and a one-time server-side token.
- Link parameters flowing into a WebView URL, a file path, an API host or an intent: trace each to its sink.
- OAuth or payment redirects through a custom scheme without PKCE or a server-checked state can be caught by another
  app.

## WebView
- What it loads: a fixed bundled page (`WebViewAssetLoader`) or a URL from an intent, deep link, push payload or API.
  An attacker-chosen URL in a WebView that has a JavaScript bridge or a session cookie is the main path to check.
- `addJavascriptInterface`: every `@JavascriptInterface` method is callable by any page the WebView ends up showing,
  including redirects and iframes. The origin must be checked on each call or navigation (`shouldOverrideUrlLoading`),
  by parsed host equality, not `startsWith`/`contains` (`https://ourapp.com.evil.net`).
- Bridge methods that read files, return tokens, start intents or call the API with the user's session: rate them by
  what an untrusted page gains.
- Settings: `setAllowFileAccess`, `setAllowFileAccessFromFileURLs`, `setAllowUniversalAccessFromFileURLs`,
  `setWebContentsDebuggingEnabled(true)` in release, mixed content allowed. A loose setting with no protected content in
  reach is hardening.
- `onReceivedSslError` calling `handler.proceed()`: any network attacker can read and change the traffic.

## Network
- `usesCleartextTraffic="true"` or `cleartextTrafficPermitted` in `network_security_config.xml` for hosts that carry
  tokens or personal data; `<certificates src="user"/>` in the release config; custom `TrustManager` or
  `HostnameVerifier` accepting everything; an OkHttp client built without the default checks.
- The API base URL or a host the token is sent to coming from an intent, deep link or remote config.

## Local data and credentials
- Tokens and personal data in plain SharedPreferences, DataStore, Room or files: readable through backups when
  `allowBackup` is true or `dataExtractionRules`/`fullBackupContent` include them; prefer a Keystore-held key and
  excluding them from backup. Plain storage readable only by our own app on a normal phone is not a finding alone.
- External or shared storage (`getExternalStoragePublicDirectory`, MediaStore, Downloads) is readable by other apps.
- Logs: `Log.*`, Timber trees or OkHttp `HttpLoggingInterceptor` at `BODY` in release builds printing tokens or
  personal data.
- Sensitive screens: `FLAG_SECURE` where screenshots or the recents preview would show secrets; OTPs or passwords put
  on the clipboard; notification content visible on the lock screen.
- `BiometricPrompt` without a `CryptoObject` is a UI gate only: fine for convenience, not for protecting a key or token
  from someone holding the unlocked phone.
- Logout and account switch: Room tables, caches, WorkManager jobs, notifications, widgets and WebView cookies
  (`CookieManager`) cleared, so the next account sees nothing of the previous one.

## The server stays the authority
Anything decided only in the app (role, premium flag, price, feature flag, "already paid") can be changed by a user
with a modified app: trace the API call and check the backend enforces it (`core-classes.md` access control). API keys
in `BuildConfig` or resources can be extracted: the backend must not treat them as a user's identity.

## Release configuration
`android:debuggable` off; signing passwords and keystores not in the repo or `build.gradle`; R8 obfuscation is not a
security control. Native code (NDK) parsing untrusted data: see the pointer in `SKILL.md`.
