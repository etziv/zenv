---
name: figcraft-build
description: "FigCraft fast build and command map: plan every step, then build in ONE run with scripts/figlive.mjs (fast, or live: the boss watches the agent's cursor build it), or generate a whole document as Figma-REST JSON with scripts/figrest.py and open it in one call; batch edits with $ref chaining, live app setup, real font list, gotchas. Use for FigCraft work bigger than a few nodes, when the boss wants to watch, a command lookup, or when FigCraft is slow; the look comes from `design`, drawing rules from `figcraft-design`."
license: MIT
metadata:
  version: "1.0.0"
  source: "learned and proven on FigCraft 0.15.0 (glyph task 9, 2026-11-03)"
---

# FigCraft build

Drawing node by node over MCP costs one model round-trip per step: a 1440x900 screen with a few hundred layers
takes many minutes and a 15-screen proposal takes hours. This skill builds the whole document in code and opens it
in one call (7,000+ layers in seconds), then uses a handful of batched commands for what JSON cannot carry.

**This skill is the how, not the look.** Before the first screen: the `design` skill's "Before ANY drawing" gate
(`.brain/design/proposal/research.md` passes `library.mjs check`; every screen's components come from the library entries it names), then the `design` skill (for app screens
`references/app-screens.md` whole: give the product a face, house rules, tells) and the `figcraft-design` quality
bar. A fast build of a generic layout is still generic: the boss rejected exactly that ("terlalu generic dan kaku").

## 0. Rules
- **Never headless: the boss SEES every design in his FigCraft window** (boss 2026-11-09, 2026-11-21: "gimana mau
  lihat"). Before the first drawing call confirm the window: the figcraft `inspect_ui` answers (not an error) and
  `screenshot {window:true}` shows the app. The wrapper refuses every tool call it cannot send to the window, and
  figlive opens his window itself and exits 1 `NOT IN THE WINDOW` when it cannot show the saved file. A design that
  only exists in a headless engine or a PNG is not delivered.
- **The figcraft MCP is down: never fall back to an HTML mock-up.** Build with `scripts/figlive.mjs` (its own engine,
  section 2b; it still shows the result in the boss's window); only work that needs the boss's open app waits: tell
  him in one line "FigCraft tidak tersambung: /mcp -> figcraft -> Reconnect" (the wrapper reopens the app; still
  down = FigCraft not installed).
- **Plan completely, then execute.** Write the plan (per screen: the question it answers, its signature element,
  composition, components, data) before generating anything; then build in as few import rounds as possible.
- **Real data only**, labelled if fictional. Fonts only from what FigCraft really has (section 5).
- Every claim of "done" rests on a screenshot of every frame you made.
- A product mark comes from the product's name (wordmark, a monogram drawn from its letterform as vector paths),
  never a generic box or document symbol (boss 2026-11-03).

## 1. Live mode (the boss watches the app window)
`.mcp.json` runs `node .claude/scripts/figcraft-mcp.mjs`. The window opens only when a figcraft tool is first USED:
until then a headless figcraft-cli answers the handshake and the tool list (no window). The first tool call starts
`figcraft.exe --control 7777` (`FIGCRAFT_PORT`) with the repo as read root and `.brain/design` as write root
(detached: the app outlives the session) when nothing listens there, attaches to it (about 3 s) and sends every later
call to that window; an app already open on the port is attached at session start. Nothing to start by hand.
The app cannot start -> every tool call returns an error with the cause and the fix (never a headless answer; the
tool list still answers): stop drawing, fix what the error names, or tell the boss the one line of section 0; once
the app listens on the port the next call attaches. The server instructions say "Backend: the FigCraft app window".
- Attached mode refuses roots on the CLI, so the app on the port keeps the roots it was opened with: an app opened
  by ANOTHER project writes only under that project's `.brain/design`. A save refused for its root -> ask the boss
  to close FigCraft, then `/mcp -> figcraft -> Reconnect`.
- Installed elsewhere: user env `FIGCRAFT_CLI=<dir>/figcraft-cli.exe` (`figcraft.exe` is taken from the same folder).
- Probe without the session: pipe `initialize`, `notifications/initialized` and a `tools/call` of `inspect_ui` into
  `node .claude/scripts/figcraft-mcp.mjs`; stderr prints "attached to the FigCraft app" (a session-start handshake
  alone stays on the headless list answerer and opens nothing).
Keep the boss's view on the work: after each import or frame, `page.select` + `view.zoomFit` (or
`view.zoomSelection`). A dark empty canvas = the view is on an empty area, not a crash.

## 2. Bulk build (the fast path)
1. Write a generator that imports `scripts/figrest.py` (helpers: `F` frame with auto layout, `T` text, `R`, `E`,
   `vector`, `comp` / `inst` components and instances with text overrides, `fillw` / `fixedw` sizing, `at`,
   `svg_layers` for editable SVG artwork, `page`, `write_document`, `vector_batch`). x/y are relative to the parent,
   as in FigCraft. Self-test: `python scripts/figrest.py out.json`.
2. `write_document('proposal.rest.json', name, pages)` (keep the JSON gitignored; the generator is the source).
3. Live: `run_command file.close {index, force}` for stale tabs, then `open_file proposal.rest.json` (new tab,
   returns page ids and `warnings`) in the boss's window. A generator self-check without the window (only a scratch
   file in `tmp/`, never what he reviews): `figcraft-cli convert in.json out.figcraft`, `figcraft-cli render --out
   page.png out.figcraft`, `figcraft-cli info out.figcraft` (lists missing fonts).
