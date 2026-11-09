# FigCraft command catalog (curated)

Command ids and params below were read from `list_commands` on the installed `figcraft` MCP server
(October 2026). The server is the source of truth: if a call fails with "invalid parameters", run
`list_commands {filter:"<id>"}` and follow its param doc. `*` = undoable. Run with
`run_command {command, params}` or as steps of `batch`.

## Contents
- MCP tools
- Pages and frames
- Layers, layout, ordering
- Text
- Styles
- Variables
- Components and instances
- Effects, images, data
- Prototyping
- Dev Mode, inspect, code
- Export and files
- Templates

## MCP tools

| Tool | Use |
|---|---|
| `create_node {type, x, y, width, height, parent, index, name, text, path, props}` | frame, rectangle, ellipse, polygon, star, line, arrow, text, vector, section, component, slice. Returns `{id, node}`. Text hugs unless `width` is set (then it wraps). |
| `set_properties {ids, props}` | `node.set` on layers; returns updated layers. |
| `get_node {id, depth, offset?, maxBytes?}` | One subtree as JSON. |
| `inspect_document {depth, allPages, maxBytes}` | Whole document: tree, styles, selection, history. Truncates on big files. |
| `screenshot {ids?, scale?, maxSize?, window?}` | PNG of layers or the page (default max side 1600). |
| `batch {commands, label}` | One undo step, all-or-nothing; `{"$step": n}` = id of step n's result, `{"$step": n, "field": "ids/0"}` other fields. |
| `run_command {command, params}` / `list_commands {filter}` | Any command / discovery. |
| `export {ids, format, scale\|size, path?}` | png, jpg, svg, webp, pdf (and more); without `path` returns data inline. |
| `select {ids}`, `undo`, `redo` | Selection and history. |
| `open_file {path}` / `save_file {path?}` | `.figcraft`, `.fig` (best effort), `.sketch`, `.svg`, images, REST JSON. |
| `type_text {text, id?}` | Replace a text layer's contents (also inside instances = override). |
| `pointer`, `key`, `inspect_ui` | Gestures, shortcuts, app/tool state (mostly for attached sessions). |

## Pages and frames

- `page.add* {name?}`, `page.select {id|index}`, `page.rename* {id?, name}`, `page.delete*`,
  `page.duplicate*`, `page.move*`, `page.setBackground* {id?, color}`, `page.list`.
- `frame.create* {preset?, x?, y?, width?, height?, parent?, name?, props?}`: a preset without x/y
  lands right of the page's frames.
