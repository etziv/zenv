# FigCraft gotchas

Observed on the installed server or documented in FigCraft's `docs/mcp.md`.

## Coordinates and layout
- `x`/`y` are relative to the **parent frame**; groups have no coordinate space (a layer in a group
  is placed in the group's frame). Use `absoluteBounds` for page positions.
- Inside an auto-layout frame, `x`/`y` are ignored: the flow positions children. To pin a badge or
  fixed header: `absolute: true` + `constraints`.
- `fill` on a child makes a hugging parent `fixed` on that axis; setting a parent to `hug` turns
  its filling children `fixed`. Decide sizing top-down.
- A layer moved out of auto layout keeps its size as `fixed`.
- Padding never lets a frame be smaller than the padding.
- Strokes inside an auto-layout frame take space (`strokesIncluded` defaults to true).
- Text created without `width` hugs a single line and can overflow; set `width` or
  `sizingH:"fill"` for paragraphs.
- `node.reparent`/`node.reorder` index counts **bottom first** (= first in auto-layout flow).

## Components
- A component set placed **inside** an auto-layout screen becomes a flow child and can overflow and
  get clipped (observed: a 385 px set inside a 345 px content area). Build mains on a separate
  page/area; place instances in screens.
- `component.create` on a `fill` child turned it `fixed` in the observed run: re-check sizing.
- `component.addVariant` without a property names variants `Property 1=Default`, `Property 1=Variant2`;
  `addVariant {property, value}` fails with "unknown variant property" until the property exists
  (`component.addProperty {type:"variant", name, default}`).
- Restyling a variant's fill only leaves the label colour unchanged (white text on a white outline
  button = invisible). Change every affected layer and re-screenshot.
- Instances refuse moving/resizing/adding layers inside them (outside slots). Edit the main, use a
  slot, or detach only if the user agrees.
- `edit.duplicate` of a main component creates an **instance**, not a new component.
- Swapping variants keeps overrides only where names and values match; name layers consistently.

## Styles and variables
- Any explicit edit to a bound field (fill, size, font size…) **detaches** the variable; editing a
  styled property unlinks the style. Re-bind or edit the token instead.
- In one call a style link wins over raw values it sets; apply deviations in a later call.
- `variable.export` formats on the tested build: `json`, `css` only (`tailwind` rejected).
- Variable ids look like `"VariableID:1"`, collections `"VariableCollectionId:1"`; names also work
  in most params.
- `variable.setMode {mode:null}` returns a frame to Auto (inherits from parent/default mode).

## Batch and history
- A `batch` is all-or-nothing: one bad step rolls back everything; the error names the step.
- `{"$step": n}` is zero-based and may only point to earlier steps.
- Each `run_command` mutation is its own undo step; `undo` after a batch removes the whole batch.

## Text and fonts
- Default family is Inter; missing families render in a fallback silently. Check
  `text.fonts {family}` and `text.missingFonts`; after installing fonts `text.rescanFonts`.
- `lineHeight` takes `"auto"`, px or `"150%"`; `letterSpacing` px or `"2%"`.
- `text.fit` needs a fixed-width text box.

## Inspect, screenshot, export
- `inspect_document` truncates large trees (`truncated` key): drill down with `get_node`.
- `screenshot` without `ids` renders the whole page; default max side 1600 px, so small details
  blur on big pages. Target ids and raise `scale`.
- `screenshot {window:true}` only works attached to a visible app window.
- `inspect.code` languages on the tested build: css, html, react, leptos, swiftui, compose.
- File paths for `export`, `save_file`, `image.place`, `variable.export {path}` may be confined to
  read/write roots configured on the server; a refusal is an ordinary error. Exports without
  `path` return data inline.

## Session hygiene
- The headless server starts with one untitled document; it isn't saved unless you call
  `save_file`/`file.save`. Don't save or overwrite user files unless asked.
- Don't use third-party logos, product icons or copyrighted imagery in designs; use neutral
  placeholders or user-provided assets.