4. ONE `batch` right after import: vector paths (`vector_batch(page_ids)` gives the commands: `node.find` by name
   per page `as: ref`, then `node.set {ids: {"$ref": ref, "field": "ids"}, path, width, height}`). This first edit
   also makes FigCraft compute every auto layout (before it, children sit where the JSON put them).
5. Verify (section 4), fix in the generator, close the tab, reopen; save with `save_file <dir>/name.figcraft`.
Interactive tweaks after that: `set_properties` / `batch` on ids from `node.find` (never redraw by hand).

What survives the REST import (proven): FRAME/COMPONENT/INSTANCE/TEXT/RECTANGLE/ELLIPSE, fills and strokes (hex
with alpha), strokeWeight/strokeAlign, cornerRadius, clipsContent, auto layout (layoutMode, itemSpacing, padding*,
primary/counterAxisAlignItems, primary/counterAxisSizingMode AUTO=hug), layoutSizingHorizontal FILL/FIXED, text
style (fontFamily, fontWeight -> style name, fontSize, lineHeightPx, textAlignHorizontal, textAutoResize),
component <-> instance links (`components` map + `componentId`), page background. Not carried: VECTOR geometry
(imports empty; fill it as in step 4). Details and field names: `references/rest-json.md`.

## 2b. Research, then plan, then build live in one run (`scripts/figlive.mjs`, the default for product screens)
The order is fixed (boss 2026-11-03): 1) the research pack passes `library.mjs check`; 2) the plan file builds every
screen the pack lists, each in its default, empty, loading and error state, and its default state in light AND dark
(boss 2026-11-10); 3) `figlive.mjs` executes the whole plan
in ONE run (no model round-trip per step), live so the boss watches. figlive enforces 1 and 2: it refuses to build
otherwise and prints what is missing.
- `node scripts/figlive.mjs .brain/design/proposal/build-plan.json --check`: the gate only; fix until it prints `plan ok`.
- Edit, never a new file (boss 2026-11-11): when the `"save"` file exists, the plan `"open"`s that same path and its
  steps change only what changes (`node.find` by name + `node.set`, or `edit.delete` a frame and rebuild that screen);
  an edit is not held to the every-state rule (the rest is already in the file). `"replace": true` instead rebuilds it
  from scratch (the old design is overwritten). A plan with neither (or one opening another file) is refused.
- One tab per design in the boss's window: before the build figlive saves his unsaved edits in that file's tab (found
  by path, so a same-named file of another project is never touched) and closes leftover clean "Untitled" tabs; the
  save comes after `--hold`, so what he draws meanwhile is kept; after the build (or a failed one: nothing saved, the
  file unchanged) it closes the live tab and shows the file there, reloaded (also without `--live` when the window is
  open). A plan without `"save"` keeps the live tab: it is the only copy. A tab with unsaved work is never closed.
- `node scripts/figlive.mjs <plan> --live`: hosts a live session on this machine and joins it in the boss's viewer
  window by itself (no click; `--no-viewer` prints the link only): the window on port 7800 (`FIGCRAFT_VIEWER_PORT`) is
  reused by every run, opened only when none answers there, so never open one yourself. The agent's cursor glides to
  every part it builds, a chat label beside it says what it is doing, and the view follows (spotlight). 20 frames
  took 5 s at `--pace 0.5`, 61 steps 28 s at `--pace 1`. `--hold 60` keeps the session open after.
