# Figma-REST file JSON that FigCraft 0.15 opens (the subset figrest.py writes)

`open_file`, `file.open`, `file.import` and `figcraft-cli convert` read a Figma REST API file JSON
(`GET /v1/files/:key` shape). Proven on 2026-11-03 by importing and reading back with `get_node`.

## Top level
```json
{"name": "File name", "schemaVersion": 0,
 "components": {"1:10": {"key": "1:10", "name": "Button/Primary", "description": ""}},
 "document": {"id": "0:0", "type": "DOCUMENT", "name": "Document", "children": [
   {"id": "0:1", "type": "CANVAS", "name": "Page name", "backgroundColor": {"r":0.17,"g":0.17,"b":0.17,"a":1},
    "children": [ /* top-level frames */ ]}]}}
```
Ids are strings; any unique "a:b" works (FigCraft assigns its own integer ids on import). Colours are 0..1 floats.

## Every node
`id`, `type`, `name`, `absoluteBoundingBox {x, y, width, height}` in PAGE coordinates (figrest computes it from
relative x/y), `fills` / `strokes` as `[{"type":"SOLID","color":{r,g,b,a}}]` (empty list = none), `strokeWeight`,
`strokeAlign` (INSIDE|CENTER|OUTSIDE), `cornerRadius`, `effects` (DROP_SHADOW with color, offset {x,y}, radius,
visible, blendMode), `rotation` (degrees; unverified on import, check by screenshot).

## FRAME / COMPONENT / INSTANCE
`clipsContent`, `children`, and for auto layout: `layoutMode` HORIZONTAL|VERTICAL, `itemSpacing`,
`paddingTop/Right/Bottom/Left`, `primaryAxisAlignItems` MIN|CENTER|MAX|SPACE_BETWEEN,
`counterAxisAlignItems` MIN|CENTER|MAX, `primaryAxisSizingMode` / `counterAxisSizingMode` FIXED|AUTO (AUTO = hug).
Children: `layoutSizingHorizontal` / `layoutSizingVertical` FIXED|HUG|FILL.
COMPONENT: same as a frame, listed in the top-level `components` map by id.
INSTANCE: `componentId` = the component's id, plus a full copy of its children with NEW ids (their text
`characters` are the overrides). FigCraft links them (`mainComponentName` reads back) and records overrides.

## TEXT
`characters` (\n for line breaks), `style {fontFamily, fontWeight (400/500/600/700 -> Regular/Medium/SemiBold/
Bold face), fontSize, lineHeightPx, textAlignHorizontal LEFT|CENTER|RIGHT|JUSTIFIED, textAutoResize
WIDTH_AND_HEIGHT|HEIGHT|NONE}`, `fills` = text colour.

## RECTANGLE / ELLIPSE
Box, fills, strokes, cornerRadius (rectangle). Use thin rectangles for rules and dividers.

## VECTOR
Imports EMPTY ("vectors without geometry are empty (export the file JSON with geometry=paths)"). Create the
vector with a unique name and its box, then after import: `node.set {ids, path: "<SVG path d>", width, height}`
(figrest `vector()` + `vector_batch()`). Strokes/fills on the node are kept.

## Not used / not verified
Images (use `file.import` of a PNG after opening, or a placeholder rectangle), styles and variables in the JSON
(create them with `style.create` / `variable.import` after import), prototype links, constraints.
