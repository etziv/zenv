---
name: figcraft-design
description: "Drive the FigCraft design app through the `figcraft` MCP tools: draw screens, tokens, components and prototypes in a .figcraft file, and read a FigCraft frame for code. Use ONLY when the figcraft tools are available and the work happens in a FigCraft file (the designer's proposal screens, or reading the frames the boss chose). Choosing the look is the `design` skill; building screens from DESIGN.md is `frontend`."
license: MIT
metadata:
  author: dnviti
  version: "1.0.0-kit" # x-release-please-version
---

# FigCraft design

> **In this kit** (upstream: github.com/dnviti/artcraft-skills, main as of the 2026-10-30 commit; licence in
> `.claude/NOTICE.md`; only this note and the description differ from upstream):
> - Before the first node, the `design` skill's "Before ANY drawing" gate: `.brain/design/proposal/research.md` passes
>   `library.mjs check`, and you read it, DESIGN.md (once chosen) and `library.mjs show` for every library entry the
>   screen names. A designer or worker spawned only to draw in FigCraft does this too; no pack: stop and say so.
> - The `designer` uses it for job 0's key screens. What to build and where to save comes from
>   `design/references/web-research.md` section 5 (`.brain/design/proposal/proposal.figcraft`, PNGs in `screens/`).
>   The design skill's anti-slop house rules, DESIGN.md and the boss's choices win over sections 2-3 here.
> - Divergent directions: the kit's DESIGN card always carries a recommendation with its reason, and the boss still
>   picks or hybridises; nothing is chosen for him. As a subagent, return the direction cards to the dev.
> - The adversarial art-director review (section 7 step 5) runs after the designer returns: the dev starts it as a
>   fresh `general-purpose` agent with the screenshots, the brief and `references/distinctiveness.md` section 8.
> - Skills it names that this kit does not have (`vectorcraft-illustration`, `effectcraft-motion`): skip them and
>   make the asset another way. Installing anything (fonts, Playwright browsers, svgo) needs the boss's yes.
> - Read only the section and the one reference you need, not the whole skill.
> - Section 9 on Windows: no `setsid`/`xdg-open`. Open the file with
>   `Start-Process (Join-Path (Split-Path $env:FIGCRAFT_CLI) 'figcraft.exe') '<absolute path>'`, or just hand over the
>   path. As a subagent, return the path with "approval pending"; the DESIGN card is the approval.
> - Section 10 is for the `frontend` skill reading the chosen frames: DESIGN.md stays the master, generated code a hint.

FigCraft is an open-source, Figma-class interface design app (frames, auto layout, components and
variants, styles, variables with modes, prototyping, Dev Mode code). You drive it through the
`figcraft` MCP server. This skill tells you **what to build, in what order, to what quality bar**,
and how to carry a design into code.

**Uniqueness is the default.** Left alone, a model converges on the average of every UI it has seen
(centred hero, three cards, a safe sans, a purple gradient), and adjectives like "bold" or "unique"
in a prompt do not move it. Distinctive work comes from improbable inputs (a metaphor from outside
software, real references that are not SaaS galleries), arbitrary constraints that block the default
answer, the user choosing among genuinely divergent directions, and critique from someone who did not
make the work. The steps below build those four in; the user's explicit direction, brand or existing
design system always wins over them.

- Detailed command ids and params: `references/command-catalog.md`
- Step-by-step recipes (mobile screen, design system, landing page, prototype flow): `references/workflows.md`
- Design → code in the user's stack: `references/design-to-code.md`
- Traps and fixes: `references/gotchas.md`
- Distinctiveness detail (DESIGN.md template, metaphors and constraints, fonts, OKLCH helper,
  extended ban list, consistency audit, full review prompt): `references/distinctiveness.md`
- Attribution: `references/sources.md`

## 0. Ground rules for the tools

