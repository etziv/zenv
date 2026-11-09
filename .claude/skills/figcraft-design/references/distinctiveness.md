# Distinctiveness: templates, fonts, colour, ban list, review

Detail for SKILL.md §2 (brief, plan, directions), §3 (tells), §5 (anchor) and §7 (review). Code-side
libraries, motion and screenshot loops live in `design-to-code.md` §11.

## Contents
1. DESIGN.md template (optional persisted brief + plan)
2. Metaphor and constraint menus
3. Direction card and one-direction subagent brief
4. Fonts: sources, licences, banned defaults, pairing
5. Colour in OKLCH and the hex helper
6. Extended ban list
7. Consistency audit in FigCraft (manual)
8. Adversarial art-director review

## 1. DESIGN.md template (optional persisted brief + plan)

When to write it: SKILL.md §2. Put it at the project root, or next to the `.figcraft` file when there is no code
project. Do not edit the project's `CLAUDE.md`/`AGENTS.md` unless asked.

```markdown
# Design: <product / page>
Date: <…> · Direction chosen: <name> (+ hybrid from <…>)

## Brief
- Users and context: <who, device, operational | showcase | hybrid>
- Primary job: <…>
- The one thing: <one sentence a viewer remembers>
- Tone: <word>, <word>, <word>
- Non-negotiable conventions: <navigation, primary actions, form behaviour, states, keyboard paths>
- Metaphor: <world outside software> → structure <…>; type <…>; interaction <…>; copy <…>
- References: <link or file> → takes <grid / size contrast / number treatment>, leaves <…>
- Arbitrary constraints: 1. <…> 2. <…>

## Type
| Role | Family | Source and licence | Weights | Notes |
|---|---|---|---|---|
| Display | <…> | <…> | <…> | <tracking at large sizes> |
| Text | <…> | <…> | <…> | <tnum, ss01…> |
Scale (ratio <…>): <…>; line height body <…>, display <…>.

## Colour (OKLCH is the source; hex for FigCraft)
| Token | OKLCH | Hex | Role | Contrast |
|---|---|---|---|---|
| color/bg | oklch(…) | #… | background | n/a |
| color/fg | oklch(…) | #… | text | ≥ 4.5:1 |
| color/accent | oklch(…) | #… | one job only: <…> | ≥ 3:1 as UI |
Dark mode: <values or "not in scope">.

## Space, shape, density
Spacing <…>; radius <…>; strokes <…>; elevation <…>; density <generous | dense>.

## Anchor component and signature interaction
<component name and id; what derived components inherit> · <interaction: where, duration, easing,
reduced-motion equivalent>

## Bans and exceptions
Project-specific bans <…>; justified exceptions to the §3 tells or §6 below <…>.

## Assets and voice
<asset master files and exports> · <three sample sentences; words we use; words we never use>
```

## 2. Metaphor and constraint menus

Starting points only; pick something from the subject's world.

**Metaphors**: nautical chart, ship's logbook, archive card catalogue, library due-date card, musical
score, technical field manual, telegram, railway timetable, synthesiser or mixing-desk panel, lab
notebook, seed packet, herbarium sheet, darkroom contact sheet, air-traffic strip board, ticket stub,
weather-station readout, tailor's pattern, carbon copy, museum label, packing list.

Translate each into four layers:

| Layer | Question | Example (railway timetable) |
|---|---|---|
| Structure | what organises the information? | rows of departures, fixed columns, platform as the key |
| Type | how is text set? | condensed sans, tabular figures, size jumps by role |
| Interaction | how do things change? | split-flap style updates for changing values |
| Copy | what words does this world use? | "departs", "delayed", "on time", "platform" |

**Arbitrary constraints**: palette sampled from one non-digital photo (a fruit stall, rusted metal,
a faded book cover); odd column grid (5, 7, 9) broken on purpose once per screen; two weights only,
sizes on a 1:3 ratio; radius 0 everywhere; no shadows, depth only from colour and overlap; text
always dominant over images, images never full-bleed; one colour per screen plus black and the
background; all numbers in tabular figures, data larger than labels; every section starts with a
verb; borders only as 2 px rules, never boxes.

## 3. Direction card and one-direction subagent brief

```markdown
### Direction B: <name>
- Recognisable for: <three short lines>
- Metaphor reading: <how it shows>
- Axes: layout <…> · type <display / text> · palette <dominant / accent> · interaction <…>
- Risk: <legibility, density, build cost>
- Screens: <absolute PNG paths>; FigCraft page "Direction B - <name>"
```

Directions in FigCraft run one at a time (SKILL.md §2). When a subagent builds one, give it the brief,
not your reasoning about the other directions, so it does not drift toward them:

```text
Load the figcraft-design skill. Build Direction <X> of <N> for <product>. Brief: <path or text>.
Your axes (must not match the other directions): layout <…>; type <display / text>, check with
text.fonts; palette dominant <…>, accent <…>; interaction <…>. Constraints: <…>.
Work only on page "Direction <X> - <name>" (id <…>), pass explicit parent ids, create variable
collection "Dir <X>", frames "<X> / Desktop" and "<X> / Mobile" with the anchor screen and real copy.
Verify with get_node and screenshot. Do not touch other pages or open other files.
Return frame ids, screenshot paths, the direction card and open issues. User approval is pending;
do not ask the user anything.
```

## 4. Fonts: sources, licences, banned defaults, pairing

Licence terms change: read each family's licence at download time and record it in DESIGN.md. The
notes below are general terms at the time of writing, not legal advice.

| Source | What you find | Licence notes (verify) |
|---|---|---|
| Fontshare (Indian Type Foundry) | contemporary grotesques, display and text families | free for personal and commercial use under ITF's own free font licence |
| Velvetyne | experimental and display libre fonts | mostly SIL Open Font License |
| Collletttivo | Italian collective, display and text faces | mostly SIL Open Font License |
| Open Foundry, The League of Moveable Type | curated libre families | SIL Open Font License |
| Use & Modification | curated list of libre fonts | mixed: check each |
| Pangram Pangram | strong display and text families | free downloads for personal use and trials only; commercial use needs a paid licence |
| Google Fonts | large libre catalogue | OFL or Apache; pick beyond the top of the popularity list |
| Commercial foundries | when the user buys a licence | per-licence limits on web views, apps, seats |

Only use fonts the user can ship; if the right face needs a paid licence, say so and offer a libre
alternative.

**Banned as a default**: the faces listed in SKILL.md §2 (Type). Fine when written into the plan: the brand or design system already uses it; a native operational app where the
platform face is the convention (SF on iOS, Roboto on Android), with distinctiveness moved to scale,
weight contrast, composition and colour; a deliberate constraint that uses the default on purpose.

**Pairing**:
1. Start from the metaphor: what would be printed or engraved in that world (a timetable's condensed
   grotesque, a score's engraved serif, a manual's sturdy sans)?
2. Display carries personality, text carries reading; contrast them in structure (serif vs sans,
   condensed vs wide, geometric vs humanist), not near-twins.
3. Text face: generous x-height at 14–16 px, clear `Il1` and `0O`, tabular figures for data, the
   languages the product needs, enough weights.
4. Display face: holds at its largest size on a 390 px screen; tracking tightened at large sizes.
5. One family is enough only with real range (width or optical-size axes).

In FigCraft, `text.fonts {family}` checks a family and `text.missingFonts` lists fallbacks (silent,
so screenshots can lie). Installing a font changes the user's system: ask first, then
`text.rescanFonts`. Fonts in code: `design-to-code.md` §11.

## 5. Colour in OKLCH and the hex helper

`oklch(L C H)` (lightness 0–1, chroma 0–~0.37, hue 0–360) keeps perceived lightness steady across
hues, so scales built by stepping L look even.

- **Dominant**: one hue that owns large areas. **Accent**: one sharp, high-chroma hue with a single
  job (primary action, live data, selection).
- **Neutrals**: the dominant hue at very low chroma (C 0.005–0.02), stepped in L.
- **States**: hover/pressed move L by a fixed step (e.g. ±0.05), not new colours.
- **Dark mode**: re-pick L per token (backgrounds around L 0.15–0.22, lower chroma on large fields).
- **Gamut**: high chroma at some hues leaves sRGB and gets clipped; the helper flags it: lower C.
- Contrast per SKILL.md §3 (AA).

Pipeline: tokens in OKLCH in DESIGN.md → hex via the helper → FigCraft variables with the same slash
names → `variable.export {format:"css"}` gives the names; in code write the OKLCH values under those
names (hex fallback only if the browser targets need it). Change DESIGN.md first, then the variable,
then the code token.

Helpers: plain Node, no dependencies; run them from the session's scratch folder, never the user's
project, and delete them afterwards. Tested: sRGB primaries round-trip exactly (`0.627955 0.257683
29.234` → `#FF0000`), out-of-gamut input is flagged; the contrast helper gives 21.00:1 for black on
white.