- Without `--live`: fast (61 steps in under 0.1 s), for rebuilds nobody watches. A product plan (research not
  `none:`) must `"save"`, and every run ends with that file open in the boss's window: figlive opens the window when
  none answers on 7800 and exits 1 `NOT IN THE WINDOW` when it cannot (then fix it and `open_file` it there before you
  report). `--no-viewer` is refused for a product plan.
- Plan: `{"research": ".brain/design/proposal/research.md", "app": true (pack checked with --app), "save":
  ".brain/design/proposal/proposal.figcraft", "open": "<optional .figcraft or figrest .json>", "fonts": [<files from design/scripts/fonts.mjs, section 5>], "defaultFonts": "<only when a built-in face is right: why>", "steps": [...]}`; a step
  is `{"label", "screen": "<### name in the pack>", "state": "empty|loading|error" (none = default), "mode": "light|dark" (none = light), ...}` plus one of
  `"batch": [{command, params, as}]` (any command, `$ref` chaining), `"draw": {"tool": "rectangle", "from": [x,y],
  "to": [x,y]}, "name", "props"` (a real mouse drag), `"text": {"text", "x", "y", "parent", "name"}` or
  `"call": {"tool", "args"}`; `"cursor": [x,y]` overrides where the cursor goes. A drawing that is not a product screen
  (a diagram) says `"research": "none: <reason>"`. A big document: generate it with figrest (section 2), `open` it,
  and give each screen and state a step that builds its own parts live.
- It starts its own FigCraft engine (`figcraft-cli mcp`), so it works while the session's figcraft MCP is down: never
  fall back to HTML for that. `--live` needs the network, so it runs without automation roots (session bound to
  this machine only); without `--live` it is confined to the working folder and the save folder. The window it opens
  for the boss has a control port and no automation roots (roots switch live sessions off).
- `node scripts/figlive.mjs --selftest`: a headless viewer joins and must see the cursor, the chat and every layer.

## 3. Commands you will use (full list: `references/commands-full.txt`, 1270 lines; grep it)
- Inspect: `inspect_document {depth, fields, outline, allPages}`, `get_node {id, depth, fields}`,
  `node.find {root, name, type, text, component, limit}` -> {ids}, `inspect_ui` (view, tabs, windows),
  `doc.list`, `file.fontReport`, `text.missingFonts`, `component.list`, `style.list`, `variable.list`.
- Create / change: `node.create` (also via create_node), `node.set` (every property: fills, strokes, layout,
  sizing, text, `path` for vectors), `node.reparent`, `edit.duplicate {map:true}`, `edit.delete`,
  `component.create`, `instance.create`, `instance.setProperties`, `component.resetOverride {id, group}`,
  `style.create`, `variable.createCollection` / `variable.create` / `variable.bind` / `variable.import`,
  `file.import {path, x, y}` (SVG becomes an editable frame; images become image layers).
- Batch: `batch {commands, onError: abort|continue, label}`; name a step `as`, reuse with `{"$ref": name}` or
  `{"$ref": name, "field": "ids/0"}` (`{"$step": n}` by position). One undo step.