The MCP tools: `create_node`, `set_properties`, `get_node`, `inspect_document`, `screenshot`,
`run_command`, `batch`, `list_commands`, `select`, `export`, `open_file`, `save_file`, `undo`,
`redo`, `pointer`, `key`, `type_text`, `inspect_ui`. They are deferred: load the schemas you need
with ToolSearch (`select:mcp__figcraft__batch,mcp__figcraft__run_command,...`) before calling them.

- **Discover before guessing.** Every menu action is a command. Run `list_commands {filter}` with a
  narrow filter (`"variable."`, `"style."`, `"component."`, `"proto."`, `"inspect."`, `"export"`)
  and trust its param doc over memory or over the online docs: the installed server can be older
  or newer than the repo docs (e.g. some builds lack `inspect.code` `tailwind`/`vue`).
- **Coordinates**: page units (1 unit = 1 px), x right, **y down**. `x`/`y` are **relative to
  the parent frame** (groups don't count). `absoluteBounds` in inspect output is page space.
- **Auto layout owns position**: inside an auto-layout frame, never set x/y; set `layout`
  (`horizontal|vertical|grid`), `gap`, `padding` (n or `[t,r,b,l]`), `mainAlign`
  (`min|center|max|spaceBetween|spaceAround|spaceEvenly`), `crossAlign` (`min|center|max|baseline`),
  `sizingH`/`sizingV` (`fixed|hug|fill`), `wrap`, `crossGap`, min/max sizes.
- **Colours** are `"#RRGGBB"` or `"#RRGGBBAA"`. `fill`/`stroke` accept a hex, `"none"` or null;
  gradients `{"type":"linearGradient","stops":["#hex","#hex"]}`. On text, `fill` is the text colour.
- **Batch for atomicity**: `batch {commands:[{command, params}], label}` runs as **one undo step**
  and rolls back entirely on failure. Refer to an earlier step's result id with `{"$step": n}`
  (zero-based); `{"$step": n, "field": "ids/0"}` picks another field. Build whole subtrees this way.
- Commands act on `ids` when given, else on the selection; new layers become the selection.
- Programmatic calls never open dialogs. A failing command leaves the document unchanged.
- **Headless vs attached**: with `figcraft-cli mcp` there is no window (screenshots render the
  document); with `--attach <port>` the user watches the live app. File reads/writes may be
  confined to read/write roots; a refused path returns a readable error: pick an allowed folder.

## 1. Workflow overview

Scale the work to the request (one screen ≠ a full library). For anything non-trivial follow:

1. **Brief** → product, audience, primary job, platform, content; operational or showcase.
2. **Design plan** → tone, metaphor, constraints, tokens + layout concept; critique it against the
   brief for genericness *before* building.
3. **Divergent directions** → 2–6 directions; the user picks or hybridises (section 2).
4. **Foundations in FigCraft** → variable collections with modes, paint/text/effect styles.
5. **Components** → the anchor component first, then the rest in its image; one signature interaction.
6. **Screens** → device frames with nested auto layout, built from component instances and real copy.
7. **Verify loop** → numbers and screenshots; fix; repeat; then a clean-context adversarial review.
8. **Prototype** → interactions, flow starting point, device settings.
9. **Show and approve** (mandatory, always last) → save the design to a `.figcraft` file, open it
   in the FigCraft app, then stop and wait for the user's explicit acceptance (section 9).
10. **Handoff / code** (if asked, and only after acceptance) → mark ready for dev, export assets,
    generate and adapt code.

State a short scope + acceptance checklist before mutating; give updates at phase boundaries, not
per call. Stop when the checklist passes; don't silently grow a screen request into a design system.

### Starting points

- Empty document: work on the current page; add pages for structure when the work is large
  (`page.add {name:"Foundations"}`, `"Components"`, `"Screens"`, `"Prototype"`).
- Existing file: `open_file {path}`, then `inspect_document {depth: 2}` and `style.list`,
  `variable.list`, `component.list` to reuse what exists. **Never redraw what a component, style or
  variable already provides.**
- Fast start: `run_command template.list`, then `file.newFromTemplate {template}` (`mobile-app`,
  `landing-page`, `dashboard`, `design-system`, `wireframe-kit`, `social-posts`, `slides`,
  `icon-set`). Templates are fully editable; restyle them so the result isn't the template.
- Built-in MCP prompts (`prompts/list`): `mobile_screen`, `design_system`, `landing_page`,
  `prototype_flow`. Their recipes, upgraded with this skill's quality bar, are in
  `references/workflows.md`; use them as skeletons, not as the visual direction.

## 2. Brief and design plan (do this before any node exists)

If the brief doesn't name the subject, audience and primary job, propose one concrete answer for
each and confirm when the work is substantial. The subject's world (industry, materials,
vocabulary, users' context) is where distinctive choices come from: a field-service app for
plumbers and a meditation app should not share a look. The brief also fixes:

- **Operational or showcase.** Operational UIs (admin, internal tools, consoles, dashboards used
  all day) keep conventional control placement, states, feedback, keyboard paths and terminology;
  their uniqueness lives in type, composition, material, motion and microcopy. Showcase UIs
  (landing, campaign, portfolio) may bend layout conventions, never accessibility.
- **The one memorable thing**, in a single sentence: what a viewer remembers after closing it.
- **Tone in 3 non-generic words** that describe a world ("ledger-precise", "salt-worn",
  "backstage"). Never modern, clean, minimal, professional, sleek, intuitive, elegant, simple.

Write a compact plan (in chat, a few lines each):

- **Metaphor from outside software** (a ship's logbook, a railway timetable, a synthesiser panel, a
  seed packet), translated into structure, type, interaction and copy, not just colour: a logbook
  becomes dated entries, ruled columns, margin annotations and a "signed off" confirmation.
- **References**: the user's, or real ones outside SaaS galleries (editorial sites, posters,
  packaging, manuals, instrument panels). Note what you take from each (the grid, the size
  contrast, the treatment of numbers), never "the style"; never copy logos, artwork or a layout.
