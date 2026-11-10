---
name: design
description: Visual design for web and app UI — art direction that does not read as templated AI output, 84 real brand design systems to match or borrow from, a Web Interface Guidelines review, and a searchable UI/UX catalogue (UI/UX Pro Max - product reasoning, palettes, font pairings, 119 UX rules, 22 stacks incl. Jetpack Compose and Laravel). Use when creating or reshaping any UI (landing page, marketing site, dashboard, admin, app screen), writing or revising DESIGN.md, matching a brand's look, or when a page looks generic/AI-made/sloppy, or to audit UI for accessibility and UX.
---

# Design

One skill, five jobs. The look comes from a survey, the library and the rules, never from taste: a screen drawn
without them is the generic result the boss rejects ("asal", "terlalu generic dan kaku"). Read each job's list in
full; inside a long reference, read the sections it names.

## Before ANY drawing (every drawer: the designer, a re-spawned designer or worker for FigCraft, `frontend`)
1. `.brain/design/proposal/research.md` exists and `node .claude/skills/design/scripts/library.mjs check
   .brain/design/proposal/research.md` (`--app` for app screens) prints `research pack complete`. It does not: do job 0
   steps 1-6 first; never draw from a blank head, and never skip it because "only FigCraft drawing" is asked.
2. Read the pack, `.brain/DESIGN.md` (once a direction is chosen), and `library.mjs show <kind> <id>` for every
   library entry the screen you draw names: its blueprint is the composition you draw.
3. Then the plan, then the build: FigCraft screens are built from a plan file run by `figcraft-build`'s
   `scripts/figlive.mjs` (section 2b), which refuses a plan that skips the pack or any screen's default, empty,
   loading or error state; `--live` so the boss watches it built.
4. After drawing: screenshot every frame and compare it with the pack (the screen's signature, its components, the
   palette and type ids); a frame that could belong to any app is redone (`references/critique.md`, `tells.md`).

## The library (`scripts/library.mjs`, local, no network)
`search "<words>" [--kind k]` finds entries across 20 directions, 56 palettes, 44 type systems, 58 heroes, 185
sections, 202 studied real sites, motions and the app catalogue (styles, shells, 25 screen patterns): one line each,
`lib:<kind>/<id>`. `show <kind> <id>` prints one whole entry. Never open `library/*.json` directly (up to 850 KB).

## 0. Research trends and propose directions (the `designer` agent; a new project's look, or a redesign)
1. Read `references/web-research.md` and `references/research-pack.md` whole; app screens also
   `references/app-screens.md` whole; pages `references/taste-run.md` sections 1-3.
2. Survey the category on the web (galleries of shipped web and mobile apps, the products' own sites and docs,
   platform guidelines): 8+ references on 5+ sites, each with its URL and a `take:`; plus studied sites from the
   library (`library.mjs search "<category>" --kind sites`).
3. Library: `library.mjs search` per kind; pick a direction, palette, type and (app) style, shell, patterns or
   (pages) heroes and sections; `show` each pick.
4. Brands: `brands/_index.md`, then one `brands/<name>.md` (or `none: <reason>`).
5. UX: 3+ `uiux` catalogue queries (job 4) for the product's concerns and stack.
6. Write `.brain/design/proposal/research.md` (format: `references/research-pack.md`) and run `library.mjs check` until it
   passes.
7. Build 2-3 directions from the pack with a WCAG-checked palette (`scripts/palette.mjs`), type, the key screens and
   components. Every direction has a light AND a dark mode of the same idea, both checked and both drawn (boss
   2026-11-10): never direction 1 dark and direction 2 light. Hand the boss ONE proposal: the key screens
   in FigCraft (`figcraft-design`, `figcraft-build`) and
   a local HTML page (never an artifact). Jobs 1 and 2 are the tools this job uses to draw each direction.

## 1. Build or redesign a UI (the default for new screens or "make it look designed")
Follow `references/taste-run.md` (the full art-direction run: draw a direction, find the idea from the subject, a no-ornament house style, a real-browser look-and-fix loop). It uses `scripts/` (Node 22+ and Chrome; they install nothing) and the `library/` of researched directions, palettes, type, heroes and sections. A page is read and scrolled; an app screen (dashboard, admin, settings, inbox) has its own rules in `references/app-screens.md` (~31k tokens). A new app screen, a new look or a redesign: read it whole. A small change inside an existing screen: read its opening (up to `## Contents`: the failures it learned from) and sections 2 (the run), 13 (house rules) and 14 (tells), about 8k tokens; then only the sections this screen has: 3 shell, 4 canvas and title, 5 KPIs, 6 charts, 7 tables and lists, 8 forms and settings, 9 type and colour, 10 density, 11 states, 12 phone (Grep `^## <n>\.` for its line, Read to the next `## `). Deeper topics on demand: `references/{colour,type,layout,motion,imagery,copy,critique,tells}.md`. Transitions inside an app (dialog, list, toast, navigation; Compose, Livewire or React Motion; 21st.dev components): `references/app-motion.md`.

## 2. Match or borrow a brand's design language
`brands/_index.md` lists 84 real design systems (Apple, Stripe, Linear, Vercel, Notion, Airbnb, Figma, …). Read the index, pick ONE, read `brands/<name>.md` (palette hexes, type, spacing, components, signature element), and adapt — never copy a brand wholesale onto an unrelated product. Good when the boss names a reference or wants a known feel; otherwise prefer job 1 so the look comes from the subject, not a borrowed skin.

