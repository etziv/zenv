# FigCraft design → frontend code

Goal: production code in the user's stack that matches the FigCraft design 1:1 visually, reuses
the project's components and tokens, and is accessible and responsive. FigCraft's generated code
is a precise **spec**, not the final code style.

## Contents
1. Inputs and target
2. Read the design
3. Visual reference
4. Tokens → code
5. Assets
6. Generated code and how to translate it
7. Mapping table: FigCraft → CSS / Tailwind
8. Components and Code Connect
9. Validate
10. Project rules file (optional)
11. Distinctive build: libraries, assets, fonts, motion, screenshot loop

## 1. Inputs and target

- Identify the frame or component id: the user names it, or find it with
  `inspect_document {depth: 2}`, `component.list`, `inspect.readyForDev {status:"any"}`.
  If several candidates exist, ask.
- Read the project first: framework (React/Next, Vue, Svelte, Leptos, SwiftUI, Compose…), styling
  (Tailwind v3/v4, CSS modules, CSS-in-JS, plain CSS), existing component library (shadcn/ui,
  MUI, in-house), token files (`tokens.css`, `tailwind.config.*`, `@theme` blocks), icon and image
  conventions, path aliases.

## 2. Read the design

1. `get_node {id, depth: 3}` for structure: layer names, `layout` (mode, gap, padding, align),
   `sizing`, min/max, fills, strokes, radii, effects, text props, `boundVariables`, component
   links and overrides.
2. Large trees: read the top level at depth 1, then each section by id. Never implement from a
   truncated response (`truncated` key present).
3. `inspect.properties {id}` per key layer: values with **style and variable names** (the token to
   use in code).
4. Interactions: `interactions` on layers tell you what is clickable and where it goes; component
   variants (`State=Hover|Pressed|Disabled`) tell you which states to implement.
5. Annotations / dev resources: `annotation.list {id}`, `devResource.list {id}` hold designer notes.

## 3. Visual reference

`screenshot {ids:[id], scale: 2}` (and per section for detail). Keep it as the source of truth
for validation. Never ship the screenshot as an asset in code.

## 4. Tokens → code

- `variable.export {format:"css"}` returns custom properties: default mode on `:root`, other modes
  on `[data-<collection>="<mode>"]` (collection name lower-cased, e.g. `[data-color="Dark"]`).
  Names come from slash paths: `color/primary` → `--color-primary`.
- `variable.export {format:"json"}` (default) gives values per mode for custom pipelines
  (Style Dictionary, a TS theme object).
- Map into the project:
  - **Tailwind v4**: put tokens in `@theme { --color-primary: …; --radius-md: …; }` so utilities
    (`bg-primary`, `rounded-md`) exist; Dark mode via the data attribute selector or
    `@custom-variant dark (&:where([data-color=Dark] *))`.
  - **Tailwind v3**: `theme.extend.colors.primary = "var(--color-primary)"` with the CSS file.
  - **CSS modules / plain CSS**: import the token file once; use `var(--token)` everywhere.
  - Existing project tokens with the same value win; if values differ slightly, prefer the
    project token and note the deviation; if they differ materially, ask.
- If the design uses raw hex not bound to a variable, check whether it equals a token before
  hardcoding; otherwise add a named token rather than a magic number.

## 5. Assets

- `inspect.assets {ids:[id]}` lists what to hand over (`kind`: icon → SVG, image → PNG, or layers
  with export settings).
- Export into the project's asset folder: `file.exportLayers {ids:[id], assets:true, dir:"<abs
  path>"}` or per layer `export {ids:[n], format:"svg"|"png", scale:2, path:"<abs path>"}`. The
  server may confine writes to allowed roots; if refused, export without `path` (data inline) or
  ask the user to allow the folder.
- Use each asset exactly where the design places it, keep SVG viewBox/size, never redraw icons or
  swap in an icon package when the design provides the icon. Keep data-driven images (avatars,
  product photos from an API) dynamic.

## 6. Generated code and how to translate it

- `inspect.code {id, language:"react"}` → TSX component + CSS (variables as
  `var(--color-primary, #3B5BDB)`, variants as `[data-variant="Secondary"]` rules, props typed as
  unions). Also `css`, `html` (whole tree), `leptos`, `swiftui`, `compose`; `unit:"rem"` with
  `rootFontSize` for rem output. Check `list_commands {filter:"inspect.code"}`: some builds also
  offer `tailwind` (React + Tailwind v4, mobile-first with `mobile: <frame id>`) and `vue`.
- `inspect.componentCode {id}` → property list (name, param, type, default) for a component's API.
- `component.playground {id, properties}` → code + preview for a specific variant combo.

Translate, don't paste:
- Replace absolute sizes with fluid layout where the design intends it: `sizingH:"fill"` →
  `flex-1`/`w-full`; fixed-width frames that hold content → `max-w-[…] w-full`; a screen's fixed
  height → `min-h`.