- **At least 2 arbitrary constraints** that block a default you would otherwise reach for: an odd
  column count, two weights on a 1:3 size ratio, radius 0 everywhere, one colour per screen plus
  black, a palette sampled from a non-digital photo. A constraint that changes nothing is replaced.
- **Colour**: one dominant hue and one sharp accent, neutrals tinted toward the dominant (not five
  timid tints): 4–6 named hex values (background, surface, text, muted text, accent, one status
  colour if needed), plus a Dark counterpart re-picked per token, not inverted, when theming is in
  scope. OKLCH makes even lightness steps easy; FigCraft takes hex (helper in the reference).
- **Type**: a display and a text face that contrast in structure, chosen for this subject; a
  modular scale (e.g. 12/14/16/20/24/32/48), weights, line heights (≈1.4–1.6 body, ≈1.1–1.25
  display), tighter tracking at large sizes. Inter, Roboto, Arial, Helvetica, system-ui and Space
  Grotesk only if the brand uses them or the plan says why (sources and exceptions in the
  reference §4). Check
  `text.fonts {family}` / `text.missingFonts`; installing a font changes the system: ask first.
- **Spacing & shape**: 4- or 8-pt scale; radius scale tied to hierarchy (not one radius
  everywhere); elevation used sparingly; density at an extreme (generous or truly dense), not the
  comfortable middle.
- **Layout**: frame size (`frame.presets`), grid/columns, alignment, a quick ASCII wireframe; where
  the one memorable thing spends its boldness (hero type, an illustration, a data visual, a colour
  field). Everything else stays quiet.

When divergent directions follow, the plan fixes only the brief-level items (subject, the one
memorable thing, tone); metaphor, constraints, type and palette are set per direction, and the
chosen one goes into `DESIGN.md`.

