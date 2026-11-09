# Zenv

A Windows local development environment manager: runtimes, web servers,
databases and local sites from one desktop app. Built with Rust + Tauri 2,
React, TypeScript, Vite, Tailwind CSS and shadcn/ui.

- Architecture and module plan: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Verified Windows compatibility per component: [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)

## Status: Foundation, Local Sites, PHP per site, installers, HTTPS

| Works now | Verified by |
|---|---|
| SQLite metadata store with versioned migrations, backup before upgrade, refusal to downgrade | `storage` unit tests |
| Catalog of 12 components with Windows support status and caveats | `runtime_catalog` tests |
| Discover, register, re-detect, set default, and unregister existing installations of all 12 components (version probes are specific to each component) | probe tests with real version output; detection against real binaries |
| Process manager: Job Object per service, kills the whole process tree, captures logs, records exit codes | `process_manager` tests (Windows) |
| Port inspector with owning PID and process name, plus excluded-range detection | `port_manager` + `platform` tests |
| Apache: generated config, `httpd -t` validation, start, readiness check, graceful reload and stop, logs | **`apache_e2e` against Apache Lounge 2.4.67** |
| Diagnostics, Zenv log, per-service activity history | unit tests |
| Local sites: unlimited, searchable and paged; project-type detection; static or reverse-proxy serving; enable, duplicate, delete (project files untouched); per-site diagnosis and logs | **`sites_e2e` against Apache**; `local_sites` tests (5,000 rows) |
| Apache virtual hosts: staged `httpd -t` validation, graceful reload, restore on failure | `sites_e2e` |
| Hosts file: Zenv-owned block, UAC only for that write, `.localhost` needs no entry | `hosts` tests (merge, helper validation on a temp file) |
| Apache **or** Nginx on the same port: switching one on switches the other off; sites can follow whichever server is running | **`nginx_e2e`** (Apache 2.4.67, Nginx 1.22.0) |
| Zenv home `C:\zenv` (bin, data, etc, logs, dev, backups): install versions from the official source with checksum verification (Go first); existing installations still detected and used; versions coexist via per-process environments | **`coexist_e2e`** (Go 1.27.2 + 1.26.9 from go.dev next to system Go; MySQL 9.7 + 9.3 and Redis 8.8 + 5.0 running at once) |
| MySQL, PostgreSQL and Redis servers: on/off switches, data folders that are only initialized on request (never re-initialized or deleted), connection details, protocol-level connection tests, graceful stops | **`db_e2e`** (MySQL 9.7, PostgreSQL 9.2, Redis 8.8 MSYS2 + 5.0) |
| PHP per site: any registered PHP version through Apache + PHP-CGI worker pools; FrankenPHP chosen automatically for the PHP version it embeds; on/off switches for installations and servers | **`php_e2e`** (PHP 8.4 + 8.1, FrankenPHP 1.12.2 / PHP 8.5) |
| Install every language from its official source into `C:\zenv\bin`: PHP (windows.php.net, NTS x64 with a ready php.ini), Go (go.dev), Python (python.org, with pip and venv), Node.js (nodejs.org zips checked against SHASUMS256, npm included), Ruby (RubyInstaller2 7z), Rust (rustup-init into its own RUSTUP_HOME/CARGO_HOME, never touching `~/.rustup`) | **`languages_e2e`** (PHP 8.5.11, Python 3.15.0, Ruby 4.0.7, Rust 1.99.1 building a program, Node.js 24.22.0 with npm) |
| Install MySQL (cdn.mysql.com, Oracle's code signature checked on every program and library) and PostgreSQL (EDB, SHA-256 cross-checked with the one Scoop recorded); put one version of a language or database on the user PATH, refused while another folder on PATH has the same programs | **`databases_install_e2e`** (MySQL 26.10.0, PostgreSQL 18.6 installed, initialized, started) |
| Docker apps: a `.test` domain proxied to a container's published port; the container (or its compose service) starts with the app and stops on request; Docker Desktop started when needed; container log as the app's output | **`docker_apps_e2e`** (plain container and compose service: list, start, status, autostart, stop, Stop all, log; user's containers untouched) |
| AI routers (OmniRoute, 9router) as Tools services: installed from npm (sha512-checked, dependencies as of the release, no install scripts but the router's own) with Zenv's Node.js, kept on 127.0.0.1, data in the service's folder, dashboard and its pages in a Zenv window, auto-update to releases at least a week old | **`ai_router_e2e`** (per router: install, run on loopback only, update to the newest release) |
| Open an app to your other devices on Tailscale (`tailscale serve`, per-app port, tailnet only) | **`tailnet_e2e`** (Apache, Nginx, FrankenPHP; php-cgi, FrankenPHP-backend and proxy apps) |
| Install web servers from their official Windows sources with checksum verification: Apache (Apache Lounge, SHA-256 from its `.zip.txt`), Nginx, Caddy and FrankenPHP (GitHub releases with SHA-256 digests); FrankenPHP's own PHP is registered automatically | **`webservers_e2e`** (Apache 2.4.69, Nginx 1.30.6 started; Caddy 2.11.7, FrankenPHP 1.13.1 / PHP 8.5.11) |
| HTTPS: a local certificate authority created and trusted only on request (Windows asks to confirm), name-constrained to local suffixes; per-site certificates issued and renewed automatically; Apache `mod_ssl` and Nginx `ssl` listeners only while a site uses HTTPS; optional HTTP→HTTPS redirect | **`https_e2e`** (Apache 2.4.67, Nginx 1.22.0, verified with curl against the CA) |

Not built yet: Caddy as a front server; managed installers for servers and databases.

## Requirements

- Windows 10/11 x64 with WebView2
- Node.js 22+
- Rust (stable). With the `x86_64-pc-windows-gnu` toolchain you also need a
  MinGW-w64 GCC that provides `gcc` and `windres` (see COMPATIBILITY.md).
  Set `ZENV_MINGW_BIN` to its `bin` folder. MSVC toolchains need nothing extra.

## Commands

```powershell
npm install
npm run tauri dev                 # run the app
npm run tauri build               # NSIS installer
npm test                          # frontend unit tests
npm run typecheck
npm run test:rust                 # core unit tests
$env:ZENV_TEST_APACHE_ROOT = "C:\path\to\Apache24"
$env:ZENV_TEST_MYSQL_ROOT = "C:\path\to\mysql"       # also ZENV_TEST_POSTGRES_ROOT, ZENV_TEST_REDIS (';'-separated)
$env:ZENV_TEST_NGINX_ROOT = "C:\path\to\nginx"
$env:ZENV_TEST_PHP = "C:\php-8.4;C:\php-8.1"            # folders with php-cgi.exe
$env:ZENV_TEST_FRANKENPHP_ROOT = "C:\path\to\frankenphp"
npm run test:e2e                  # real Apache / PHP / FrankenPHP tests
```

## Installing

`npm run installer` builds it and puts it at `installer/Zenv-setup.exe` (the bundle itself is `src-tauri/target/release/bundle/nsis/Zenv_<version>_x64-setup.exe`).
It installs for the current user (no administrator rights) into `C:\zenv` by
default, which is also Zenv's home; choosing another folder (for example
`D:\zenv`) makes that folder the home. Drive roots, Windows and Program Files
are refused. Silent install: `Zenv-setup.exe /S /D=D:\zenv`.

Uninstalling removes the app files only. `bin`, `data`, `etc`, `logs`, `dev`,
`backups` and `zenv.db` are kept, so reinstalling picks everything up again.

The installer is Tauri's NSIS template with one change (the default folder),
in `src-tauri/windows/installer.nsi`, plus `src-tauri/windows/hooks.nsh`.
After upgrading `@tauri-apps/cli`, run `npm run check:nsis`.

## Data and safety

Everything Zenv installs or manages lives in its home, `C:\zenv` by default
(the folder Zenv is installed into; `ZENV_HOME` overrides it): `bin\<component>\<version>` for installed versions,
`data` for database data, `etc` for generated configuration, plus `logs`, `dev`,
`backups`, `downloads` and `zenv.db`. Projects can live anywhere.

Zenv never edits the system PATH, never modifies a server installation's own
config files, never deletes project files, data folders or installations it
did not install, and only overwrites files listed in its managed-file ledger,
keeping a backup of the previous content.
