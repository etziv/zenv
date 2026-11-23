# 17 security review, round 2: native Studio chat (M20 step 8c)

## Plan
Re-check only the four round-1 findings against the fixes listed in `17-dev.md` ("security round 1"), plus the two qa changes the
coordinator named (Studio window `placement`, `follow_mode`) for new surface. No new audit.
Files: `src-native/src/studio/chat/{mod.rs,composer.rs,markdown.rs,tools.rs,model.rs}` (untracked, read in full where changed),
`git diff -- src-native/src/studio/mod.rs`. Proof: read every call site of the catalog, rerun the round-1 stack test on the new parser.

## Round-1 findings, re-checked
1. [high -> partly fixed; residual medium, open] Claude Code started in the project folder without the user asking (`src-native/src/studio/chat/mod.rs:L116`, `L285-L316`, `L373-L375`; `composer.rs:L480-L482`, `L512-L513`)
   - Fixed case: `catalog_stale` is only set at creation; `studio_chat_catalog` runs only from `want_catalog`, whose only callers are
     `Action::Send` (mod.rs L374) and `composer.wants_catalog`, set only by a click on the "/" palette chip (L482) or the model chip (L513).
     Nothing on Studio open, tab creation or a timer starts it. Test `opening_studio_starts_no_claude_code_until_the_user_acts` (5 polls, no job;
     job after `wants_catalog`). The case reported in round 1 (open Studio, no click, repo code runs) is closed.
   - Residual (the variant listed in round 1, not changed): the first click on the model chip or palette, or the first message, still runs
     `claude -p` in the project root with no trust check (`studio_chat_catalog`, `studio_open_chat`, core L521-L621). A user who opens a cloned
     repo's Studio just to pick a model runs that repo's `.mcp.json` servers and hooks (per Claude Code docs, `-p` connects them "without
     asking"); the terminal tab would show Claude Code's trust dialog first. Rated medium now: it needs a click in the chat on a hostile repo.
   - Fix (unchanged): run the catalog with `--setting-sources user --strict-mcp-config` (it needs no project settings), and before
     `studio_open_chat` check `~/.claude.json` `projects[<root>].hasTrustDialogAccepted`; if not true, show a one-time "Trust this folder?" card.
     Or record a ruling accepting it as D-012 behaviour (React does the same until step 9).
2. [medium -> fixed] Deep quote nesting crashed the app (`markdown.rs:L55-L56`, `L117-L125`, `L177-L180`)
   - `blocks` stops nesting at `MAX_QUOTE_DEPTH` = 8 (deeper `>` lines fall to the paragraph path, which still advances `i`); `spans` stops at
     `MAX_INLINE_DEPTH` = 8; `show` recursion follows the block depth. Rerun of the round-1 scratch test on the new parser (release, 1 MB main
     stack): 3,000 `>` -> ok, 30.7 ms; 100,000 `>` plus 100,000 nested `**__` markers -> ok, 144.5 ms (round 1: overflow from 3,000).
     Test `hostile::deep_quotes_and_markers_do_not_overflow_the_stack` covers parse and draw. ExitPlanMode uses the same parser.
3. [medium -> fixed] Permission card hid part of what is approved (`tools.rs:L52-L66`, `L88-L99`)
   - Edit now ends each side with "… N more lines not shown"; Write and the JSON view end with "… N more characters not shown"
     (`chars().count()` on the rest, char-boundary safe). The user now knows the card is incomplete before clicking Allow, which closes the
     silent case. The rest still cannot be expanded and `replace_all` / the full path are still not in the body: under Hardening.
4. [low -> fixed] "Always allow" named one rule but applied all (`model.rs` `suggestion`)
   - The label adds "and N more", counting every rule of every suggestion (a suggestion without `rules` counts 1); test expects
     "Always allow Bash ls:* and 2 more". The button now says it does more than the first rule.

## New since round 1 (surface check)
- `placement` (`studio/mod.rs`): window size and position from monitor size and the main window rect; geometry only, no input from files,
  model or network. No surface.
- `follow_mode`: now adopts a reported mode only when Claude Code's reported `permissionMode` changes; it narrows the earlier behaviour and
  does not persist options. No surface.

## Findings (open after round 2)
1. [medium] [confirmed] A click on the model menu or palette, or the first message, runs `claude -p` in an untrusted project folder without a trust check (`src-native/src/studio/chat/mod.rs:L312-L316`, `L373-L375`; core `src-tauri/core/src/studio_chat.rs:L521-L621`)
   - What this is: the residual of round-1 finding 1 above.
   - Problem: a cloned repo with a `.mcp.json` server command or a SessionStart hook; the user opens its Studio and clicks the model chip;
     Claude Code (documented `-p` behaviour) runs that command as the user with no prompt.
   - Fix: catalog with `--setting-sources user --strict-mcp-config`; trust check (or one-time confirm card) before `studio_open_chat`. Test: the
     catalog command line carries the flags; an untrusted root does not open a chat without the confirm.
   - If we skip it: the chat is a way to run a cloned repo's code that Claude Code's own terminal would ask about first.

Rejected:
- Round-1 rejections stand (code unchanged there): path escape via Open in editor / CLAUDE.md, links never opened, answers to the wrong program, late lines, orphaned processes, MCP config secrets, `GetKeyState`, clipboard injection, regex backtracking, char-boundary panics, attachment traversal, palette terminal screens, dependencies.

Hardening (not counted):
- Permission card: let "N more lines/characters" expand to the full input, show `replace_all` and the full `file_path` in the body.
- "and N more" could list the extra rules in a hover (a later `setMode` suggestion is applied but not named).
- Round-1 items still open: ExitPlanMode plan re-parsed every frame; `model::relative` lowercase-length slice (non-ASCII root); unmatched control responses kept; 20 MB reads on the UI thread; pre-existing lock advisories.

Findings: high 0, medium 1, low 0
Safe to proceed: yes
Not checked: the live run with a marker `.mcp.json` (read-only review; the residual rests on Claude Code's documented `-p` behaviour); the dev's tests were not rerun here (I read them; `task.mjs check` passing is the coordinator's claim); "Safe to proceed: yes" assumes the residual medium gets a fix or a recorded ruling accepting it.
