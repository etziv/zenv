import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { indentationMarkers } from "@replit/codemirror-indentation-markers";
import { tags as t } from "@lezer/highlight";

import { rainbowBrackets } from "./brackets";

/** Zenv's black panel around VS Code's Dark+ colours. */
const C = {
  bg: "#0d0e0e",
  raised: "#151717",
  line: "#232626",
  fg: "#d4d4d4",
  gutter: "#6e7681",
  led: "#3ee8a0",
  selection: "#264f78",
};

const view = EditorView.theme(
  {
    "&": { color: C.fg, backgroundColor: C.bg, height: "100%", fontSize: "13px" },
    ".cm-scroller": { fontFamily: '"JetBrains Mono Variable", "Cascadia Mono", Consolas, monospace', lineHeight: "1.55" },
    ".cm-content": { caretColor: "#aeafad", paddingBlock: "8px" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#aeafad", borderLeftWidth: "2px" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": { backgroundColor: C.selection },
    ".cm-activeLine": { backgroundColor: "#ffffff08" },
    ".cm-gutters": { backgroundColor: C.bg, color: C.gutter, border: "none", paddingRight: "4px" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "#c6c6c6" },
    ".cm-foldPlaceholder": { backgroundColor: C.raised, border: `1px solid ${C.line}`, color: C.gutter },
    ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": { backgroundColor: "#0064001a", outline: "1px solid #888888" },
    ".cm-nonmatchingBracket": { color: "#ff5a4a" },
    ".cm-searchMatch": { backgroundColor: "#623315", outline: "1px solid #9e6a03" },
    ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#515c6a" },
    ".cm-selectionMatch": { backgroundColor: "#add6ff26" },
    ".cm-panels": { backgroundColor: C.raised, color: C.fg, borderColor: C.line },
    ".cm-panels.cm-panels-top": { borderBottom: `1px solid ${C.line}` },
    ".cm-panels.cm-panels-bottom": { borderTop: `1px solid ${C.line}` },
    ".cm-panel input, .cm-panel button": { fontSize: "12px" },
    ".cm-textfield": { backgroundColor: C.bg, border: `1px solid ${C.line}`, color: C.fg, borderRadius: "4px" },
    ".cm-button": { backgroundImage: "none", backgroundColor: C.bg, border: `1px solid ${C.line}`, color: C.fg, borderRadius: "4px" },
    ".cm-tooltip": { backgroundColor: "#202222", border: `1px solid ${C.line}`, color: C.fg },
    ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "#04395e", color: "#fff" },
  },
  { dark: true },
);

/** VS Code Dark+ token colours. */
const highlight = HighlightStyle.define([
  { tag: [t.controlKeyword, t.moduleKeyword], color: "#c586c0" },
  { tag: [t.keyword, t.definitionKeyword, t.modifier, t.operatorKeyword, t.self, t.null, t.bool, t.atom], color: "#569cd6" },
  { tag: [t.string, t.special(t.string), t.character], color: "#ce9178" },
  { tag: [t.regexp, t.escape], color: "#d16969" },
  { tag: [t.number, t.integer, t.float], color: "#b5cea8" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "#6a9955" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName)), t.macroName], color: "#dcdcaa" },
  { tag: [t.className, t.typeName, t.namespace, t.definition(t.typeName), t.standard(t.typeName)], color: "#4ec9b0" },
  { tag: [t.variableName, t.propertyName, t.definition(t.variableName), t.special(t.variableName), t.labelName], color: "#9cdcfe" },
  { tag: [t.constant(t.variableName), t.standard(t.variableName)], color: "#4fc1ff" },
  { tag: [t.attributeName], color: "#9cdcfe" },
  { tag: [t.tagName, t.angleBracket], color: "#569cd6" },
  { tag: [t.operator, t.punctuation, t.separator], color: "#d4d4d4" },
  { tag: [t.meta, t.processingInstruction, t.annotation], color: "#569cd6" },
  { tag: [t.heading], color: "#569cd6", fontWeight: "bold" },
  { tag: [t.strong], fontWeight: "bold" },
  { tag: [t.emphasis], fontStyle: "italic" },
  { tag: [t.link, t.url], color: "#3794ff", textDecoration: "underline" },
  { tag: [t.invalid], color: "#f44747" },
]);

export const zenvEditorTheme = [
  view,
  syntaxHighlighting(highlight),
  rainbowBrackets,
  indentationMarkers({
    highlightActiveBlock: true,
    markerType: "codeOnly",
    colors: { dark: "#404040", activeDark: "#707070", light: "#404040", activeLight: "#707070" },
  }),
];
