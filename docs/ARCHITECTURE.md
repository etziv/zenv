# Zenv Architecture

Zenv is a Windows-first local development environment manager built with
Rust + Tauri 2 (backend) and React + TypeScript + Vite + Tailwind + shadcn/ui
(frontend). It manages runtimes, web servers, databases and local sites through
narrow, typed backend operations. The frontend never executes processes or
touches the filesystem directly.

## Layering

```
React feature modules (src/features/*)
        │  typed invoke() wrappers (src/lib/api.ts)
        ▼
Tauri shell: src-tauri/src/commands.rs      ← narrow, validated, no raw exec
        ▼
zenv-core crate (src-tauri/core): Core owns Db, ServiceManager, SiteManager, AppPaths
        ▼
Domain modules ─ runtime_catalog · runtime_manager · service_manager · web_server
               · local_sites · virtual_host · domain_manager (+ hosts)
               · config_generator · port_manager · process_manager · log_manager
        ▼
storage (SQLite + migrations)     platform/windows (Job Objects, IP Helper, events)
```

## Crates

- `src-tauri` (`zenv`): the Tauri shell. It holds window setup, plugins, and the command surface, and nothing else.
- `src-tauri/core` (`zenv-core`): all domain logic, with no GUI dependencies. Keeping it separate lets the core be unit- and integration-tested on its own. It also sidesteps Windows test executables failing to load `comctl32` v6 entry points that only the app manifest provides.

## Modules (zenv-core)

| Module | Responsibility | Status |
|---|---|---|
| `storage` | SQLite connection, `PRAGMA user_version` migrations, pre-migration backups (`VACUUM INTO`), downgrade refusal | M1 |
| `runtime_catalog` | Static, verified metadata per component: category, Windows support, distribution source, caveats, integration status | M1 |
| `runtime_manager` | Installation registry; per-component version probes; discovery of existing installs; register/unregister (never deletes files) | M1 |
| `process_manager` | Spawns allow-listed executables only, one Windows Job Object per service (kill-on-close), log capture, exit tracking | M1 |
| `web_server` | `WebServerAdapter` trait; `apache` adapter (config render, `httpd -t` validation, named-event graceful stop/reload) | M1 (Apache) |
| `config_generator` | Server-specific minijinja templates (`src-tauri/templates/<server>/`), managed-file tracking, staged validation, atomic replace, backups | M1 |
| `service_manager` | Service records, start/stop/restart orchestration, port pre-checks, readiness probing, event history | M1 |
| `port_manager` | Listener table (GetExtendedTcpTable) with owning PID/process name, bind probe for reserved ranges, inter-service conflicts | M1 |
| `log_manager` | Allow-listed log sources per service, bounded tail reads | M1 |
| `platform/windows` | Job Objects, TCP owner tables, process image names, named events, UAC elevation (`ShellExecuteExW runas`) | M1–M2 |
| `local_sites` | Site records (indexed SQLite table, paged search), input validation, project-type detection, diagnosis; every mutation applied together with its config or rolled back | M2 |
| `virtual_host` | Apply engine: render all enabled sites with the server's serializer into staging, validate, sync through the ledger, reload, verify, restore on failure | M2 |
| `php_runtime` | PHP per site: php-cgi worker pools per PHP version (supervised, restarted when they exit) and FrankenPHP instances with a generated Caddyfile; engine selection | M2+ |
| `database_manager`, `cache_manager` | MySQL / PostgreSQL / Redis adapters: data-folder inspection and explicit initialization, graceful stops, connection info, protocol probes | M5 |
| `runtime_installer`, `runtime_env` | Official-source installers with checksum verification (PHP, Go, Python, Ruby, Rust, Node.js); per-process environments so versions coexist | M6 |
| `tailnet` | Opening apps to the user's Tailscale devices through `tailscale serve` and a per-app loopback port | M6 |
| `tls` | Local certificate authority (rcgen, name-constrained, folder restricted to the current user), trust/untrust in the current user's Root store via CryptoAPI (Windows confirms), per-site certificates reissued when missing, expiring, renamed or from another CA | M6 |
| `domain_manager` (+ `hosts`) | Domain rules (`.test` default, HSTS TLDs refused), resolution strategy, hosts-file block planning/merging, elevated helper | M2 |
| Nginx / Caddy / FrankenPHP adapters | Separate adapters and templates | M3 |
| Language providers (PHP, Go, Rust, Ruby, Python) | Version providers, per-project selection, project processes | M4 |
| `database_manager`, `cache_manager` | MySQL, PostgreSQL, Redis lifecycle; data-dir protection | M5 |

