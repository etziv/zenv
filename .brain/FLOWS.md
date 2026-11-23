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
Trigger: the boss saves the New app dialog (`SiteFormDialog`, native `pages/apps/form.rs`); both shells call `Core.create_site`
(Docker: `Core.create_docker_site`), which ends with `Core.sync_app_process`.
1. `SiteManager.create` holds the sites lock (`SiteManager.guard`) and checks the input (`SiteManager.validate`: folders, domain via `validate_domain`).
2. `SiteManager.insert_row` stores it as pending (disabled apps stop here).
3. `SiteManager.apply_or_undo` runs `apply_service` for every server that may serve it: render all enabled apps into a staged folder,
   check them with the server (`ServiceManager.check_with_sites_dir`), sync through the ledger, reload (`ServiceManager.reload_checked`).
4. A failed apply deletes the new row and re-applies; a server that does not survive the reload gets the previous files back (`restore`).
Rules: no app row without a working configuration ([D-001](DECISIONS.md#d-001-zenv-never-edits-a-server-installation-it-validates-before-activating)).
History:
- 2026-11-09 adopted
- 2026-11-19 task 13: create/update/enable/delete moved from the Tauri commands into `Core` for both shells

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
Trigger: the boss sends a message in the Claude tab (React `Composer` -> `ClaudeChat`; native `chat::composer::Composer` -> `Chat.send`, first tab of `Terminals` when Claude Code is installed).
1. First message (or after settings changed between turns: React `sameOptions`, native `conn::needs_restart`): `Core.studio_open_chat` starts `claude -p`
   in stream-json (`Core.claude_command`) with model, effort, mode and `--settings` from the composer (React via `chat_open`; native on the chat's worker thread, `conn::spawn`, ops in order).
2. `Core.studio_chat_send`: `prompt_content` turns pasted images/text/files into blocks (other files saved under the attachments folder). Native attachments: picker (rfd), drop, `PasteImage`, Ctrl+V of files copied in Explorer (`GetKeyState` + `arboard` file list, egui-winit sends no event for them).
3. Each output line is trimmed by `shape` (text deltas, messages, permission requests, MCP status) and applied per frame (React `chatReducer`; native `ChatState.apply`, lines tagged with a generation so a New chat ignores the old program's late lines).
4. A permission card answers through `Chats.answer` (`permission_answer`, "always" adds Claude Code's suggestion).
5. MCP status/toggle/reconnect go through `Chats.control`; the answer comes back as a `control_response` matched by request id (native: `Chat.poll`, 20 s timeout).
Rules: the program belongs to its Studio window (`studio_owner` / native owner `native-studio:<site>`); closing the window kills its process tree (`kill_tree`, native `Terminals.close_all`) ([D-012](DECISIONS.md#d-012-studio-chat-runs-the-official-claude-code-over-pipes)).
Native Markdown is a port of React's own parser (`chat::markdown::parse`), not egui_commonmark.
History:
- 2026-11-09 adopted
- 2026-11-23 task 17: native chat tab (M20 step 8c)

## F-007 Studio editor: open, edit and save a project file
Trigger: Open in Studio in the app panel or its actions menu (React `EditButtons`; native `pages/apps/panel.rs`, `grid::actions_menu` -> `Action::OpenStudio`).
1. Window: React `studio_window::show` (one Tauri window per app); native `studio::Windows.open` (one deferred egui viewport per app, G-016; at most 1440x900 and 90% x 80% of the screen, centred on the main window, `placement`).
2. Files: `project_files::list` per opened folder (`FileTree` / `Tree.toggle`), refreshed every 5 s and on window focus; new file/folder via `project_files::create`.
3. Open: `project_files::read` (UTF-8, <= 2 MB, `.env` read-only); native keeps CRLF files as LF in the buffer (`Doc.load`) and writes CRLF back.
4. Save (Ctrl+S): `project_files::write` with the mtime it opened; a changed file fails with Conflict -> banner Load theirs (`Doc.reload`) / Keep mine (write without the check).
5. Outside edits: every 1.5 s `project_files::stat`; a clean file reloads, a dirty one shows the conflict banner (`Doc.poll_disk`).
6. Format: gofmt/rustfmt/ruff through `Core.format_code` (Shift+Alt+F, format on save); a missing tool offers `Core.install_formatter`. React also runs Prettier in the WebView; native does not.
7. Go to file (Ctrl+P): `project_files::search` 120 ms after typing (`QuickOpen`).
8. Leaving: closing a Studio window, Quit from the tray, or closing the main window without a tray asks when files are unsaved (`Studio.guard_close`, `Windows.ask_quit`).
Rules: paths never leave the project root (`project_files::parts`); native colours (syntect, Dark+) only for files <= 100 KB with lines <= 2 KB, re-coloured per changed line (`Highlight.update`); files > 1 MB or with a line > 40 KB open read-only.
History:
- 2026-11-23 task 15: native Studio window, tree, editor, Go to file (M20 step 8a)
- 2026-11-23 task 17: Studio window sized to the screen and placed over the main window (qa: it opened below a 1080 px screen)

## F-008 Studio terminal: run Claude Code, Codex or PowerShell in the app's folder
Trigger: the terminal panel of a Studio window (`+` menu, the chat palette's terminal rows; PowerShell is the first tab only when Claude Code is not installed, else the chat F-004 is; Ctrl+` / the PanelRight button shows or hides the panel).
1. Agents: `Core.studio_agents` (React `TerminalPanel`, native `Terminals.refresh_agents`) resolves each program on the app's PATH (`studio::resolve`).
2. Open: `Core.studio_open_terminal` checks the agent and screen, builds the env (`studio_env`) and starts it in a ConPTY (`Terminals::open`, max 16) in the project root; Claude Code gets `--append-system-prompt` with `studio_context`.
3. Output: the reader thread hands bytes to the window's sink: React writes them into xterm.js; native feeds `Screen` (alacritty_terminal grid + vte parser) and repaints.
4. Input: typed keys (`terminal::keys::encode`), paste (bracketed when asked), mouse reports go to `Terminals::write`; native answers the program's queries (ConPTY's cursor query, colours, size) from the reader thread through `Outbox`, queued until the session id is known.
5. Resize: the widget fits columns and rows to its rect (`grid_size`) and calls `Terminals::resize`.
6. Exit: the program's end shows "[name ended (exit code N). Press Enter to start it again.]"; Enter starts it again on a clean screen.
7. Close: a tab's close ends its program (`Terminals::close`); closing the window ends all of the window's (`close_owned_by`); quitting Zenv ends every one (`close_all`).
Rules: only known programs run, always in the project root; a terminal belongs to the window that opened it (owner check on every call).
The empty editor closes so the terminal takes the full width until a file opens (boss 2026-11-06: "kasih default kosong workspace editor dan bisa disilang, jika disilang maka claude akan full screen").
History:
- 2026-11-23 task 16: native terminal panel and widget (M20 step 8b)
- 2026-11-23 task 17: the chat tab comes first when Claude Code is installed

## F-005 Load a saved Claude Code session
Trigger: History in the chat header.
1. `chat_sessions` -> `sessions`: lists `<id>.jsonl` in Claude Code's folder for the project (`sessions_dir`), titles via `studio_history::title_of` (last 64 KB).
2. Picking one: `chat_history` -> `transcript` -> `transcript_lines` walks `parentUuid` from the last message (current branch) and returns prompts and trimmed messages.
3. The UI shows them (React `chatReducer` "history", native `ChatState::history` via `Chat.load_session`) and keeps the session id; the next message resumes it with `--resume`.
History:
- 2026-11-09 adopted
- 2026-11-23 task 17: native History menu (`Chat.header`)

## F-006 Open an app on the tailnet
Trigger: Open on tailnet in the app panel (`TailnetPanel`), optionally with an own name.
1. `Core.tailnet_open` reads Tailscale (`status`); a Service name must pass `valid_service` and `tailnet::service_problem` (tagged host, HTTPS on).
2. A loopback port is reserved in `tailnet_apps` (with the Service name) and the app's servers are re-applied (`Core.servers_of`); the
   port accepts only this computer's names and the app's Service name (`hosts_for`).
3. `tailscale serve --tcp=<port>` (port mode) or `tailscale serve --service=svc:<name> --https=443` to that port.
4. `Core.tailnet_close` undoes it (`serve clear svc:<name>` or `--tcp=<port> off`) and re-applies.
5. Renamed PC or re-rolled tailnet name: `Core.follow_tailnet_names` (thread `tailnet-names` in `lib.rs`, every 60 s, and on each
   `tailnet_apps` read) saves the new names and re-applies the web servers; nothing runs while no app is open or Tailscale is offline.
History:
- 2026-11-09 adopted
- 2026-11-19 task 13: native panel `pages/apps/tailnet.rs` repeats the tagged-host/HTTPS check before it offers the Service name
- 2026-11-09 task 3: names followed in the background, not only when the tailnet panel is open
