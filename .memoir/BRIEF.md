# BRIEF (requirements, adopted from README and code 2026-11-09, approved by the boss, edit in place, max 80 lines)

Status: approved 2026-11-09 (adopted from README and code)

## Goal
Zenv: one Windows desktop app to run a local development environment (Laragon/Herd-like): runtimes, web servers, databases,
caches and local apps, plus an in-app IDE ("Studio") with Claude Code. Source: README, the boss's sessions.

## Users and problem
The boss (and later other Windows developers) running many PHP, Go, Rust, Python, Node and Docker projects side by side, who
replaced FlyEnv and Laragon with Zenv (2026-10-19) and want each service and app switched on/off from one place.

## Platform (web / android / other) and why
Other: a native Windows desktop app (Tauri 2 + Rust), because it must own processes, ports, the hosts file and certificates.

## Modules
MVP: see MODULES.md (M1-M18): services, web servers, apps, PHP per app, databases, installers, HTTPS, proxy app programs,
Docker apps, AI routers, tailnet, Studio (editor, terminals, Claude Code chat), editors, tray.
Later:
- macOS port (M19), only on request.

## Not in scope
- Project profiles / Stacks ([D-002](DECISIONS.md#d-002-no-project-profiles-or-stacks)); a public tunnel ([D-005](DECISIONS.md#d-005-tailscale-instead-of-a-public-tunnel)); an import feature from other tools.

## Constraints (budget, hosting, team skills, existing tools, deadline)
- Windows 10/11 x64 with WebView2; Rust windows-gnu toolchain with WinLibs GCC; no administrator rights needed except UAC for the hosts file and the CA trust.
- Everything Zenv manages lives in its home `C:\zenv`; projects can live anywhere.

## Expected scale
- One user per machine; thousands of apps supported in the list (sites tests use 5,000 rows).

## Integrations
- Official download sources (php.net, go.dev, nodejs.org, python.org, Apache Lounge, GitHub releases, cdn.mysql.com, EDB, npm).
- Docker Desktop, Tailscale, Claude Code CLI, Codex CLI, VS Code and other editors, headless Edge (thumbnails).

## Security / compliance needs
- See ARCHITECTURE.md "Security baseline"; hardening for a public release done 2026-10-31.

## Assumptions
- The boss uses Claude Max through the official Claude Code CLI; Zenv never stores Anthropic credentials.

## Open questions
- Public release: code signing (SignPath, Certum OSS, Azure Trusted Signing) and Zenv self-update are undecided.
- Is the macOS port still wanted, and when?
- Should Caddy become a front server (README "Not built yet")?
