# Windows Compatibility Matrix

Last reviewed: 2026-10-16. "Verified" means it was exercised by Zenv's
tests against a real executable on Windows 11 x64.

| Component | Official Windows distribution | Zenv status | Notes |
|---|---|---|---|
| Apache HTTP Server | None from ASF; Apache Lounge (VS17/VS18 builds) is the de facto source | **Service management verified** (2.4.67 VS18) | Console mode (mpm_winnt). `httpd -k shutdown/restart` only targets an installed Windows service, so Zenv signals the parent via the `ap<pid>_shutdown` / `ap<pid>_restart` named events. mod_php requires a thread-safe PHP build with a compatible VC runtime; FastCGI via mod_proxy_fcgi + php-cgi is the alternative. |
| Nginx | nginx.org "nginx/Windows" zip | **Service management verified** (1.22.0) | nginx.org describes the Windows build as beta quality: select() only, limited connections; Zenv runs one worker. `nginx -t/-s stop/-s reload` with `-p`/`-c`. `-s quit` waits for browser keep-alive connections (over 10 s observed), so Zenv uses `-s stop` and terminates after 2 s. PHP only via FastCGI (php-cgi). |
| Caddy | Official GitHub release zips (amd64/arm64) | Version detection | Configured via Caddyfile/JSON; PHP via `php_fastcgi`. |
| FrankenPHP | Official Windows builds since v1.12.0 | Version detection | Worker mode and hot reload are supported on Windows. Bundles its own PHP (not a separately registered PHP) and embeds Caddy. Must be treated as its own server and runtime. |
| PHP | windows.php.net (TS/NTS, VS16/VS17) | Version detection | No PHP-FPM on Windows. |
| Go | go.dev zip/MSI with SHA-256 checksums | **Installable** (go.dev feed, checksum verified) + detection | Toolchain only, not a server. Installed under `C:\zenvin\go\<version>`; each terminal or process gets its own GOROOT. |
| Rust | rustup | Version detection | Zenv never modifies an existing toolchain without consent. |
| Ruby | RubyInstaller2 (+ MSYS2 DevKit) | Version detection | Native gems need the DevKit. |
| Python | python.org installers / Python install manager | Version detection | `WindowsApps\python.exe` App Execution Alias stubs are skipped during discovery: running one can open the Microsoft Store. |
| MySQL | dev.mysql.com ZIP archives | **Service management verified** (9.7.0) | Data directories are never initialized or overwritten without explicit confirmation (M5). XAMPP ships **MariaDB** as `mysqld.exe`; Zenv detects it from the version output and refuses to register it as MySQL. The started `mysqld` is a monitor process; setting the named event `mysqld<pid>_shutdown` stops it cleanly without credentials (verified). `upgrade=NONE` keeps MySQL from silently upgrading older data folders. |
| PostgreSQL | EDB binaries (linked from postgresql.org) | **Service management verified** (9.2.24) | `initdb` → `postgres -D` → `pg_ctl stop -m fast`. The data folder's postgresql.conf is used unchanged; port and listen address are passed on the command line. A data folder from another major version is refused. |
| Redis | **None.** Memurai is the commercial Windows partner; community MSYS2/Cygwin builds exist | **Service management verified** for user-registered builds (8.8.0 MSYS2, 5.0.14) | Redis 8 license: RSALv2 / SSPLv1 / AGPLv3. Zenv does not bundle or download Redis. MSYS2/Cygwin builds resolve `C:/…` as a relative path, so Zenv passes `/cygdrive/c/…` paths to them. Stop is `SHUTDOWN` over RESP (saves first). |

## Local domains on Windows

- Windows has no wildcard DNS for local development, so `*.test` domains need
  one hosts-file line each. Writing `C:\Windows\System32\drivers\etc\hosts`
  requires administrator rights; Zenv asks through UAC for that write only.
- `*.localhost` (RFC 6761) resolves to loopback inside Chromium/Edge, Firefox
  and curl without any system change. The Windows DNS client itself may not
  resolve it, so tools that use the system resolver need the hosts file.
- `.dev`, `.app` and other HSTS-preloaded TLDs force HTTPS in browsers; `.local`
  collides with multicast DNS. Zenv refuses them until local HTTPS exists (M6).

Only components with a verified installation provider are offered for install
(currently Go). Everything else is registered from an existing installation.

## Build toolchain note

This machine's Rust is `x86_64-pc-windows-gnu` without MSVC. Tauri's resource
compiler needs `windres`, and bundled SQLite needs a C compiler, so the build
uses a portable WinLibs GCC (MSVCRT, matching Rust's windows-gnu target):

```
ZENV_MINGW_BIN=D:\DevCache\toolchains\winlibs-gcc16-msvcrt\mingw64\bin
```

`scripts/with-toolchain.mjs` prepends this directory to PATH for `npm run tauri`
and `npm run test:rust`. An MSVC toolchain works too; then the variable is not needed.