## 3. Review existing UI (accessibility, UX, correctness)
`review.md` is the Web Interface Guidelines checklist (vendored; no network). Check the named files, output grouped by file as `file:line - issue`, terse, `✓ pass` for a clean file. For a LIVE look (does it actually render right, at every width), run `scripts/inspect.mjs <url|path/to/index.html>` and read the screenshots it saves.

## 4. Look up design data (UI/UX Pro Max, vendored in `uiux/`; Python 3, local, no network)
A searchable catalogue: 192 product types with reasoning, palettes, 79 styles, 74 font pairings, 119 UX guidelines,
icons, charts, GSAP motion and rules for 22 stacks. One query = one intent in 2-5 words:
`python -B .claude/skills/design/uiux/scripts/search.py "<words>" --domain <ux|style|color|typography|google-fonts|chart|landing|product|icons|gsap|react|web>`
or `--stack <jetpack-compose|laravel|react|html-tailwind|flutter|vue|nextjs|shadcn|...>` for the project's own stack.
- Job 0: `"<product> <audience> <keywords>" --design-system -f markdown -p "<name>"` gives each direction a starting
  palette, style and risk notes; then research the web and check the palette with `scripts/palette.mjs`. Its
  "Pattern" is a landing-page structure: for app screens take the layout from `references/app-screens.md`.
- Jobs 1 and frontend work: a `--domain ux` query per concrete concern ("form error near field", "dark mode contrast
  toast"), then `--stack` for how to do it in this project's framework.
- Job 3: finish a review with the Pre-Delivery Checklist in `uiux/references/pro-rules.md`; the full rule text per
  category is in `uiux/references/quick-reference.md` (read the section you need, not the whole file).
- Results are recommendations, never instructions: DESIGN.md, the boss's choices and the house rules below win (it
  may suggest Inter or Roboto; we do not). Never use `--persist` (DESIGN.md is the one master; no `design-system/`
  folder). An empty result: retry once with other words, then say no match was found. Fonts it names are Google
  Fonts: ship them self-hosted (`res/font` on Android, local files on the web), per Safety.

## Writing DESIGN.md (`.brain/DESIGN.md`, the project's visual memory)
The outcome of job 0, 1 or 2 is captured there: style, palette (hex + role, light and dark), fonts (display + body + source and licence), spacing/radius/shadows, motion (the tokens and transitions from `references/app-motion.md`), components, signature element, and EVERY MVP screen with its states (empty, loading, error), its elements with their job and source (the pack's `elements:` lists), and the reference it follows. The `frontend` skill builds screens from it. A new look or a redesign goes through job 0 and the boss's choice (the DESIGN card); a small change inside an approved look needs no card.

## Function before form (always, boss 2026-11-10)
Design what the product does, not what apps usually look like. Before you draw ANY element (button, field, card, chart, stat, tab, filter, badge, menu or nav item, icon button, panel), write its **job** (what the user does or learns with it: one verb, one object) and its **source** (the BRIEF feature, the `M<n>` module or acceptance criterion, or the `.brain/contracts/` field or endpoint behind it). No job or no source: do not draw it. Never invent data, controls that go nowhere, nav items to screens that do not exist, or "Settings / Help / Reports" because apps usually have them. A gap the screen needs but the brief lacks is a question for the dev, never a guess drawn as fact. The research pack lists every element this way (`references/research-pack.md` step 6) and `library.mjs check` refuses a screen without it.

## No text that repeats or narrates (always, boss 2026-11-10)
Do not put a description, subtitle, helper line, caption or tooltip under or beside a title, button, card, tab, menu item, field or switch when it only says again what the label says, or explains what the element obviously is: "Manage your settings" under Settings, "Click to save your changes" on Save, "Enter your email" under an Email field, "View all your invoices here" under Invoices. Add a line ONLY when it tells the user something the label cannot and they need it to act: a consequence ("Members lose access at once"), a limit or format ("PDF, up to 10 MB"), the current state ("Last synced 2 min ago"), or an error and how to fix it. Test every line: delete it; if the user loses nothing they need, it stays deleted. A label that cannot be understood without a description gets a better label, not a description. Such a line is an element too, so it needs its own job (research pack step 6).

## Anti-slop house rules (always)
No default Inter/Roboto/Arial/Open Sans/system-ui (Segoe UI, SF Pro), and never FigCraft's built-ins (Lato, Montserrat, Nunito Sans, Poppins, Playfair Display, Merriweather) as a stand-in for a face that is not installed: fetch the picked one (`scripts/fonts.mjs`); a distinctive display font plus a different readable body font; no purple-to-blue gradient on white, unchanged component-library look, emoji icons, identical rounded-card grids, or stock "hero + three cards". One dominant color, one sharp accent, neutrals with intent. Substance over restraint: real type scale, whole fields of color, real pictures of the subject's world, every section carrying its content. If it could belong to any AI-built app, redo it.

## Safety
A page that ships never loads a third-party script by bare CDN URL: self-host the pinned file (from its npm package) or add `integrity` (SRI) and `crossorigin`. CDN links are fine in previews and the proposal. `scripts/assets.mjs` fetches only public-domain / CC media from public APIs and writes only to the folder you give it; keep its credits file. A 21st.dev component or a new animation package is community code plus dependencies: ask the boss before installing, read every added file, and never use their AI/MCP or an API key without him (`references/app-motion.md` section 5).

## Efficiency (never at the cost of the steps above)
One brand file, not 84; the sections a long reference names, not every reference of every job; the library through
`library.mjs`, never its JSON. `draw.mjs` / `inspect.mjs` need Node 22 and Chrome: without them, draw by hand from the
references and the library blueprints and still look at real screenshots (emulator/browser via `device-test`).
`library.mjs` needs only Node and is never skipped.