Modules for later milestones do not exist yet as code. They are added when
they get a real, tested implementation, not as empty placeholders.

## Key design decisions

- **Zenv never edits a user's server installation.** It generates its own
  main config per service under `C:\zenv\etc\<service>\`
  and launches the server with `-f <that file>`. The installation's own
  `conf/httpd.conf` stays untouched.
- **Validate before activation.** Generated config is written to a staged file,
  validated with the server's own checker (`httpd -t -f staged`), and only then
  atomically replaces the active file. A failed validation leaves the previously
  active config in place, so no rollback is needed.
- **Managed-file ledger.** Every file Zenv writes is recorded in
  `managed_files` with its SHA-256. Zenv refuses to overwrite a file it does
  not own. When it replaces content that changed, it first copies the old
  content to `backups/`.
- **Process ownership.** Each service runs in its own Job Object with
  `KILL_ON_JOB_CLOSE`. Stop is graceful first (Apache: `ap<pid>_shutdown` named
  event), then `TerminateJobObject` after a timeout. If Zenv crashes,
  Windows closes the job handle and kills only Zenv's own processes.
- **Ports.** Server listen ports are distinct from (future) project upstream
  ports. Before start: duplicate address:port across Zenv services is
  rejected, the OS listener table is checked (owner PID and name are reported),
  and a bind probe catches Windows excluded port ranges.
- **Typed errors.** `AppError` serializes to `{ code, message, details }`, and
  the UI renders diagnostics from `details` (log tails, port owners, checker output).

## Local sites (M2)

- **Declarative model, server-specific serializers.** A site becomes a
  `SiteModel` (domain + `Static { document_root }` or
  `ReverseProxy { upstream }`). Each adapter renders it with its own template
  (`templates/apache/site.conf.j2`); nothing is shared across server formats.
- **Activation per web server** (`virtual_host::apply_service`): render all
  enabled sites into `sites.staged/` (plus any foreign `.conf` already
  included), check a main config pointing at the staging folder with
  `httpd -t`, and only then write the real files through the ledger. Then
  graceful reload, and confirm the server is still alive and accepting
  connections. If it died, the previous files are restored and the server is
  restarted. Mutations that don't change the set of active sites (disabled
  sites) skip activation.
- **Default virtual host.** The main config declares a `ServerName localhost`
  vhost before the site includes, so unknown domains get the server's default
  page instead of an arbitrary site.
- **Sites never leak PHP sources or hidden files.** Static sites answer
  `.php`/`.phtml`/`.phar` with 403 instead of serving them as text. Hidden
  files and folders are refused at any depth (`.env`, `.git/config`,
  `sub/.env`), also under their Windows short name (`GIT~1`); folders are not
  listed (no `Indexes`, no `browse`).
- **Domain resolution strategies.** `*.localhost` names resolve to loopback in
  browsers without any system change. `.test` (default), `.example` and
  `.invalid` need a hosts entry. Every other ending is refused: real names on
  the internet or on networks (`github.com`, a router's `.lan`, a company's
  `.internal`), HSTS-preloaded TLDs (`.dev`, `.app`, …) and `.local`.
- **Hosts file.** Zenv owns one `# BEGIN zenv … # END zenv` block and never
  edits other lines: entries the user already has are not duplicated, and
  conflicting ones are reported and left alone. Writing happens only from an
  explicit “Update hosts file” action. Zenv saves a backup, then starts
  `zenv.exe --zenv-apply-hosts <request>` through UAC. The elevated helper
  accepts only `127.0.0.1`/`::1` entries for valid local domains, and
  recomputes the merge itself. Declining UAC changes nothing, and the UI offers
  the manual lines and the `.localhost` alternative. Afterwards Zenv re-reads
  the file to confirm the change landed.