Then review the plan: if a part is what you'd produce for *any* similar brief, change it and say
what you changed. The user's explicit direction always wins over these defaults. Work that spans
sessions or goes to code persists brief and plan as `DESIGN.md` beside the `.figcraft` file or at
the project root (template in the reference); otherwise the plan in chat is enough.

### Divergent directions

Generate, then let the user choose. Light requests (one screen, a restyle) get 2–3 quick
directions; a new product, landing page with ambition, design system or redesign gets 4–6. Small
edits skip this; otherwise skip it only when the user already gave a direction or an existing
brand/system fixes the look.

- Every direction differs from every other on **all four axes**: layout structure, type, palette,
  interaction behaviour. Two that share two axes are one direction: drop one and redo it. Each
  renders the **same anchor screen** with real copy (desktop and mobile for full identity work).
- **One page per direction**: `page.add {name:"Direction A - Logbook"}` (the new page becomes
  current, so new frames land on it). **One variable collection per direction**
  (`variable.createCollection {name:"Dir A", modes:["Light"]}`) with the same slash names in each:
  identical names coexist across collections and `variable.export {collections:["Dir A"],
  format:"css"}` exports one alone (both verified). Bind layers even in sketches.
- **Build FigCraft directions sequentially, never in parallel subagents**: all would drive one
  headless session where `page.add`/`page.duplicate` switch the current page, `open_file` the
  document, and concurrent calls are unverified. Code-side directions may run in parallel.
- **Present**: verify each (section 7 steps 1–4), save and open the file (section 9), then give each
  direction a short card (name, what makes it recognisable, the metaphor reading, the risk,
  screenshots) and ask with the multiple-choice question tool (AskUserQuestion in Claude Code; else a
  numbered list), multi-select so the user can hybridise ("grid of A + type of C"). **Never pick for
  the user**; if asked for a recommendation, give its reason and still let them choose. This rule
  covers creative directions, while technical options may carry a labelled recommendation. Inside a
  subagent, return the options (cards, screenshot paths, brief) to the orchestrator, which asks the
  user. Keep rejected direction pages in the file.
- **Promote the winner**: `variable.renameCollection {collection:"Dir B", name:"Color"}` (verified),
  move hybrid values in with `variable.set`. `variable.deleteCollection` leaves bound layers with
  raw values: delete rejected collections only when those pages no longer need to stay editable.

## 3. Design quality bar

Distinctive, intentional, accessible. Generic AI-looking design is the main failure mode.

**Typography carries personality.** Pick faces deliberately for this subject; build a real scale;
make headlines an active design element. Keep line length under ~80 characters (set `maxWidth`
or a fixed text width). Serif body text gets a bit more line height.

**Avoid default tells** unless the brief asks for them:
- one word in a headline accented by colour/italic/bold; ALL-CAPS tracked eyebrow labels above
  every heading; labels joined with middle dots; `→` appended to every link/button;
- the SaaS card kit: everything chopped into identical rounded cards with one radius and the same
  soft grey shadow, gradient washes as decoration;
- stock palettes that read as "AI default": warm cream + serif + terracotta accent; near-black +
  one acid-green accent; hairline-ruled newspaper grids (archival, editorial and manual metaphors
  pull straight here: let a constraint take another route);
- big number + small label + gradient accent as the reflexive hero; 01/02/03 markers on content
  that is not a sequence; monospace for small data labels by habit;
- the template page: centred hero with headline, subline and primary + ghost buttons; a three-up
  icon + title + two-line feature grid repeated down the page; a greyed "trusted by" logo strip;
  every section the same width, alignment and rhythm;
- purple/violet/indigo gradients on white; glassmorphism, blobs, mesh gradients or noise as filler;
  Dark mode as a mechanical inversion;
- emoji as icons; a stock icon set at its default stroke and size; fade-and-slide-up on every
  element and hover scale on every card. More in `references/distinctiveness.md`.