```js
// oklch-to-hex.mjs: node oklch-to-hex.mjs "0.62 0.19 35" "0.97 0.01 90"
const toHex = (L, C, H) => {
  const h = (H * Math.PI) / 180, a = C * Math.cos(h), b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3;
  const lin = [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
  ];
  const out = lin.some((v) => v < -1e-4 || v > 1 + 1e-4);
  const hex = lin.map((v) => {
    v = Math.min(1, Math.max(0, v));
    v = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
    return Math.round(v * 255).toString(16).padStart(2, '0');
  }).join('');
  return `#${hex.toUpperCase()}${out ? '  (out of sRGB gamut: clipped, lower C)' : ''}`;
};
for (const arg of process.argv.slice(2)) {
  const [L, C, H] = arg.split(/[\s,]+/).map(Number);
  console.log(`oklch(${L} ${C} ${H}) -> ${toHex(L, C, H)}`);
}
```

```js
// contrast.mjs: node contrast.mjs "#1B1D21" "#FBFAF7"   -> WCAG 2 contrast ratio
const lum = (hex) => {
  const c = hex.replace('#', '').match(/../g).map((x) => parseInt(x, 16) / 255)
    .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const [a, b] = process.argv.slice(2).map(lum).sort((x, y) => y - x);
console.log(`${((a + 0.05) / (b + 0.05)).toFixed(2)}:1`);
```

## 6. Extended ban list

Defaults to avoid, not laws: the user's direction, an existing brand or system, or a written
exception in the plan overrides any item. This extends SKILL.md §3 "Avoid default tells" (those stay
in force and are not repeated here); the point is that none appear by reflex.

- **Visual**: a timid palette of five near-identical tints with no dominant and no sharp accent;
  stock photos, generic 3D renders, "flat people" illustrations, placeholder images left in.
- **Type**: a single family in a single weight doing every job; display sizes with default tracking
  and line height.
- **Icons**: icons on every heading or list item by habit. An existing icon set is fine once its
  stroke, size and corner logic are tuned to the anchor.
- **Copy**: "Supercharge your…", "Effortless", "Revolutionise", "All-in-one platform", "Built for the
  modern…"; headlines that could fit any product in the category; "Success!" as a state.
- **Motion**: parallax or scroll-jacking without a reason. Code-side motion rules (reduced motion,
  what to animate) are in `design-to-code.md` §11.
- **Libraries**: a component still recognisable as "the one from library X" (rules in
  `design-to-code.md` §11).

## 7. Consistency audit in FigCraft (manual)

There is no lint command for tokens; audit with the inspect tools after the §7 loop, per screen and
on the Components page.

1. `variable.list` and `style.list`: the allowed values (they must match the plan or DESIGN.md).
2. Per frame, `get_node {id, depth:3}` (section by section on big trees; never audit a `truncated`
   response). Flag: fills/strokes with a hex and no `boundVariables` entry (unbound colour); `gap`,
   `crossGap`, `padding` off the spacing scale; `cornerRadius` off the radius scale; text outside the
   type plan or without a text style; instance overrides that recreate a missing variant.
3. `inspect.properties {id}` on key layers confirms a value is a token, not a lookalike hex.
4. `text.missingFonts`: any fallback invalidates the screenshots.
5. Fix by binding (`variable.bind`), applying styles, or adding a genuinely new value to the
   variables first. Report as a short table: layer id, issue, fix.

## 8. Adversarial art-director review

Run it on the anchor (recommended) and on the final screens (by default, before SKILL.md §9).

- A **separate agent with a clean context**: a fresh subagent, not a fork of your conversation. It
  must not see your reasoning, your own critique or the rejected directions.
- Give it only the screenshot paths (desktop and mobile), the brief/plan or DESIGN.md, and the bans
  (SKILL.md §3 tells and §6 above). Do not say what you think is good or ask it to fix anything.

```text
You are a severe art director reviewing a UI before launch. You did not make it and you owe its
authors nothing. Read the brief and design plan, and the list of banned defaults, then look at
every screenshot (<paths>).

List everything that looks generated by an AI or taken from a template or UI kit, and everything
that contradicts the brief, the plan or the bans. Order by severity:
- SEVERE: a viewer would call it generic or "AI-made"; the one memorable thing is not visible; a
  banned default; an accessibility failure (contrast, missing focus, unreadable text).
- MAJOR: inconsistency with the anchor component or the scales; a library component left as shipped;
  copy that could belong to any product; weak hierarchy.
- MINOR: polish.
For each item: screenshot, location, what is wrong, why it reads as generic, the direction of a fix
(one line). No compliments, no summary of strengths, no redesign.
End with one line: would this be recognisable with the logo removed? yes/no and why.
```

Triage: fix every SEVERE item; fix MAJOR items unless they conflict with the brief or a user decision
(say which and why). Do not debate the reviewer; report disagreements to the user. After large fixes,
re-screenshot and run a new clean reviewer, not the same one. Fixes that change the system go through
the variables (and DESIGN.md) first, then through the §9 loop if the user already approved an earlier
version.