- **Scale.** Listing is paged and searched in SQL (5,000 sites: under 2 s in the
  unit test). Activation renders only enabled sites, and the real-Apache test
  creates 300 sites and activates 4 in about 5 s.

## Web servers sharing a port

- Web servers may share a port and all run at once, each serving the apps
  bound to it. The first one started holds the port; one started later runs
  on an internal 127.0.0.1 port (`ServiceManager::host_behind`, never
  persisted) and the front server gets a proxy block per app bound to it
  (Host kept). HTTPS for those apps ends at the front server.
- "Any server" apps are served by the front, except ones with Apache-only
  `.htaccess` rules, which go to an Apache running behind a non-Apache front.
- Stopping the front hands the port to the servers behind it
  (`Core::stop_service`): the first takes it, the rest run behind it again.
  "Start apps" starts the server Apache-only apps need first, so it fronts.
- Verified by `tests/shared_port_e2e.rs` (Apache 2.4.67 + FrankenPHP 1.12.2
  in both orders, hand-over on stop) and `tests/nginx_e2e.rs`.
- A site's web server is optional (`sites.service_id` NULL, migration v4):
  such a site is written into **every** web server's configuration, so
  whichever server is running serves it. Hosts entries point to 127.0.0.1.
- Nginx adapter: its own `nginx.conf` per service (`-p` = the generated folder,
  temp paths under it), server blocks per site, PHP through an `upstream` of
  php-cgi workers with Laravel's `try_files`, FrankenPHP and apps via
  `proxy_pass` with Host and Upgrade headers.
- Graceful stop timeouts are per adapter (Apache 4 s, Nginx 2 s). Browsers'
  keep-alive connections otherwise hold a shutdown for 10 s or more, and both
  servers write unbuffered logs, so nothing is lost by terminating.
- Verified by `tests/nginx_e2e.rs`: one port, Nginx on → static, PHP 8.4,
  PHP 8.1, FrankenPHP (PHP 8.5) and proxy sites all served; Apache joins
  behind Nginx; Nginx off → Apache takes the port and serves the same sites;
  Nginx back on behind Apache with a site added while both run.

## Programs behind proxy apps

- A proxy app (Rust, Go, Node... on its own port) can have a start command;
  without one, Zenv uses the project's usual one (`Cargo.toml` -> `cargo run`,
  `go.mod` -> `go run .`, `package.json` -> `npm run dev` / `npm start`, with
  pnpm/yarn/bun picked from the lock file). Stored in `app_commands`
  (migration v8); `.env` is only read.
- A detected command never runs by itself: adding a folder (a freshly cloned
  repository, say) must not run its code. The panel shows it with what it
  runs (`npm run dev` → `vite`); pressing Start or saving it approves it, and
  only then does it start with the web server. Apps from before this rule had
  their detected command saved once (migration v13, `adopt_detected`).
- Commands run without a shell, in the project folder, only for known tools
  (`app_process::ALLOWED`), with each language's default version first on
  PATH. Output goes to `logspps\<site>.log` (the app's "Output" log).
- Ports belong to Zenv: a new proxy app gets the nearest free port from its
  kind's usual one (Go 8080, Rust/Python 8000, Node/Ruby 3000, else 8100),
  skipping ports other apps, services or programs use ("Best free port").
  The program gets the project's `.env` as environment variables (like
  compose's `env_file`; the file is only read), with Zenv's port in `PORT`
  and in whatever port setting the `.env` uses (`APP_PORT`, `BIND=host:port`).
  The address is always `127.0.0.1` (`HOST`, and the host part of
  `BIND`-like settings); the panel warns when the program listens on every
  network anyway. `.env` loaders keep variables already set, so Zenv's values
  win.
- Started after the app's web server starts and by "Start apps" (unless its
  port already answers); stopped by Stop all, on removal or switch-off, and on
  exit. Verified by `tests/shared_port_e2e.rs` with a Node app behind Apache.

## Tray and background

- Closing the window hides it; Zenv and its services keep running with an
  icon in the notification area. Its menu lists running services (click to
  stop), a submenu to start stopped ones, Start apps, Stop all, Open, and
  Quit (stops everything). A second launch shows the existing window
  (single-instance plugin) instead of a second Zenv managing the same
  services.

## Zenv home and installers