- Turn `<div>` soup into semantic HTML: buttons are `<button>`, links `<a>`, inputs `<input>` with
  `<label>`, lists `<ul>`, headings in order.
- Reuse project components (Button, Input, Card) and pass variant props instead of re-styling.
- Implement states the design shows (hover, focus-visible, pressed, disabled, error, loading,
  empty) and interactions from the prototype (navigation, overlays → dialogs/popovers).
- Keep generated class names out of the codebase if the project uses another convention.

## 7. Mapping table: FigCraft → CSS / Tailwind

| FigCraft | CSS | Tailwind |
|---|---|---|
| `layout: horizontal` / `vertical` | `display:flex; flex-direction: row/column` | `flex` / `flex flex-col` |
| `layout: grid`, `columnTracks: ["1fr","1fr"]` | `display:grid; grid-template-columns: 1fr 1fr` | `grid grid-cols-2` |
| `gap: 16` | `gap:16px` | `gap-4` |
| `crossGap` (wrap) | `row-gap` | `gap-y-*` |
| `padding: [t,r,b,l]` | `padding: t r b l` | `pt-* pr-* pb-* pl-*` / `px-* py-*` |
| `mainAlign: min/center/max/spaceBetween/spaceAround/spaceEvenly` | `justify-content: flex-start/center/flex-end/space-between/space-around/space-evenly` | `justify-start/center/end/between/around/evenly` |
| `crossAlign: min/center/max/baseline` | `align-items: flex-start/center/flex-end/baseline` | `items-start/center/end/baseline` |
| `wrap: true` | `flex-wrap: wrap` | `flex-wrap` |
| `sizingH: fill` (in a row) | `flex:1 1 0; min-width:0` | `flex-1 min-w-0` |
| `sizingH: fill` (in a column) | `align-self:stretch` | `self-stretch` / `w-full` |
| `sizingH: hug` | `width:auto` (fit-content) | `w-fit` |
| `sizingH: fixed`, `width: 320` | `width:320px` | `w-80` / `w-[320px]` |
| `minWidth/maxWidth` | `min-width/max-width` | `min-w-[…]` / `max-w-[…]` |
| `absolute: true` + constraints | `position:absolute` + insets (parent `relative`) | `absolute top-… left-…` |
| `cornerRadius: 12` / `[tl,tr,br,bl]` | `border-radius` | `rounded-xl` / `rounded-[…]` |
| `strokes` + `strokeAlign: inside` | `box-shadow: inset 0 0 0 Wpx` or `border` with `box-sizing:border-box` | `border` / `ring-inset` |
| `strokeSides` | per-side border | `border-b` etc. |
| `dropShadow {offset:[x,y], radius, spread, color}` | `box-shadow: x y radius spread color` | `shadow-[…]` or a theme shadow token |
| `backgroundBlur {radius}` | `backdrop-filter: blur(radius/2 px)` (check visually) | `backdrop-blur-*` |
| `opacity` | `opacity` | `opacity-*` |
| `clip: true` | `overflow:hidden` | `overflow-hidden` |
| `fontSize/lineHeight/letterSpacing` | same (`"150%"` → `1.5`; `"2%"` → `0.02em`) | `text-*`, `leading-*`, `tracking-*` |
| `fontStyle: "Semi Bold"` | `font-weight:600` | `font-semibold` |
| `textAutoResize: truncate` / `maxLines` | `text-overflow:ellipsis` / `line-clamp` | `truncate` / `line-clamp-*` |
| `textCase: upper` | `text-transform:uppercase` | `uppercase` |
| Variable `color/primary` | `var(--color-primary)` | `bg-primary` (theme token) |
| Variable mode Dark | `[data-color="Dark"]` scope | `dark:` variant mapped to it |

Blur radius conventions differ between design tools and CSS: compare the rendered result rather
than trusting the number.

## 8. Components and Code Connect

- Before writing a component, search the repo for an equivalent (name, role, props). Extend it with
  a variant rather than duplicating.
- Link FigCraft components to code so future generated code calls the real component:
  `codeConnect.set {id:<component or set>, language:"react", importPath:"@/components/ui/button",
  name:"Button", template:"<Button variant={Variant}>{Label}</Button>"}` (placeholders are
  component property names; `{prop ? "a" : "b"}` for booleans). Check the returned `unknown` list
  for misspelled placeholders.
- Persist the links in the repo: `codeConnect.export {path:"<repo>/figcraft.connect.json"}`;
  reload later with `codeConnect.import {path}`.
- After linking, `inspect.code` on a screen emits `<Button …/>` calls with imports
  (`connected: true`).

## 9. Validate

Render the implementation (dev server + browser screenshot, or Storybook) and compare with the
FigCraft screenshot at the same width:

- [ ] Layout: spacing, alignment, sizes, wrapping, section order
- [ ] Typography: family, size, weight, line height, tracking, case
- [ ] Colour: exact token values, both modes if themed
- [ ] Radii, borders, shadows, blur
- [ ] Assets: every icon/image present, correct size and position, no placeholders left
- [ ] States: hover, focus-visible, active, disabled, error, loading, empty
- [ ] Responsive: behaves like the design's fill/hug/min/max rules; mobile frame if provided
- [ ] Accessibility: semantic elements, labels, alt text, AA contrast, keyboard path, reduced motion

Fix every in-scope mismatch; note (don't fix) unrelated pre-existing issues. Document intentional
deviations in a code comment or the PR description.

## 10. Project rules file (optional)

When the user wants a repeatable flow (the analogue of Figma's "create design system rules"),
write a short section into the project's `CLAUDE.md` / `AGENTS.md` (only if asked):

- where UI components, tokens and assets live; styling approach;
- "FigCraft flow: get_node → inspect.properties → screenshot → variable.export css → inspect.code
  react as spec → reuse components → validate against screenshot";
- token rules (never hardcode hex; spacing scale; radius scale), asset rules (export from
  FigCraft into `<dir>`; no new icon packages), Code Connect file location.

## 11. Distinctive build: libraries, assets, fonts, motion, screenshot loop

The design's distinctiveness (SKILL.md §2–3) has to survive implementation: the code carries the
plan's tokens, fonts and the one signature interaction, not a library's defaults.

**Component libraries are raw material, never the result.** The codebase's own components come
first (§8). Outside libraries:

| Kind | Examples | Use for | Never |
|---|---|---|---|
| Unstyled accessible primitives | Radix UI, React Aria, Ark UI, Headless UI | focus management, keyboard, ARIA, overlays, menus, comboboxes | keep their demo styling |
| Skeleton kit | shadcn/ui | file structure and variant plumbing | ship its default tokens, radii, shadows or neutral palette |
| Animated effects | Magic UI, Aceternity UI, React Bits, Motion Primitives | one effect that serves the signature interaction | paste a preset as is, stack several effects |

For every piece taken from outside: cite the source (code comment or PR description); check its
licence and dependencies before adding code (some collections mix permissive and restrictive terms
or keep components behind a paid tier) and ask before adding a package the project does not have;
rewrite it with the project's fonts, tokens, radius and stroke scales, spacing and motion timings. If
it is still recognisable as "the one from library X", it is not finished.

**SVG assets** (exported from FigCraft or made with `vectorcraft-illustration`): optimise with svgo
(`npx svgo in.svg -o out.svg`, or the project's bundler plugin); keep `viewBox`, and keep ids that
CSS or animation target (disable svgo's `cleanupIds` for those). Inline the SVG when it animates, is
styled from CSS or should inherit `currentColor`; otherwise reference the file. Decorative SVG gets
`aria-hidden="true"`, meaningful SVG a `<title>` or accessible label. Animated assets from
`effectcraft-motion`: Lottie (play with a Lottie player, show a still frame under reduced motion) or
WebM with alpha plus a poster image.

**Fonts**: self-host WOFF2 with `@font-face` and `font-display: swap`, subset to the needed scripts,
preload only the display face used above the fold; `font-variant-numeric: tabular-nums` where data
aligns. Colour tokens planned in OKLCH go into code as `oklch()` under the exported variable names
(`distinctiveness.md` §5).

**Motion**: CSS first; GSAP or Motion (Framer Motion) when sequences, timelines or gestures need
choreography; WebGL or canvas only when it carries the metaphor, lazy-loaded, paused off-screen, with
a static fallback. One orchestrated entrance beats many hovers. Animate `transform` and `opacity`, not
layout properties or large `box-shadow`/`filter` areas; nothing may block input. Modern CSS worth
reaching for, with `@supports` where support is partial: odd grid tracks and subgrid, container
queries, `clamp()` type, `text-wrap: balance`/`pretty`, `clip-path`, `@property`, variable-font axes,
view transitions, scroll-driven animations. Reduced motion is mandatory and keeps meaning (replace
movement with an instant change or a fade, never remove feedback):

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}
```

**Screenshot loop at 1440 and 390 px.** After each iteration, screenshot the running page at 1440 and
390 px wide, compare with the FigCraft reference (§9) and the plan, fix, repeat: at least 2–3 rounds,
stopping when a round finds nothing severe. Also capture hover and focus, an empty and an error state,
reduced motion and Dark mode if in scope. Playwright (its MCP server, or the CLI:
`npx playwright screenshot --viewport-size=1440,900 --full-page <url> <file>`; check
`npx playwright screenshot --help` for the installed version) is optional third-party tooling, not an
ArtCraft app: installing it or its browsers needs the user's agreement, and anything installed only
for the task is removed afterwards. Write screenshots to a scratch folder, not the repository. The
final screens then go to the adversarial review (SKILL.md §7).