**Structure is information.** Borders, dividers, numbering, labels and eyebrows must encode
something real. Use proximity and spacing for grouping first. **Traceability**: every visible
choice traces to the brief, the metaphor or a constraint; if it doesn't, remove it.

**Hierarchy**: one primary action per view; size, weight and contrast define reading order; align
to a grid; generous, consistent spacing from the scale (no 13 px or 27 px one-offs).

**Colour**: build from tokens; neutrals with a slight tint toward the brand are fine, but contrast
must pass WCAG AA (4.5:1 body text, 3:1 large text and UI boundaries). Status colours never carry
meaning alone (pair with text/icon).

**Restraint**: one bold idea; remove one accessory before finishing. Motion in prototypes only
where it explains a change (Smart Animate between states, a single entrance moment).

**Copy is design**: real, specific content for the subject (no lorem ipsum). Name things the way
users think of them; sentence case; active verbs on buttons ("Save changes", not "Submit"); the
same action keeps its name across the flow ("Publish" → "Published"); errors say what happened and
how to fix it; empty states invite the next action. Write in the product's own voice and its
metaphor's vocabulary (the logbook UI "logs" and "signs off"), never "Unlock the power of…",
"Seamless", "Elevate your…"; loading, empty, error and success are designed moments in that voice,
not "No data" or "Something went wrong".

**Quality floor (state coverage)**: components show default/hover/pressed/disabled/focus (visible
focus ring) and, where relevant, error/empty/loading; touch targets ≥ 44×44 on mobile.

## 4. Foundations: variables and styles

Prefer **variables** for raw values and themes, **styles** for composite typography and effects.

```json
batch {"label":"Foundations","commands":[
 {"command":"variable.createCollection","params":{"name":"Color","modes":["Light","Dark"]}},
 {"command":"variable.create","params":{"collection":"Color","name":"color/bg","type":"color","values":{"Light":"#FBFAF7","Dark":"#121417"}}},
 {"command":"variable.create","params":{"collection":"Color","name":"color/fg","type":"color","values":{"Light":"#1B1D21","Dark":"#ECEDEF"}}},
 {"command":"variable.create","params":{"collection":"Color","name":"color/primary","type":"color","values":{"Light":"#1F5C4A","Dark":"#7CC4A8"}}},
 {"command":"variable.createCollection","params":{"name":"Space","modes":["Default"]}},
 {"command":"variable.create","params":{"collection":"Space","name":"space/4","type":"number","value":16}},
 {"command":"variable.create","params":{"collection":"Space","name":"radius/md","type":"number","value":10}}]}
```

- Name with slash paths (`color/bg`, `space/4`, `radius/md`): `variable.export {format:"css"}`
  turns `color/primary` into `--color-primary`, so names become your code tokens.
- Set `codeSyntax` (`variable.update {variable, codeSyntax:{WEB:"--color-primary"}}`) when the
  codebase already has token names, and `scopes` so pickers only offer sensible variables.
