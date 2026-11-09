# FigCraft workflows

These recipes extend the four prompts that ship with the FigCraft MCP server
(`crates/mcp/src/prompts.rs`: `mobile_screen`, `design_system`, `landing_page`, `prototype_flow`,
Apache-2.0). They keep the original tool sequence and add the quality bar from SKILL.md. The
default colours in the built-in prompts (#3B5BDB blue, #7048E8 violet) are placeholders: derive
real ones from the brief.

Every recipe ends with the same verify step (from the built-in prompts): after each major step
call `get_node` on the frame (or `inspect_document`) and check sizes, fills, layout and text; at
the end call `screenshot` and look at it; fix anything off; report the ids of the frames made.

## Contents
1. Mobile screen
2. Design system starter
3. Landing page
4. Prototype flow
5. Dashboard (bonus)
6. Restyling a template

---

## 1. Mobile screen (`mobile_screen`: screen, device = "iPhone 16", brand_color)

1. Plan: what the screen's one job is, primary action, content list, token set (or reuse the
   file's variables/styles).
2. Frame: `run_command frame.create {"preset":"iPhone 16","x":0,"y":0,"name":"<Screen>"}` with a
   background (bind `color/bg` if variables exist).
3. Layout: `set_properties` on the frame
   `{"layout":"vertical","padding":[64,24,32,24],"gap":16,"sizingV":"fixed"}` (top padding clears
   the status bar; adjust gap per section rather than one gap for everything: wrap groups in
   nested vertical frames with their own gap).
4. Content with `create_node` / `batch` (parent = frame):
   - title: text, ~28 px, bold or the display face;
   - supporting text: 16 px, muted colour, `sizingH:"fill"` so it wraps;
   - inputs / list rows: horizontal frames, padding 16, cornerRadius from the radius scale, 1 px
     stroke in a border token, `sizingH:"fill"`; labels above inputs, helper/error text below;
   - primary button: horizontal frame, `mainAlign:"center"`, padding `[14,24,14,24]`, radius,
     brand fill, `sizingH:"fill"`, white bold label; min height 44–48.
   Prefer instances of existing Button/Input components.
5. 8-pt spacing, left-aligned text, one primary action, real copy.
6. Verify (numbers + screenshot). Check text contrast and that nothing clips at the bottom.

Example batch (one undo step):

```json
{"label":"Sign-in screen","commands":[
 {"command":"frame.create","params":{"preset":"iPhone 16","x":0,"y":0,"name":"Sign in",
   "props":{"fill":"#FFFFFF","layout":"vertical","padding":[64,24,32,24],"gap":16}}},
 {"command":"node.create","params":{"type":"text","parent":{"$step":0},"text":"Welcome back",
   "props":{"fontSize":28,"fontStyle":"Bold"}}},
 {"command":"node.create","params":{"type":"text","parent":{"$step":0},
   "text":"Sign in to see today's routes.","props":{"fontSize":16,"fill":"#5B6168","sizingH":"fill"}}},
 {"command":"node.create","params":{"type":"frame","parent":{"$step":0},"name":"Button",
   "props":{"layout":"horizontal","mainAlign":"center","padding":[14,24,14,24],"cornerRadius":12,
            "fill":"#1F5C4A","sizingH":"fill","sizingV":"hug"}}},
 {"command":"node.create","params":{"type":"text","parent":{"$step":3},"text":"Sign in",
   "props":{"fontSize":16,"fontStyle":"Bold","fill":"#FFFFFF"}}}]}
```

## 2. Design system starter (`design_system`: name = "Core", primary)

Scope first: tokens only, one component family, or a full library. Do only the agreed scope.

1. Variables: `variable.createCollection {"name":"<Name> colours","modes":["Light","Dark"]}`,
   then `variable.create` for `color/bg`, `color/fg`, `color/muted`, `color/border`,
   `color/primary`, `color/on-primary` (and status colours if needed) with
   `{"values":{"Light":…,"Dark":…}}`. Number collection for `space/*`, `radius/*`.
   Primitives + semantic aliases (`variable.set {value:{alias}}`) for larger systems.
   Set `scopes` and `codeSyntax` if the codebase has token names.
2. Paint styles (optional when variables cover colour): a swatch rectangle per colour, then
   `style.create {"type":"paint","name":"<Name>/Primary","from":<id>}`; bind the style's fill to
   the variable with `style.bind`.
3. Text styles from real text layers: e.g. `Display` 48, `Heading` 32 Bold, `Title` 20,
   `Body` 16 Regular, `Caption` 12; `style.create {"type":"text","name":"Text/Heading","from":<id>}`.
   Use the chosen families, line heights and tracking from the plan.
4. Effect styles: 1–2 elevations at most (`style.create {"type":"effect", ...}`).
5. Button component: frame with auto layout (horizontal, padding `[12,20,12,20]`,
   `mainAlign:"center"`, radius) containing a text "Button"; bind fill to `color/primary`, text
   fill to `color/on-primary`; `component.create`.
6. Variants: `component.addVariant` → rename `Property 1` to `Variant`, values `Primary` /
   `Secondary` (outline: transparent fill, primary stroke **and** primary text colour); add a
   `State` axis (`component.addProperty {type:"variant", name:"State", default:"Default"}`, then
   `component.addVariant {property:"State", value:"Hover"|"Pressed"|"Disabled"}`) as needed.
   Optional `text` property for the label (`component.addProperty {type:"text", layer:<label>}`)
   and a `boolean` for an icon.
7. Check modes: put an instance in a frame, `variable.setMode {ids:[frame], collection,
   mode:"Dark"}`, screenshot, then reset.
8. Optional specimen page: colour swatches with names and hex, type ramp, components grid.
9. `list_commands {filter:"style"}` / `{filter:"variable"}` for exact params. Verify.

## 3. Landing page (`landing_page`: product, width = 1440, accent)

1. Plan the page narrative from the product: what the hero must show (the most characteristic
   thing in the product's world), proof, how it works, call to action. Don't default to "big
   number + gradient" heroes or three identical feature cards unless they're the right answer.
2. Page frame: `create_node {"type":"frame","x":0,"y":0,"width":1440,"height":2400,"name":"Landing",
   "props":{"fill":"#FFFFFF","layout":"vertical","gap":0}}`.
3. Sections are child frames with `sizingH:"fill"` and their own auto layout:
   - Nav: horizontal, `mainAlign:"spaceBetween"`, padding `[20,80,20,80]`, `crossAlign:"center"`.
   - Hero: vertical (or horizontal split), padding ~120, gap 24: display headline (56–72 px),
     subhead (~20 px, max ~60 characters per line via `maxWidth`), primary CTA in the accent.
   - Content sections: vary the rhythm (split layouts, a grid, a full-bleed band) instead of
     repeating one card row; grid layout via `layout:"grid"`, `columnTracks:["1fr","1fr","1fr"]`.
   - CTA band in the accent with text that passes contrast.
   - Footer: padding 40, small muted text, real links.
4. Use `batch` for repeated styling across ids (one undo step).
5. Finish with `sizingV:"hug"` on the page frame so it fits its content.
6. Responsive (if code will follow): duplicate the page at 390 wide with the **same layer names**
   and a mobile layout, so code generation can pair them.
7. Verify: screenshot the whole page, then each section at scale 1 for detail.

## 4. Prototype flow (`prototype_flow`: screens = "Home, Details, Checkout", transition = smartAnimate)

1. If the frames don't exist, create one phone frame per screen
   (`frame.create {"preset":"iPhone 16", ...}`) side by side 100 units apart, each with a title and
   a "Next" button (frame with text inside). Prefer real screens with real content.
2. For each screen except the last, add an interaction on its button:
   `proto.interaction.add {"id":<button>,"trigger":{"type":"onClick"},"action":{"type":"navigate",
   "dest":<next frame>},"transition":{"type":"smartAnimate"},"durationMs":300}`.
   Add a back button on later screens with `{"action":{"type":"back"}}`.
3. `proto.flow.add {"start":<first frame>,"name":"Main flow"}`; optionally
   `proto.settings {"device":"iPhone 16"}`.
4. Check `get_node` on each button shows `interactions`, and `inspect_document` shows the page's
   `flows`. Verify.
5. Polish: hover/pressed via component variants + `changeTo`; overlays (`openOverlay` with
   `position`, `close_on_outside`); Smart Animate needs matching layer names across screens.

## 5. Dashboard (bonus)

- Start from `file.newFromTemplate {"template":"dashboard"}` only if the user is fine with a
  starter; otherwise: Desktop 1440×1024 frame, sidebar (fixed width, vertical) + content
  (`sizingH:"fill"`, vertical), header row, KPI row, a grid of panels (`layout:"grid"`).
- Data density: tabular numerals (`features {"tnum": true}`), right-aligned numbers, muted grid
  lines, one accent for the series that matters; charts drawn as vectors (`create_node
  {"type":"vector","path":"M…"}`) or simple bars (rectangles in a horizontal auto layout).
- Realistic data and labels for the domain; no lorem ipsum.

## 6. Restyling a template

`template.list` → `file.newFromTemplate {template}` → `variable.list` / `style.list` /
`component.list` → change the tokens (variables and styles) first so every instance updates, then
adjust composition and copy. Verify with screenshots; the result should not read as the template.
