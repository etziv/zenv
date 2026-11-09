import { syntaxTree } from "@codemirror/language";
import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

/** VS Code's bracket pair colors, by depth. */
const COLORS = ["#ffd700", "#da70d6", "#179fff"];
const marks = COLORS.map((c) => Decoration.mark({ attributes: { style: `color: ${c}` } }));
const OPEN = new Set(["(", "[", "{"]);
const CLOSE = new Set([")", "]", "}"]);

/** Brackets coloured by nesting, as in VS Code; brackets in strings and comments are not tokens, so they stay as they are. */
function build(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const { from: visibleFrom, to: visibleTo } = view.viewport;
  const doc = view.state.doc;
  let depth = 0;
  syntaxTree(view.state).iterate({
    to: visibleTo,
    enter: (node) => {
      if (node.to - node.from !== 1) return;
      const ch = doc.sliceString(node.from, node.to);
      if (OPEN.has(ch) && node.name === ch) {
        if (node.from >= visibleFrom) builder.add(node.from, node.to, marks[depth % marks.length]!);
        depth++;
      } else if (CLOSE.has(ch) && node.name === ch) {
        depth = Math.max(0, depth - 1);
        if (node.from >= visibleFrom) builder.add(node.from, node.to, marks[depth % marks.length]!);
      }
    },
  });
  return builder.finish();
}

export const rainbowBrackets = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
