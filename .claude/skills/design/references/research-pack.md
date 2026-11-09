# The research pack (`.brain/design/proposal/research.md`)

The look of a product comes from what was surveyed, what the library already knows and the rules, never from taste.
This file is the proof, written BEFORE any screen is drawn, and the brief every later drawer reads (a designer
re-spawned for FigCraft, a worker, the `frontend` build). `node .claude/skills/design/scripts/library.mjs check
.brain/design/proposal/research.md` (add `--app` for app screens) must print `research pack complete`.

## How to fill it
1. Survey: 8+ shipped products of the category and its neighbours (web search: galleries, the products' own sites,
   release notes, docs with screenshots; `references/web-research.md` lists where), on 5+ different sites, plus
   1+ studied site from the library (`library.mjs search "<category words>" --kind sites`). Per reference: its
   URL and a `take:` line, the one concrete thing it gives THIS product (a layout, a control, a way to show state).
2. Library: `library.mjs search "<product, audience, mood words>"` per kind; pick and name by id (`lib:<kind>/<id>`)
   a direction, a palette, a type system and, for app screens, an app style, shell and the screen patterns (pages:
   heroes / sections). `library.mjs show <kind> <id>` gives the full entry: its blueprint is what you draw from.
3. Brands: read `brands/_index.md`, then ONE `brands/<name>.md` whose language fits (what you borrow, what you do
   not). Or write `none: <reason>`.
4. UX rules: 3+ catalogue queries (`python -B .claude/skills/design/uiux/scripts/search.py "<concern>" --domain ux`,
   `--stack <stack>`), each with the rule it gave and where it applies.
5. Screens: one `###` per key screen with `question:` (what the screen answers for its user), `signature:` (the one
   element that makes it this product's screen), `components:` (each from the library, `lib:` ids, or named) and
   `states:` (what its empty, loading and error states show; each one is drawn, not only described).

## Template
```markdown
# Research pack: <product>
## Survey
- Rows Report Designer https://www.example.com/designer take: band layout with a live preview strip under the canvas
- ... (8+)
- lib:sites/<id> take: ...
## Library
- direction lib:directions/<id>: <why>
- palette lib:palettes/<id>, type lib:type/<id>
- app style lib:app-styles/<id>, shell lib:app-shells/<id>, patterns lib:app-patterns/<id>, lib:app-patterns/<id>
## Brands
- brands/<name>.md: borrow <..>; not <..>
## UX rules
- search.py "inline validation near field" --domain ux -> errors next to the field, on blur; applies to: Settings
- ... (3+)
## Screens
### Template editor
question: where does this field come from and how will it print?
signature: <the one element>
components: lib:app-patterns/builder-canvas, lib:app-shells/<id>, property inspector, ...
states: empty = <what shows>; loading = <...>; error = <...>
```
