# The research pack (`.brain/design/proposal/research.md`)

The look of a product comes from what was surveyed, what the library already knows and the rules, never from taste.
This file is the proof, written BEFORE any screen is drawn, and the brief every later drawer reads (a designer
re-spawned for FigCraft, a worker, the `frontend` build). `node .claude/skills/design/scripts/library.mjs check
.brain/design/proposal/research.md` (add `--app` for app screens) must print `research pack complete`.

## How to fill it
1. UX rules, first (design SKILL.md "First: UI/UX Pro Max"): 3+ catalogue queries
   (`python -B .claude/skills/design/uiux/scripts/search.py "<product> <audience>" --design-system -f markdown`,
   `"<concern>" --domain ux`, `--stack <stack>`), each with the rule it gave and where it applies; 1+ of them on
   motion and interaction (`"<trigger> <effect>" --domain gsap` or `"animation <concern>" --domain ux`;
   `references/app-motion.md` section 1). Their words seed the survey and library searches.
2. Survey: 8+ shipped products of the category and its neighbours (web search: galleries, the products' own sites,
   release notes, docs with screenshots; `references/web-research.md` lists where), on 5+ different sites, plus
   1+ studied site from the library (`library.mjs search "<category words>" --kind sites`). Per reference: its
   URL and a `take:` line, the one concrete thing it gives THIS product (a layout, a control, a way to show state).
3. Library: `library.mjs search "<product, audience, mood words>"` per kind; pick and name by id (`lib:<kind>/<id>`)
   a direction, a palette, a type system and, for app screens, an app style, shell and the screen patterns (pages:
   heroes / sections). `library.mjs show <kind> <id>` gives the full entry: its blueprint is what you draw from.
4. Brands: read `brands/_index.md`, then ONE `brands/<name>.md` whose language fits (what you borrow, what you do
   not). Or write `none: <reason>`.
5. Screens: one `###` per key screen with `question:` (what the screen answers for its user), `signature:` (the one
   element that makes it this product's screen), `components:` (each from the library, `lib:` ids, or named) and
   `states:` (what its empty, loading and error states show; each one is drawn, not only described) and
   `interactions:` (what moves on it and why, `trigger -> effect`: the press, the loading -> result change, how it
   is entered and left, a gesture; `references/app-motion.md` sections 4-7; they become rows of the direction's
   Motion & Interaction table).
6. Elements (function before form, boss 2026-11-10): under each screen, `elements:` lists EVERY element you will draw
   on it (button, field, card, chart, stat, tab, filter, badge, menu or nav item, icon button, panel), one line each:
   `- <element>: job: <what the user does or learns with it> | source: <what backs it>`. The source is a real thing in
   this project: a feature in `BRIEF.md`, a module or acceptance criterion in `MODULES.md` (`M3`), or a field,
   endpoint or event in `.brain/contracts/` (`contracts/invoice.md#total`). An element with no job or no source is
   not drawn. Do not invent it to fill space. A purely visual element (a texture, a divider, an illustration)
   still writes its job and uses `source: decoration (<why it earns its place>)`. A signature that shows data or does
   something is not decoration: it needs a real source like any other element. Indent a wrapped line. An element shown only in
   one state gets its own line with the state first: `- (empty) import button: job: ... | source: ...`. A description, subtitle or helper line is an element
   too: its job is what it tells the user that the label cannot (a consequence, limit, state or error), never a
   repeat of the label (design SKILL.md "No text that repeats or narrates").

## Function before form: the questions every element must pass
Answer these before you draw an element. One "no" or "I do not know" means it is cut, or it becomes a question for
the dev to settle first. It is never drawn as a guess.
1. What does the user DO or LEARN with it? Name one verb and one object: "filter invoices by status", "see today's
   unpaid total". "Looks modern" or "fills the header" is not a job.
2. Where does its content come from? Real data the app has (a contract field or endpoint), or a BRIEF feature. A
   number, chart, avatar, notification count or activity feed with no source is invented data. Invented data is
   not drawn.
3. What happens when it is used? A button or link leads to a screen, dialog or action that exists in the brief or a
   contract. A control that goes nowhere is a fake control. Do not draw one.
4. Is it on the right screen? Every nav item, tab and menu entry opens a screen in this pack or the brief. Never
   draw "Settings", "Help", "Reports" or "Integrations" because apps usually have them.
5. Does it hold real content in every state? Empty, loading and error say what is missing and what to do next.
   They are never a blank card or a shimmering box with nothing behind it.

## Template
```markdown
# Research pack: <product>
## UX rules
- search.py "report designer finance staff" --design-system -> navy/grey, trust blue, accent only on the main action (its landing "Pattern" not used: app screens); applies to: palette
- search.py "inline validation near field" --domain ux -> errors next to the field, on blur; applies to: Settings
- search.py "skeleton loading" --domain gsap -> shimmer on transform, stop off-screen and under reduced motion; applies to: Template editor loading
- ... (3+, 1+ on motion)
## Survey
- Rows Report Designer https://www.example.com/designer take: band layout with a live preview strip under the canvas
- ... (8+)
- lib:sites/<id> take: ...
## Library
- direction lib:directions/<id>: <why>
- palette lib:palettes/<id> (light + dark: the mode the entry lacks is derived from it, colour.md section 6), type lib:type/<id>
- app style lib:app-styles/<id>, shell lib:app-shells/<id>, patterns lib:app-patterns/<id>, lib:app-patterns/<id>
## Brands
- brands/<name>.md: borrow <..>; not <..>
## Screens
### Template editor
question: where does this field come from and how will it print?
signature: <the one element>
components: lib:app-patterns/builder-canvas, lib:app-shells/<id>, property inspector, ...
states: empty = <what shows>; loading = <...>; error = <...>
interactions: field dropped -> settles on the band grid (spring); export -> button spinner, then the preview strip slides up; open from the list -> container transform
elements:
- field list: job: pick the data field to place on the band | source: contracts/template.md#fields
- band canvas: job: place and move fields where they will print | source: M2 acceptance "drag a field onto a band"
- print preview strip: job: see the printed page before export | source: BRIEF "preview before printing"
- grid dots: job: align fields by eye | source: decoration (the canvas signature, helps placement)
```