- `frame.presets`: Phone (iPhone 16 393×852, iPhone 16 Pro 402×874, iPhone 16 Pro Max 440×956,
  iPhone 16 Plus 430×932, iPhone SE 320×568, Android Compact 412×917, Android Medium 700×840),
  Tablet (iPad mini 8.3 744×1133, iPad Pro 11" 834×1194, iPad Pro 12.9" 1024×1366, Surface Pro 8,
  Android Expanded 1280×800), Desktop (MacBook Air 1280×832, MacBook Pro 14" 1512×982,
  MacBook Pro 16" 1728×1117, Desktop 1440×1024, Wireframe 1440×1024, TV 1280×720),
  Presentation (Slide 16:9 1920×1080, Slide 4:3), Watch, Paper (A4, A5, A6, Letter, Tabloid),
  Social (Instagram post 1080×1350, story 1080×1920, Facebook/X/LinkedIn posts, YouTube thumbnail,
  Dribbble shot).
- `frame.swapOrientation* {ids?}`.

## Layers, layout, ordering

`node.set*` props (the `set_properties`/`create_node` props) — most used:
- Geometry: `x`, `y` (parent-relative), `width`, `height`, `rotation`, `constrainProportions`,
  `constraints {horizontal, vertical}`.
- Paint: `fill`, `fills`, `stroke`, `strokes`, `strokeWeight`, `strokeAlign`
  (`inside|center|outside`), `strokeSides [t,r,b,l]`, per-side weights, `dashes`, `opacity` (0–1),
  `blend`, `cornerRadius` (n or `[tl,tr,br,bl]`), `cornerSmoothing` (0–1), `effects`, `clip`.
- Auto layout: `layout` (`null|horizontal|vertical|grid`), `gap`, `crossGap`, `padding`
  (n or `[t,r,b,l]`), `paddingTop/Right/Bottom/Left`, `mainAlign`
  (`min|center|max|spaceBetween|spaceAround|spaceEvenly`), `crossAlign`
  (`min|center|max|baseline`), `contentAlign`, `wrap`, `strokesIncluded`, `reverseZ`.
- Grid: `gridColumns`, `gridRows`, `columnGap`/`rowGap`, `columnTracks`/`rowTracks`
  (`[120, "1fr", "auto"]`), on children `gridColumn`, `gridRow`, `gridColumnSpan`,
  `gridRowSpan`, `gridAlignH/V`.
- Sizing: `sizingH`/`sizingV` (`fixed|hug|fill`), `minWidth`, `maxWidth`, `minHeight`,
  `maxHeight`, `absolute` (out of flow).
- Misc: `name`, `visible`, `locked`, `isMask`, `overflow`, `interactions`, `exports`, `grids`,
  `devStatus` (`none|readyForDev|completed`), `description`, `pluginData`.

Commands:
- `layout.add* {ids?, direction?}`, `layout.remove*`, `layout.suggest*` (infer auto layout from
  positioned children), `layout.setTrack* {axis, index, sizing, value?}`.
- `node.reparent* {ids?, parent, index?}`, `node.reorder* {id, index}` (index counts bottom
  first = first in an auto-layout flow).
- `align.left|right|top|bottom|...`, `distribute.horizontal|vertical`, `arrange.spacing {ids,
  horizontal?, vertical?}` for non-auto-layout siblings.
- `object.convertToGroup`, `object.convertToFrame`, `object.convertToSection`.
- `edit.duplicate {ids}` → `{ids}`, `edit.delete`, `edit.copy`, `edit.paste`.

## Text

- Props on `node.set`: `characters`, `fontFamily`, `fontStyle` ("Bold", "Semi Bold"…), `fontSize`,
  `lineHeight` (`auto|px|"150%"`), `letterSpacing` (`px|"2%"`), `textAlign`,
  `textAlignVertical`, `textAutoResize` (`widthAndHeight|height|none|truncate`), `textCase`,
  `textDecoration`, `paragraphSpacing`, `listStyle`, `maxLines`, `leadingTrim`
  (`none|capHeight`), `features {tnum: true}`, `fontVariations {wght: 550}`. With `start`/`end`
  they apply to a char range.
- `text.create* {text, x, y, parent?, width?, props?}`, `text.set* {id?, characters}`,
  `text.style* {id?, start?, end?, fontFamily?, fontStyle?, fontWeight?, fontSize?, lineHeight?,
  letterSpacing?, textFills?, hyperlink?, ...}`.
- `text.fit* {ids?, maxLines?, minSize?, orphans?}` (needs fixed width).
- `text.fonts {family?, style?, features?, axes?}`, `text.missingFonts`, `text.rescanFonts*`,
  `text.loadFont* {path|data}`, `text.refit*`.
- `text.maxLines* {lines}`, `text.case*`, `text.listStyle*`, `text.link* {url}`.

## Styles

- `style.create* {type: paint|text|effect|grid, name, from? (node id), description?, value?}`.
- `style.apply* {ids?, style (id or name), target? (fill|stroke), start?, end?}`.
- `style.update* {style, name?, description?, value?, from?}`, `style.detach* {ids?, which}`.
- `style.bind* {style, field, variable?}`: variable inside a style (`fill`, `fills/<i>/color`,
  `effects/<i>/radius`, text `fontSize|lineHeight|letterSpacing|fontFamily|fontStyle`).
- `style.list {type?}`, `style.delete*`, `style.duplicate*`, `style.move*`, `style.renameFolder*`.
- Names use `/` folders: `Text/Heading L`, `Color/Primary`, `Shadow/Raised`.

## Variables

- `variable.createCollection* {name, modes?}` → `{id}` (e.g. `"VariableCollectionId:1"`).
- `variable.create* {collection, name, type: color|number|string|boolean, value | values
  {modeName: value}}` → `{id}` (e.g. `"VariableID:1"`). Collections/variables accept id or name.
- `variable.set* {variable, mode?, value | {alias: variable}}`.
- `variable.bind* {ids?, field, variable}` (null unbinds). Fields: `fill`, `stroke`,
  `fills/<i>/color`, `strokes/<i>/color`, gradient stops, `effects/<i>/<prop>`, `width`,
  `height`, `opacity`, `cornerRadius`, `gap`, `paddingHorizontal`, `characters`, `visible`,
  `fontSize`, `componentProperties/<name>`…
- `variable.setMode* {ids?, collection, mode | null}` on frames/page.
- `variable.update* {variable, name?, description?, scopes?, hiddenFromPublishing?, codeSyntax?
  {WEB?, iOS?, ANDROID?}}`.
- `variable.addMode*`, `variable.renameMode*`, `variable.removeMode*`, `variable.moveMode*`,
  `variable.renameCollection*`, `variable.deleteCollection*`, `variable.delete*`,
  `variable.duplicate*`, `variable.list`.
- `variable.export {collections?, format? json|css, path?}`: CSS puts the default mode on `:root`
  and other modes on `[data-<collection>="<mode>"]`; `color/primary` → `--color-primary`.
  (Repo docs also mention `format: "tailwind"`; the tested build rejects it.)
- `variable.import* {variables}` (JSON as `variable.export` writes it, object/text/path).

## Components and instances

- `component.create* {ids?}`, `component.createMultiple* {ids?}`.
- `component.combineVariants* {ids?}` (names without `=` become `Property 1=Default`, …).
- `component.addVariant* {id?, property?, value?}` (property must already exist).
- `component.addProperty* {id?, name, type: boolean|text|instanceSwap|variant|slot, default?,
  layer?, preferred?}`, `component.editProperty* {id?, name, newName?, default?, value?,
  newValue?, index?, preferred?}`, `component.deleteProperty*`, `component.bindProperty* {layer?,
  property, field?: visible|characters|mainComponent|slot}`.
- `component.properties {id?}` (→ properties, options, defaults, `conflicts`), `component.list`.
- `component.makeSlot* {id?, name?, preferred?}`, `component.removeSlot*`,
  `component.exposeInstance*`.
- `component.playground {id?, properties?, language?, path?, scale?}`: try values on a copy,
  get code (+PNG with `path`). Document unchanged.
- `component.detach*`, `component.resetInstance*`, `component.resetOverride* {id?, group?}`,
  `component.pushChanges*`, `component.goToMain`, `component.restoreMain*`,
  `component.hideWhenPublishing*`.
- `instance.create* {component, x?, y?, parent?}`, `instance.place* {component, x, y}` (page
  point; inserts into auto layout by position), `instance.setProperties* {ids?, properties}`,
  `instance.swap* {ids?, component}`, `instance.swapAt*`, `instance.slotInsert* {id?, slot?,
  component, index?, replace?}`, `instance.resetSlot*`, `instance.exposed`.
- Libraries: `library.add {path}`, `library.list`, `library.place`, `library.import`,
  `library.applyStyle`, `library.bindVariable`, `library.publish`, `library.updates`,
  `library.acceptUpdates`.

## Effects, images, data

- `effect.add* {ids?, type? (dropShadow|innerShadow|layerBlur|backgroundBlur|noise|texture|glass),
  effect?}`, `effect.set* {ids?, index, effect}`, `effect.remove*`, `effect.move*`.
  Effect JSON example: `{"type":"dropShadow","color":"#00000026","offset":[0,4],"radius":16}`.
- `image.place* {data (base64) | path, x?, y?, width?, height?, parent?, mode? fill|fit|crop|tile}`;
  `image.adjust {ids?, preset?|filters?}`.
- `data.fill {id, rows, gap?, formats?, dir?, format?, scale?}`: repeat a frame per data row
  (keys = layer names; values text or `{text?, fill?, visible?, fit?}`), optional export per copy.

## Prototyping

- `proto.interaction.add* {id?, trigger?, action, actions?, transition?, durationMs?, easing?,
  delayMs?}` or `{id, dest}` for a simple click-navigate. Triggers: `onClick`, `onHover`,
  `onPress`, `onDrag`, `mouseEnter`, `mouseLeave`, `mouseDown`, `mouseUp`, `afterDelay {ms}`,
  `keyPress {key}`. Actions: `navigate {dest, preserveScroll?}`, `back`, `scrollTo`,
  `openOverlay {dest, position?, offset?, close_on_outside?, background?}`, `swapOverlay`,
  `closeOverlay`, `openUrl {url}`, `changeTo` (variant), `setVariable {variable, value |
  expression}`, `setVariableMode`, `conditional {blocks}`. Transitions: `instant`, `dissolve`,
  `smartAnimate`, `moveIn`, `moveOut`, `push`, `slideIn`, `slideOut` (+`direction`). Easing:
  `linear`, `easeIn`, `easeOut`, `easeInOut`, `easeInBack`, `easeOutBack`, `easeInOutBack`,
  `gentle`, `quick`, `bouncy`, `slow`, custom bezier/spring.
- `proto.interaction.set*`, `proto.interaction.remove* {id?, index}`, `proto.interaction.clear*`.
- `proto.flow.add* {start?, name?}`, `proto.flow.rename*`, `proto.flow.remove*`.
- `proto.settings* {device?, background?, rotation?, color?, width?, height?, responsive?}`.
- `proto.present`, `proto.preview`, `proto.closePreview`, `proto.presentOptions` (app UI).
- `proto.evaluate {expression | expr, id?}`; `proto.action.*`, `proto.conditional.*` for
  editing action lists.

## Dev Mode, inspect, code

- `inspect.code {id?, language? css|html|react|leptos|swiftui|compose, unit? px|rem,
  rootFontSize?}` → `{code, language, connected}`. React output = TSX + a CSS file with token
  vars and `data-*` variant rules. (Repo docs list `tailwind`, `vue`, `arkui` too: check the
  installed build.)
- `inspect.componentCode {id?, language?}` → props, definition, usage, `connected`.
- `inspect.properties {id?, colorFormat?}` → layout, auto layout, typography, fills, strokes,
  effects with style and variable names.
- `inspect.assets {ids?}` → `{assets: [{id, name, kind: export|icon|image, exports}]}`.
- `inspect.readyForDev {status?, page?}` → frames/sections by handoff status.
- `codeConnect.set* {id?, language?, importPath?, name?, template?}`, `codeConnect.remove*`,
  `codeConnect.export {path?}` (`figcraft.connect.json`), `codeConnect.import* {json|path}`.
- `annotation.add* {id?, label?, properties?, category?}`, `annotation.list`, `annotation.edit*`,
  `annotation.remove*`; `devResource.add {id?, url, name?}`; `devMode.addMeasurement`.
- `dev.compare {id?, version?}`: what changed since "ready for dev".

## Export and files

- `export.add* {ids?, setting?}`, `export.update*`, `export.remove*`: per-layer export settings.
- `file.exportLayers {ids?, assets?, dir?}`: every layer once per export setting, or with
  `assets: true` the Inspect assets (icons SVG, images PNG). Without `dir`, base64 data.
- `file.export {ids?, format?, scale?|size?, path?}`, `file.exportSvg`, `file.exportFramesPdf`.
- `file.save {path?}`, `file.saveAs {path}` (`.figcraft`; `.fig` is experimental).

## Templates

- `template.list` → `mobile-app` (4 trail-app screens, components, prototype), `landing-page`,
  `dashboard` (grid, vector charts, table), `design-system` (styles, Light/Dark variables,
  buttons, inputs, cards), `wireframe-kit`, `social-posts`, `slides`, `icon-set`.
- `file.newFromTemplate {template, name?}` opens a new unsaved document from one.