- Everything Zenv installs or manages lives in one folder, `C:\zenv` by
  default (`ZENV_HOME` or a `home.txt` pointer in the per-user app folder can
  move it): `bin\<component>\<version>` for installed versions, `data` for
  database data, `etc` for generated configuration, `logs`, `dev` (web servers' default pages), `backups`,
  `downloads`, and `zenv.db`. Projects can be anywhere. On first start, earlier
  metadata from the per-user app folder is copied in with `VACUUM INTO`; the
  original is kept.
- `runtime_installer`: one provider per language. Go (go.dev JSON feed),
  PHP (windows.php.net `releases.json`, NTS x64, php.ini generated from
  php.ini-development with the common extensions that exist), Python
  (python.org `index-windows.json`, the install manager's full packages),
  Ruby (RubyInstaller2 GitHub releases, `.7z` with SHA-256 digests), Rust
  (`rustup-init.exe` with `--no-modify-path` into the version's own
  `rustup`/`cargo` homes; `runtime_env` sets RUSTUP_HOME, CARGO_HOME and
  RUSTUP_TOOLCHAIN for it, so the developer's `~/.rustup` is never used).
  Versions come from the source's own metadata with SHA-256 checksums. The
  installer downloads over HTTPS (Windows TLS and certificate store, with
  allow-listed hosts, re-checked after redirects), verifies the checksum,
  extracts into a staging folder with zip-slip protection, renames atomically,
  then runs the executable's version command before registering it as
  *managed*. A failed install leaves nothing behind. Uninstall deletes only
  managed folders under `bin`; external installations can only be removed from
  the list.
- Node.js and Python also need their vendor's code signature (OpenJS
  Foundation on `node.exe`; Python Software Foundation on `python.exe`,
  `pythonw.exe` and `python3*.dll`): the checksum comes from the same server
  as the download, the signing key does not.
- The home is limited to the user, SYSTEM and Administrators (`icacls` with
  SIDs; the installer does it for a new home before copying files, Zenv on
  every start for an existing one). A folder made on `C:\` lets every
  signed-in account change it, and Zenv starts `zenv.exe` from there with
  administrator rights (hosts file) and runs the programs in `bin`. Before
  asking for those rights, Zenv checks that no other account can change
  `zenv.exe` or its folder (`platform::folder_access`), and the elevated
  helper writes nothing but the hosts file (its result is its exit code).
- Existing installations are still discovered (PATH, common locations, and
  the home's `bin`) and used in place.
- `runtime_env`: Zenv never edits the system PATH. Terminals ("Open terminal")
  and project processes get the chosen versions first on their own PATH, plus
  GOROOT/PHPRC, so versions coexist.
- Verified by `tests/coexist_e2e.rs`: Go 1.27.2 and 1.26.9 installed from
  go.dev next to the system Go 1.27.0, each reporting its own GOROOT; MySQL 9.7
  and 9.3 running at once; Redis 8.8 and 5.0 running at once; uninstall
  removes only the managed folder.

## HTTPS

- No mkcert download: `tls` creates the CA itself (ECDSA P-256, 10 years,
  `pathLen 0`, extended key usage server authentication only, name
  constraints for `.test`, `.localhost`, `.example`, 127.0.0.0/8 and ::1; CAs
  made by earlier versions also allowed `.internal`, `.lan` and `.home.arpa`
  and had no EKU, so recreate those). Creating it never trusts it;
  trusting is a second explicit action, and Windows shows its own
  confirmation for the current user's Root store. No administrator rights.
- Web servers have an HTTPS port (default 443). The TLS listener and the
  default `localhost` certificate exist only while an enabled site served by
  that server has HTTPS on, so port 443 stays free otherwise. Adding or
  removing the listener restarts the server (after checking the port is
  free); other site changes keep using graceful reloads.
- Site certificates (397 days) are reissued on apply when missing, within 30
  days of expiry, for different names, or not signed by the current CA.
- Verified by `tests/https_e2e.rs` on Apache 2.4.67 and Nginx 1.22.0.

## Database installers

- MySQL: releases from the `mysql-X.Y.Z` tags of github.com/mysql/mysql-server
  (newest patch of the LTS series 8.4 and 9.7 and of the newest series), zips
  from cdn.mysql.com (current folder, else its archives). Oracle publishes
  only MD5 sums behind a page that refuses automated requests, so Zenv checks
  Oracle's Authenticode signature on every `.exe` and `.dll` after extracting
  (WinVerifyTrust, signer "Oracle America, Inc."); anything else is removed.
- PostgreSQL: supported versions from postgresql.org's `versions.json`, zips
  from get.enterprisedb.com (the Windows builds postgresql.org links to).
  EDB publishes neither checksums nor signatures, so Zenv takes the zip's URL
  and SHA-256 from Scoop's manifest (`ScoopInstaller/Versions`
  `postgresqlNN.json`, else `Main` `postgresql.json`): a witness outside EDB's
  server, which also picks EDB's newest rebuild (`-5`). A build Scoop has not
  recorded yet is protected by HTTPS only (newest build number wins); the
  install dialog says which applies. pgAdmin,
  StackBuilder and the docs in the zip are not extracted.
- Verified by `tests/databases_install_e2e.rs`: MySQL 26.10.0 and PostgreSQL
  18.6 installed, data folders initialized, servers started and probed.

## PATH

- One version per language or database can be put on the user PATH (no
  administrator rights; the computer-wide PATH is only read). Choosing another
  version replaces the folders Zenv added (`path_choices`, migration v10).
- Before adding, Zenv looks through the system and user PATH for other
  folders holding the same programs (`php.exe`, `python.exe`, `psql.exe`, ...;
  Microsoft Store aliases excepted). Windows runs the first match and the
  system PATH comes first, so Zenv refuses and names the folders to remove.
- Running programs are told the environment changed (`WM_SETTINGCHANGE`);
  terminals opened afterwards use the new PATH.

## AI routers: OmniRoute and 9router (Tools)

Both are `Tool` components described by one `RouterSpec` each
(`core/src/ai_router.rs`); install, run, update, dashboard window and Codex
share the same code.

- Installed per version under `bin\<component>\<version>` from the npm
  registry (`runtime_installer/npm_tool.rs`): the package tarball is checked
  against the registry's sha512 integrity, then `npm install` with a
  supported Node.js (Zenv's own first, then PATH) installs it and its
  dependencies (npm checks each one's integrity).
  - OmniRoute: Node.js 22.22+/24+, ~1,200 dependencies (about 2.5 GB); npm
    11's script allow-list goes in the folder's `.npmrc`.
  - 9router: Node.js 22+, a few dependencies, `--ignore-scripts` (its only
    script fills `%APPDATA%\9router`, which Zenv does not use).
- The service runs with that Node.js, bound to the service address
  (127.0.0.1 by default; both routers bind every network otherwise), data in
  the service's data folder:
  - OmniRoute: `bin/omniroute.mjs serve --no-open --no-tray` with `PORT`,
    `OMNIROUTE_SERVER_HOST`, `DATA_DIR` = `data\omniroute`. Ready within
    180 s; health from `/api/monitoring/health`.
  - 9router: its server `app/custom-server.js` directly (the `9router`
    command is an interactive launcher with menus, tray and self-update)
    with `PORT`, `HOSTNAME`, `DATA_DIR` = `data\9router` and `NODE_PATH`
    (`<data>
untime
ode_modules`, then the bundled `app
ode_modules`).
    Health from `/api/health`.
- Install (`runtime_installer/npm_tool.rs`): the tarball must match the
  registry's sha512; `npm install --ignore-scripts --before=<publish time of
  that release>` resolves dependencies as they were when the release came out
  (a later, possibly hijacked, version never gets in); then `npm rebuild
  <names>` runs the install scripts of the few packages the router needs
  (npm 10 and 11 alike).
- Auto-update (default on per router, `app_settings` `<component>.auto_update`,
  migration v11): 90 s after Zenv starts and every 6 hours, the newest release
  that is at least 7 days old is installed next to the current one, the
  router's services move to it (restarted if they ran; moved back if it does
  not start) and versions no service uses are removed. A younger release is
  shown with the date it becomes eligible; "Install now" takes it earlier.
- The dashboard and its pages open in a Zenv window (one per router, labelled
  by component; the remote page has no access to Zenv); the panel copies the
  `/v1` base URL.
- Codex (`core/src/codex.rs`): provider and profile `omniroute`, or
  `ninerouter` (key from `NINEROUTER_API_KEY`, the profile's model left to
  the user's combo).

## Docker apps

- "New app → Docker container" lists the containers (`docker ps -a`, running
  first; containers that never ran get their ports from
  `HostConfig.PortBindings`). Only TCP ports published on this computer can
  be used: Windows cannot reach a container's own network.
- The app is an ordinary reverse-proxy app to `127.0.0.1:<published port>`
  (vhosts, HTTPS, preview, thumbnails and the tailnet as for any app). Its
  folder is the compose project's folder, else Zenv's `dev` folder.
- `docker_apps` (migration v12) ties it to the container and, for compose
  containers (labels `com.docker.compose.*`), to the project, service, folder
  and files. `core/src/docker_apps.rs`.
- Its "program" is the container: started with the web server, on "Start
  apps" or from the panel with `docker compose -p <project> -f <files>
  --project-directory <dir> up -d --no-recreate --no-build <service>`
  (dependencies start too; falls back to `docker start` when the compose
  files moved), else `docker start <container>`. Docker Desktop is started
  when the engine does not answer (up to 2 minutes). Stop = `docker stop`.
  "Stop all" stops only containers Zenv started in this session; quitting
  Zenv, switching the app off or deleting it leaves the container alone.
- Status: `docker inspect` state + whether the port answers. Output: the
  last 1,000 lines of `docker logs`, copied into the app's output log when
  it is read.
- `docker` runs without a shell; container, project and service names are
  checked against Docker's own name rules.

## Opening apps on the tailnet (Tailscale)

- An app can be opened to the devices signed in to the user's Tailscale
  account: `http://<this computer's MagicDNS name or tailnet IP>:<port>`. Nothing outside
  the tailnet can reach it, and Tailscale encrypts the traffic, so plain HTTP
  is used (no certificates, no firewall rules, no administrator rights).
- Zenv picks a free port (from 9001), stores it (`tailnet_apps`, migration
  v9), adds a loopback listener `127.0.0.1:<port>` for the app to the web
  server that answers for it (never a server running behind another), and
  runs `tailscale serve --bg --tcp=<port>
  tcp://127.0.0.1:<port>` (TCP mode answers the tailnet IP as well as the
  names; HTTP mode matches names only). Closing (or removing the app) runs
  `tailscale serve --tcp=<port> off`.
- The host name the visitor used (`my-laptop:9001`) reaches the app
  unchanged (Nginx forwards `$http_host`), so frameworks build working links.
  Only this computer's tailnet names and addresses are accepted there (kept
  in `app_settings` `tailnet.hosts`); any other host name gets 421/403, so a
  web page elsewhere cannot point its name at the port (DNS rebinding).
- Visitors arrive from 127.0.0.1 through `tailscale serve`, which apps would
  take for this computer. They are given `100.64.0.0` (no device has it)
  instead: PHP's `REMOTE_ADDR`, and `X-Forwarded-For` / `X-Real-IP` for proxy
  apps. Apache's own `Require local` still sees 127.0.0.1.
  Apps on a FrankenPHP backend are routed by an `X-Zenv-Site` header the front
  server sets, since their host name is not the app's domain.
- Verified by `tests/tailnet_e2e.rs` (Apache 2.4.67, Nginx 1.22.0, FrankenPHP
  1.12.2 with php-cgi, FrankenPHP-backend and proxy apps) and by a real
  `tailscale serve` round trip.

## Databases and caches

- MySQL, PostgreSQL and Redis are services like the web servers (same switch,
  logs, events, port checks, Job Objects), through the shared `ServiceAdapter`
  trait. Database adapters live in `database_manager`, Redis in `cache_manager`.
- Each has a data folder (`services.data_dir`, migration v5), by default
  `<data>/data/<component>-<id>`, or any folder the user picks. Before every
  start the adapter classifies it: missing, empty, initialized, foreign
  (something else) or incompatible (another major version). Only missing or
  empty folders can be initialized, and only through an explicit, confirmed
  action. Foreign and incompatible folders are refused and never touched, and
  deleting a service never deletes its data.
- Connection details are shown, and checks run at the protocol level (MySQL
  greeting, PostgreSQL SSLRequest, Redis PING), so no client libraries are
  needed.
- Verified by `tests/db_e2e.rs`: start refused before init; init; re-init
  refused; data (database, table, key) survives a graceful stop and restart for
  MySQL 9.7, PostgreSQL 9.2, Redis 8.8 (MSYS2) and Redis 5.0; a foreign folder
  is refused and left intact; deleting the service keeps the data folder.

## PHP per site

- A PHP site picks a PHP installation (its version) and an engine: `auto`,
  `fastcgi` or `frankenphp`.
- **PHP-CGI** (Windows has no PHP-FPM): Zenv runs 4 `php-cgi.exe -b 127.0.0.1:<port>`
  workers per PHP version in use, restarts any that exit (php-cgi quits after
  `PHP_FCGI_MAX_REQUESTS`), and stops a pool when no enabled site needs it.
  Apache reaches them through `mod_proxy_fcgi` + a per-site balancer, with
  `ProxyFCGIBackendType GENERIC` and an explicit `SCRIPT_FILENAME` (verified:
  without it php-cgi answers "No input file specified").
- **FrankenPHP** embeds exactly one PHP version (1.12.2 → PHP 8.5.6). With
  `auto`, a site whose PHP minor version matches an enabled FrankenPHP
  installation is served by it: FrankenPHP listens on an internal loopback port
  (Caddyfile with one block per site, validated, then hot-reloaded through its
  admin API), and the front web server proxies the domain to it. One HTTP port
  serves every site.
- Switching FrankenPHP off re-routes its sites to PHP-CGI immediately. A PHP
  version used by enabled sites cannot be switched off.
- Verified by `tests/php_e2e.rs`: PHP 8.4 and 8.1 sites side by side, Laravel
  rewrites, `.env` refused, PHP 8.5 routed to FrankenPHP automatically, the
  on/off re-route, supervisor restart of a killed worker, and pools stopping
  when their sites are removed.

## Compatibility constraints (verified 2026-10-16)

See `docs/COMPATIBILITY.md`. Summary:

- Apache: no ASF Windows binaries; Apache Lounge builds. Console mode does not
  support `httpd -k shutdown` (it targets the "Apache2.4" service); graceful
  control uses named events (verified against Apache Lounge 2.4.67 VS18).
- FrankenPHP: native Windows since v1.12.0, including worker mode; it bundles its
  own PHP and embeds Caddy (local copy: 1.12.2 / PHP 8.5.6 / Caddy 2.11.2).
- Redis: no official Windows build (Memurai is the commercial partner). Zenv
  will not bundle Redis; users may register an existing build.
- PHP on Windows has no FPM; FastCGI integrations use `php-cgi.exe`.

## First end-to-end acceptance test

`src-tauri/core/tests/apache_e2e.rs` (ignored by default; run with
`ZENV_TEST_APACHE_ROOT` set):

1. Initialize `Core` in a temp data dir (fresh SQLite and migrations).
2. Register a real Apache installation and detect its version.
3. Create a service on a free port. Generate, stage, and validate the config with `httpd -t`.
4. Start the service. Assert that it is running, the PID is reported, the port is owned by a job PID, and `GET /` returns 200 with Zenv's default page.
5. Stop it gracefully. Assert that the process exited, the port is free, and logs are readable.
6. Inject an invalid include. Assert that validation fails with checker output, start is refused, and the active config is unchanged.

## Milestone 2 acceptance test

`src-tauri/core/tests/sites_e2e.rs` (same environment variable) checks, against
a running Apache: static site served by its domain; `.php` and `.env` refused;
unknown hosts get the default page; reverse proxy to a live upstream with the
Host header preserved; disable/enable; duplicate domains and self-proxies
rejected; a broken foreign include makes activation fail without changing what
is served; 300 sites; deleting a site keeps its project files.

## Persistence

SQLite at `<data>/zenv.db` (WAL). The schema version lives in
`PRAGMA user_version`, and the history in `schema_migrations`. Each migration
runs in its own transaction. Before upgrading an existing database, Zenv
writes a backup to `<data>/backups/zenv-v<N>-<timestamp>.db`. A database
with a newer schema than the build supports is refused, not altered.
