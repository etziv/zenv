# FLOWS (how the system works NOW; one entry per flow, edited in place, max 25 lines each)

## F-001 Start a web server
Trigger: the boss switches a web server on (dashboard tile, tray or Start apps).
1. `Core.start_service` checks whether the service serves apps (`adapter_for` -> `supports_sites`; databases and AI routers say no,
   e.g. `RouterAdapter.supports_sites`) and asks `ServiceManager.running_rivals` for running servers on the same port.
2. A web server with a rival runs behind it: `Core.start_behind` takes a free port (`suggest_free_port`), `ServiceManager.host_behind`,
   regenerates its apps (`SiteManager.reapply`), starts it and re-applies the front so it forwards those apps ([D-004](DECISIONS.md#d-004-web-servers-on-the-same-port-run-together)).
3. Otherwise every rival on the port is stopped, whatever its kind (starting MySQL on a port Apache holds stops Apache); a web
   server's apps are regenerated (`SiteManager.reapply`), then `ServiceManager.start` and `SiteManager.mark_applied`.
4. `Core.start_apps_of` starts the programs and Docker containers behind its proxy apps (`Core.autostart_apps`, `Core.start_app_process`).
Rules: on a failed start behind a front, the service is stopped, unhosted and the front re-applied.
History:
- 2026-11-09 adopted

## F-002 Create an app (local site)
Trigger: the boss saves the New app dialog (`SiteFormDialog`).
1. `SiteManager.create` holds the sites lock (`SiteManager.guard`) and checks the input (`SiteManager.validate`: folders, domain via `validate_domain`).
2. `SiteManager.insert_row` stores it as pending (disabled apps stop here).
3. `SiteManager.apply_or_undo` runs `apply_service` for every server that may serve it: render all enabled apps into a staged folder,
   check them with the server (`ServiceManager.check_with_sites_dir`), sync through the ledger, reload (`ServiceManager.reload_checked`).
4. A failed apply deletes the new row and re-applies; a server that does not survive the reload gets the previous files back (`restore`).
Rules: no app row without a working configuration ([D-001](DECISIONS.md#d-001-zenv-never-edits-a-server-installation-it-validates-before-activating)).
History:
- 2026-11-09 adopted

## F-003 Install a version from its official source
Trigger: Install in a component page (`InstallDialog`) calls `installer_install`.
1. `Installer.start` refuses an existing folder or a running job, records the job and works on a thread.
2. `run_install` finds the offer, downloads to `downloads` (allowed hosts only), compares the SHA-256 and discards a mismatch.
3. `unpack_into` extracts a zip or 7z into a staging folder and renames it into `bin/<component>/<version>`; a custom provider
   (`custom_install`, e.g. an installer it runs) writes straight into that folder.
4. The provider's post-install step runs, the version is probed (`detect_version`) and registered as managed; FrankenPHP also
   registers its embedded PHP. Any failure removes the new folder; the UI polls `installer_jobs` for the phase.
History:
- 2026-11-09 adopted

## F-004 Studio chat: a prompt to Claude Code
Trigger: the boss sends a message in the Claude tab (`Composer` -> `ClaudeChat`).
1. First message (or after settings changed between turns, `sameOptions`): `chat_open` -> `Core.studio_open_chat` starts `claude -p`
   in stream-json (`Core.claude_command`) with model, effort, mode and `--settings` from the composer.
2. `chat_send` -> `Core.studio_chat_send`: `prompt_content` turns pasted images/text/files into blocks (other files saved under the attachments folder).
3. Each output line is trimmed by `shape` (text deltas, messages, permission requests, MCP status) and batched per frame in the UI (`chatReducer`).
4. A permission card answers through `chat_answer` -> `Chats.answer` (`permission_answer`, "always" adds Claude Code's suggestion).
5. MCP status/toggle/reconnect go through `chat_control` -> `Chats.control`.
Rules: the program belongs to its Studio window (`studio_owner`); closing the window kills its process tree (`kill_tree`) ([D-012](DECISIONS.md#d-012-studio-chat-runs-the-official-claude-code-over-pipes)).
History:
- 2026-11-09 adopted

## F-005 Load a saved Claude Code session
Trigger: History in the chat header.
1. `chat_sessions` -> `sessions`: lists `<id>.jsonl` in Claude Code's folder for the project (`sessions_dir`), titles via `title_of` (last 64 KB).
2. Picking one: `chat_history` -> `transcript` -> `transcript_lines` walks `parentUuid` from the last message (current branch) and returns prompts and trimmed messages.
3. The UI shows them (`chatReducer` "history") and keeps the session id; the next message resumes it with `--resume`.
History:
- 2026-11-09 adopted

## F-006 Open an app on the tailnet
Trigger: Open on tailnet in the app panel (`TailnetPanel`), optionally with an own name.
1. `Core.tailnet_open` reads Tailscale (`status`); a Service name must pass `valid_service` and `service_problem` (tagged host, HTTPS on).
2. A loopback port is reserved in `tailnet_apps` (with the Service name) and the app's servers are re-applied (`Core.servers_of`); the
   port accepts only this computer's names and the app's Service name (`hosts_for`).
3. `tailscale serve --tcp=<port>` (port mode) or `tailscale serve --service=svc:<name> --https=443` to that port.
4. `Core.tailnet_close` undoes it (`serve clear svc:<name>` or `--tcp=<port> off`) and re-applies.
5. Renamed PC or re-rolled tailnet name: `Core.follow_tailnet_names` (thread `tailnet-names` in `lib.rs`, every 60 s, and on each
   `tailnet_apps` read) saves the new names and re-applies the web servers; nothing runs while no app is open or Tailscale is offline.
History:
- 2026-11-09 adopted
- 2026-11-09 task 3: names followed in the background, not only when the tailnet panel is open