- View: `page.select {id}`, `view.zoomFit`, `view.zoomSelection`, `view.set {zoom, cx, cy}`.
- Files: `open_file {path, reload}`, `save_file {path}`, `file.close {index, force}`, `doc.switch {index}`.
- CLI for lookups and scratch checks, never the boss's view: `figcraft-cli commands | info | convert | render |
  export | run SCRIPT.json [--in f] [--out f]` (`run` takes a JSON array of {command, params}, one all-or-nothing step).
Refresh the catalogue for a new FigCraft version: `figcraft-cli commands > references/commands-full.txt`.

## 4. Verify loop (every frame)
`screenshot {ids:[frame], maxSize}` and look: clipped or wrapped labels, overlaps, off-palette colour, empty
vectors, text in a fallback font. `screenshot {window:true}` shows what the boss sees. `file.fontReport` must list
0 missing families. Fix in the generator, never by hand on 7,000 layers.

## 5. Fonts (the pack's type is drawn as picked, never substituted)
FigCraft draws only its built-ins (Inter, Roboto, Open Sans, Lato, Montserrat, Nunito Sans, Poppins, Playfair
Display, Merriweather, Liberation) and the system's faces; any other family falls back silently. That is why designs
kept landing in the same AI faces (noirlynx 2026-11: the pack picked Hanken Grotesk / Albert Sans / Libre Franklin,
the frames were drawn in Nunito Sans / Montserrat / Lato; boss 2026-11-10). So:
- Fetch the picked faces: `node .claude/skills/design/scripts/fonts.mjs "Hanken Grotesk:400,600,700" "<Family>:<w>"
  --out .brain/design/proposal/fonts` (Google Fonts TTF files in the project, not an install) and list the printed
  paths in the plan's `"fonts"`. figlive loads them (`text.loadFont`) into its engine AND the boss's window.
- A loaded font lives only in that app session; the .figcraft does not keep it. Opening the proposal again (figcraft
  `open_file`) needs `text.loadFont {path}` per file in that window first.
- A product screen draws only the faces it loaded: figlive `--check` refuses a plan naming a built-in or system face,
  and after the build exits 1 with `FONT NOT PICKED: <family>` for any face not loaded from `"fonts"` (a text without
  fontFamily is Inter; styles, variables and an opened file count too) and `FONT FALLBACK: <family>` for one it could
  not draw. `"defaultFonts": "<why>"` (a desktop exe that must ship 0 KB of fonts) lifts both but the fallback.
  `fonts.mjs` fails on a weight Google lacks; a bold drawn from a 400-only file is not detected: fetch every weight used.
- What this machine has: `figcraft-cli run` a one-step script `[{"command":"text.fonts","params":{}}]`. `font.download`
  needs downloading turned on in the app (Text > Missing fonts): a setting of the boss's, ask first.

## 6. Gotchas (each cost real time)
- Instance text overrides must keep a fixed-width label's width, or it wraps ("Leba r"): `fixedw()` on the label,
  and figrest re-measures only auto-width text.
- Auto layout is computed on the first edit after import, not at import: run the vector/batch step even when there
  are no vectors (any harmless `node.set`), then screenshot.
- Setting a layout property on one instance creates an override: undo it with
  `component.resetOverride {id, group: "alignment"}`.
- Thumbnails of a big SVG at a small scale: drop texts and tiny rects (`svg_layers(min_text_scale)`), or a page
  of 12 thumbnails reaches 10,000 layers.
- `node.find` searches strict descendants of `root` (default: the current page): give each page id explicitly.
- Tabs pile up across reopen rounds: `doc.list`, then `file.close {index, force:true}` from the highest index.
- A shadow under every card, an icon in a coloured circle, text abbreviations as icons: design tells, not FigCraft
  limits. Draw real icons as vectors (paths) and follow the `design` skill.

## 7. Motion in the prototype (what moves comes from DESIGN.md / the direction's Motion & Interaction table)
FigCraft shows interaction and transitions, not every effect (no scroll scrub, shimmer loop, haptics or physics):
draw what it can here, demo the rest live in `proposal.html` (`design/references/app-motion.md` section 9).
- Component states: one component set per interactive component with variants `State=Default|Hover|Pressed|Focus|
  Disabled|Loading` (`component.combineVariants`, `component.addVariant`), then on the Default variant
  `proto.interaction.add {id, trigger:{type:"onHover"}, action:{type:"changeTo", dest:<Hover variant>},
  transition:{type:"smartAnimate"}, durationMs:<token>, easing:{customBezier:{x1,y1,x2,y2}}}` (onPress -> Pressed).
- Screen changes: `navigate` / `back` / `openOverlay` with the direction's transition (`push` or `slideIn` +
  `direction` for forward navigation, `dissolve` for fade-through, `smartAnimate` for a shared element: keep the
  moving layer's name identical on both frames); `easing:{customSpring:{mass, stiffness, damping}}` for a spring token.
- A feedback sequence (submit -> loading -> success or error): frames or variants chained with
  `trigger:{type:"afterDelay", ms}` + `changeTo` / `navigate`.
- Durations and curves are the DESIGN.md tokens, never FigCraft's presets by habit; exit faster than enter.
- Check: `get_node` lists `interactions`, `proto.flow.add` names the flow, `proto.preview {start}` plays it in the
  boss's window; a smartAnimate that jumps means a renamed layer.