- Semantic layer: alias with `variable.set {variable, mode, value:{"alias": <primitive>}}`.
- **Text styles**: make a text layer with the exact props, then
  `style.create {type:"text", name:"Text/Heading L", from:<id>}`; or pass `value`
  (`{"fontSize":32,"lineHeight":"120%","letterSpacing":"-1%"}`). Same for `paint` and `effect`.
  `style.bind` binds a variable inside a style (e.g. a paint style's `fill`).
- Bind layers: `variable.bind {ids, field, variable}`; fields include `fill`, `stroke`,
  `cornerRadius`, `gap`, `paddingHorizontal`, `width`, `opacity`, `fontSize`, `characters`,
  `visible`. Check `boundVariables` in `get_node`.
- Theme check: `variable.setMode {ids:[frame], collection:"Color", mode:"Dark"}`, screenshot, then
  set back (`mode: null` = Auto).
- Colour planned in OKLCH: convert to hex for the variables (helper in
  `references/distinctiveness.md`) and keep both in the plan or `DESIGN.md`; code can use OKLCH.

## 5. Components and variants

Build components **on a Components page or an empty canvas area, never inside a screen's
auto-layout flow** (a component set dropped into a flow resizes and clips it).

**Anchor first.** Finish the one component that carries the identity (header, main card, data
table, key form, hero) with variants and states before anything else; in full identity work get it
approved through the section 9 loop. Everything else looks made by the same hand: it reuses the
anchor's variables, text styles, radii, strokes and spacing rhythm; a new value goes into the
variables (and `DESIGN.md`) first, never as a one-off on a layer.

**One signature interaction**: a single behaviour repeated coherently (how panels arrive, how data
reveals, a press feedback), not ten scattered hovers. Prototype it with Smart Animate (section 8);
in code it needs a `prefers-reduced-motion` equivalent that keeps the meaning.

1. Build the base as an auto-layout frame (hug height, sensible padding), with tokens bound.
2. `component.create {ids:[frame]}` → main component.
3. `component.addVariant {id}` duplicates it into a set; variants are first named
   `Property 1=Default`, `Property 1=Variant2`. Rename right away:
   `component.editProperty {id:<set>, name:"Property 1", newName:"Variant"}`, then
   `{name:"Variant", value:"Variant2", newValue:"Secondary"}`. Add another axis with
   `component.addProperty {id:<set>, name:"State", type:"variant", default:"Default"}`, then
   `component.addVariant {id:<variant>, property:"State", value:"Hover"}` (addVariant refuses a
   property that doesn't exist yet). Alternative: separate components named
   `Variant=Primary, State=Hover` and `component.combineVariants`.
4. Edit each variant **fully** (fill *and* text colour *and* stroke): restyling one property
   detaches its variable; rebind the variant's own token.
5. Text/boolean/instance-swap properties: `component.addProperty {id, name, type:"text"|"boolean"|
   "instanceSwap"|"slot", layer}`; slots via `component.makeSlot` for free-form content areas.
6. Use instances in screens: `instance.create {component:<set or variant>, parent, x?, y?}`;
   configure with `instance.setProperties {ids, properties:{Variant:"Secondary"}}`; override text
   with `type_text {id:<text layer in instance>, text}` (read the inner id with `get_node`).
7. Verify: `component.properties {id}` (check `conflicts`), `component.playground {id, properties}`
   to preview a combination and its code without touching the document.

Name layers semantically (`Button`, `Label`, `Icon`), because names drive generated code,
`data.fill` keys and override matching on swap.

## 6. Screens with auto layout

- Device frames: `frame.create {preset:"iPhone 16", x, y, name, props}` (`frame.presets` lists
  them: iPhone 16 393×852, Android Compact 412×917, Desktop 1440×1024, MacBook Air 1280×832…).
  Place screens side by side ~100 px apart on y = 0.
- Make the screen a vertical auto-layout frame; nest section frames with `sizingH:"fill"` and their
  own layout. Rows are horizontal frames; use `mainAlign:"spaceBetween"` for bars, `grid` layout
  (`gridColumns`, `columnTracks:["1fr","1fr","1fr"]`, `columnGap`) for card grids.
- Text: set a fixed width or `sizingH:"fill"` for paragraphs so they wrap; `hug` for labels;
  `textAutoResize:"height"` for growing copy; `text.fit {ids, maxLines}` for headlines in fixed boxes.
- Scrolling content taller than the device: let the frame grow (`sizingV:"hug"`) for a long page,
  or keep the device height and set `overflow` for prototype scrolling; fixed headers via
  `absolute: true` + constraints.
- Images: `image.place {path|data, parent, width, height, mode:"fill"}` (needs a readable path).
  Don't use third-party logos or copyrighted images; use neutral placeholders the user can swap.
- Original assets: at least one asset no template has, drawn from the metaphor: icons, patterns,
  illustration via `vectorcraft-illustration` ("Icon on a 24 pt grid" recipe; keep the master),
  animated ones (logo sting, empty state, loader) via `effectcraft-motion` (Lottie, WebM with
  alpha). Place PNG exports in mockups with `image.place`. No emoji icons, no stock illustration.
- Repeated content: build one card, then `data.fill {id, rows:[{Title:"…", Price:"…"}]}` or
  instances; apply repeated styling to many ids in one `batch`.
- Build section by section (one `batch` per section), verifying after each.

## 7. Verify loop (mandatory)

After each major step:

1. `get_node {id:<frame>, depth:3}`: check sizes, `sizing`, `layout`, fills, `boundVariables`,
   text props, `overrides`. Look for width 0/1 text, children wider than parents, unexpected
   `fixed` sizing, unbound hex values that should be tokens, spacing or radii off the scale, fonts
   or sizes outside the plan. Check `text.missingFonts`: a fallback makes the screenshot lie.
2. `screenshot {ids:[frame], maxSize:1200}` and **look at it** critically: alignment, rhythm,
   contrast, clipping, truncated text, empty boxes, the hierarchy you planned.
3. Fix with `set_properties` / `batch`; use `undo` for a bad batch (one step).
4. At the end: screenshot every deliverable frame, both theme modes if themed, and report the frame
   ids, what was built and any known limitations.
5. **Adversarial art-director review** (anything beyond a small edit, before section 9): a fresh
   agent with a clean context (not a fork) critiques the screenshots against the brief and the ban
   list; prompt and triage in `references/distinctiveness.md` §8. Inside a subagent, return the
   critique inputs (screenshot paths, brief, tells) to the orchestrator, which starts the reviewer.

A picture beats a JSON diff for aesthetics; JSON beats a picture for exact values. Use both.

## 8. Prototype

- Wire clicks: `proto.interaction.add {id:<button>, trigger:{type:"onClick"},
  action:{type:"navigate", dest:<frame>}, transition:{type:"smartAnimate"}, durationMs:300,
  easing:"easeOut"}`; back buttons `action:{type:"back"}`; overlays `openOverlay` /
  `closeOverlay`; hover states via `onHover` + `changeTo` on component variants.
- Flow start: `proto.flow.add {start:<first frame>, name:"Main flow"}`; device:
  `proto.settings {device:"iPhone 16"}`.
- Stateful prototypes: `setVariable` / `conditional` actions with text expressions
  (`"count + 1"`, `"count >= 3"`); test with `proto.evaluate`.
- Verify: `get_node` shows `interactions` on triggers, `inspect_document` shows the page `flows`.
  Smart Animate matches layers by name: keep names identical across screens for elements that move.

## 9. Show and approve (mandatory final phase)

Every design ends here, after the verify loop (and after the prototype, if any). The headless
`figcraft-cli mcp` session has no window, so the user cannot see the design until you save it and
open the real app on it.

1. **Save** with `save_file {path}` (or `file.saveAs {path}`; no extension adds `.figcraft`) into
   the project directory, or the path the user named. Use a descriptive name (`login-screen.figcraft`)
   and an allowed write root. Note the full absolute path.
2. **Open FigCraft on that file**, detached so it doesn't block:
   `setsid figcraft /abs/path/design.figcraft >/dev/null 2>&1 &`
   (`figcraft [FILE]` is the desktop app binary, `/usr/bin/figcraft`; the MIME type is
   `application/x-figcraft`, so `setsid xdg-open /abs/path/design.figcraft >/dev/null 2>&1 &` works
   too.) If the app is already bridged/open on this design (`--attach`/`--control` session), don't
   launch another copy: the user already sees it, so just bring that file up (`file.open {path}`
   returns the index of an already-open file instead of opening it twice).
3. **Stop and ask.** Give the user the full file path and ask them to accept the design or describe
   the changes. Never continue on your own: no implementation, code, asset export or handoff
   before explicit acceptance. Silence or a vague reaction is not acceptance.
4. **Loop on changes.** Apply the requested changes, re-run the verify loop for what changed,
   `save_file` again, reopen/refresh FigCraft on the file, and ask again. Repeat until the user
   accepts.
5. **Inside a subagent** (it cannot talk to the user): do steps 1 and 2, then return the file path
   and a short summary of the design to the orchestrator, stating clearly that **user approval is
   pending**. The orchestrator asks the user and relays any requested changes back (to this agent
   or a new one); handoff/code waits for the acceptance.

## 10. Design → code

Follow `references/design-to-code.md`. Summary:

1. **Get the target**: frame/component id (ask, or find via `inspect_document`/`component.list`,
   or `inspect.readyForDev`).
2. **Context**: `get_node {id, depth}` (fetch big trees section by section),
   `inspect.properties {id}` (values with style/variable names), and a reference
   `screenshot {ids:[id], scale:2}` kept as the visual source of truth.
3. **Tokens**: `variable.export {format:"css"}` → CSS custom properties (default mode on `:root`,
   others on `[data-<collection>="<mode>"]`); map them into the project's token system
   (Tailwind theme, CSS vars, TS theme), reusing existing tokens when values match.
4. **Generated code as a spec, not as final code**: `inspect.code {id, language:"react"}` (also
   `css`, `html`, `leptos`, `swiftui`, `compose`; check `list_commands {filter:"inspect.code"}` for
   what this build supports, `tailwind`/`vue` may exist), `inspect.componentCode` for props.
   Translate into the project's components, layout primitives and conventions.
5. **Assets**: `inspect.assets {ids}` then `file.exportLayers {ids, assets:true, dir}` (icons SVG,
   images PNG) or `export {ids, format:"svg"|"png", scale:2, path}` into the project's asset
   folder (must be an allowed write root). Never redraw icons by hand or add icon packages when
   the design provides them.
6. **Reuse first**: search the codebase for existing Button/Input/Card/tokens before writing new
   ones; map FigCraft components to code with `codeConnect.set` / `codeConnect.import` so later
   `inspect.code` emits the real component calls. Outside libraries (Radix, shadcn/ui, effect
   collections) are raw material rewritten to the tokens, never the result; library, motion, font
   and 1440/390 screenshot rules are in `references/design-to-code.md` §11.
7. **Validate** the running UI against the reference screenshot: spacing, typography, colours,
   radii, states, responsive behaviour, assets; fix every in-scope mismatch, document intended
   deviations (accessibility, platform constraints).

Code → design (recreating an existing UI in FigCraft): read the source for structure, fonts and
tokens first, recreate it as editable layers (never paste a screenshot as the design), reuse the
file's components and variables.

## 11. Top gotchas (more in `references/gotchas.md`)

- x/y inside auto layout are ignored; children follow the flow. Use `absolute:true` to opt out.
- A child can't `fill` an axis its parent `hug`s: setting fill makes the parent fixed on that axis.
- Text without width hugs one long line; give paragraphs a width or `sizingH:"fill"`.
- Editing a bound/styled field (a new fill, font size…) **detaches** the variable or style. Re-bind
  afterwards, or change the variable/style instead of the layer.
- Instances refuse structural edits (move/resize/add children) outside slots: edit the main.
- `component.addVariant` without an existing property creates `Property 1=…` names; rename them.
- A whole `batch` rolls back on one bad step: read the error (it names the step), fix, rerun.
- `inspect_document` on big files truncates (`truncated` key): use `get_node` on subtrees.
- `screenshot` defaults to 1600 px max side; pass `ids` to focus and `scale` for detail.
- Fonts: missing families fall back silently; check `text.missingFonts` and pick installed ones.
- Exports/saves outside allowed roots fail; `export` without `path` returns data inline.
- Don't `save_file` unless the user wants the file saved or section 9 requires it; save where they say.
