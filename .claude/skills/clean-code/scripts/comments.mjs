#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// ---------- language families

const JS = ["js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts"];
const C_FAMILY = { line: ["//"], block: [["/*", "*/"]], quotes: "'\"" };
const HASH_FAMILY = { line: ["#"], hash: true, quotes: "'\"" };
const SHELL = ["sh", "bash", "zsh", "ps1", "psm1"];
const HASH_EXT = [...SHELL, "rb", "pl", "r", "yml", "yaml", "toml", "dockerfile", "makefile", "mk", "tf", "conf", "ini", "env"];

export function familyOf(file) {
  const base = path.basename(file).toLowerCase();
  if (base.endsWith(".blade.php")) return { html: true };
  const ext = base.includes(".") ? base.slice(base.lastIndexOf(".") + 1) : base;
  if (["html", "htm", "vue", "svelte", "xml", "svg", "astro"].includes(ext)) return { html: true };
  if (JS.includes(ext)) return { ...C_FAMILY, quotes: "'\"`", template: true, regex: true };
  if (["kt", "kts"].includes(ext)) return { ...C_FAMILY, quotes: "'\"`", triple: true };
  if (["java", "scala", "swift", "dart", "groovy", "gradle"].includes(ext)) return { ...C_FAMILY, triple: true };
  if (["c", "h", "cc", "cpp", "hpp", "cs", "rs"].includes(ext)) return C_FAMILY;
  if (ext === "go") return { ...C_FAMILY, quotes: "'\"`", rawBacktick: true };
  if (ext === "php") return { ...C_FAMILY, line: ["//", "#"], hash: true, multiline: true, php: true, heredoc: "php" };
  if (ext === "css") return { ...C_FAMILY, line: [], stylesheet: true };
  if (["scss", "less", "sass"].includes(ext)) return { ...C_FAMILY, lineNeedsSpace: true, stylesheet: true };
  if (["py", "pyi"].includes(ext)) return { ...HASH_FAMILY, triple: true, docstrings: true };
  if (ext === "sql") return { line: ["--"], block: [["/*", "*/"]], quotes: "'\"", multiline: true };
  if (!HASH_EXT.includes(ext)) return null;
  const block = ext.startsWith("ps") ? [["<#", "#>"]] : [];
  return { ...HASH_FAMILY, multiline: SHELL.includes(ext), block, heredoc: heredocKind(ext) };
}

function heredocKind(ext) {
  if (ext.startsWith("ps")) return "ps";
  return ["sh", "bash", "zsh", "rb", "pl"].includes(ext) ? "shell" : undefined;
}

// ---------- scanner

const WORD = /[A-Za-z0-9_]/;
const STRING_PREFIX = /^(r|b|f|u|rb|br|fr|rf|u8|L|U|R)$/i;
const REGEX_AFTER = new Set(["", "(", ",", "=", "=>", ":", "[", "!", "&", "|", "?", "{", "}", ";", "+", "-", "*", "%", "~", "^"]);
const REGEX_WORDS = new Set([
  "return", "typeof", "case", "do", "else", "in", "of", "void", "yield", "await", "throw", "new", "delete", "instanceof",
]);

function skipQuoted(src, i, q, multiline) {
  for (let j = i + 1; j < src.length; j++) {
    const c = src[j];
    if (c === "\\") { j++; continue; }
    if (c === q) return j + 1;
    if (c === "\n" && !multiline) return j;
  }
  return src.length;
}

function skipTemplate(src, i) {
  for (let j = i + 1; j < src.length; j++) {
    const c = src[j];
    if (c === "\\") { j++; continue; }
    if (c === "`") return j + 1;
    if (c === "$" && src[j + 1] === "{") j = skipExpression(src, j + 2) - 1;
  }
  return src.length;
}

function skipExpression(src, j) {
  let prev = "";
  for (let depth = 1; j < src.length; j++) {
    const c = src[j];
    if (c === "'" || c === "\"") j = skipQuoted(src, j, c, false) - 1;
    else if (c === "`") j = skipTemplate(src, j) - 1;
    else if (c === "/" && src[j + 1] === "*") j = closeIndex(src, "*/", j + 2) - 1;
    else if (c === "/" && src[j + 1] === "/") j = closeIndex(src, "\n", j + 2) - 2;
    else if (c === "/" && REGEX_AFTER.has(prev)) j = skipRegex(src, j) - 1;
    else if (c === "{") depth++;
    else if (c === "}" && --depth === 0) return j + 1;
    if (!/\s/.test(c)) prev = significant(src, j);
  }
  return src.length;
}

function closeIndex(src, token, from) {
  const at = src.indexOf(token, from);
  return at < 0 ? src.length : at + token.length;
}

function significant(src, i) {
  if (WORD.test(src[i])) return "a";
  return src[i] === ">" && src[i - 1] === "=" ? "=>" : src[i];
}

const HEREDOC_OPEN = {
  php: /^<<<[ \t]*(['"]?)([A-Za-z_]\w*)\1/,
  shell: /^<<[-~]?[ \t]*(['"]?)([A-Za-z_]\w*)\1/,
  ps: /^@(['"])[ \t]*(?=\r?\n)/,
};

function heredocAt(src, i, kind) {
  if (kind === "shell" && src.startsWith("<<<", i)) return null;
  const m = src.slice(i, i + 120).match(HEREDOC_OPEN[kind]);
  const from = m ? src.indexOf("\n", i) + 1 : 0;
  if (!from) return null;
  const closer = kind === "ps" ? new RegExp(`^${m[1]}@`, "m") : new RegExp(`^[ \\t]*${m[2]}\\b`, "m");
  const c = src.slice(from).match(closer);
  return { from, to: c ? from + c.index + c[0].length : src.length, openLength: m[0].length };
}

function skipRegex(src, i) {
  for (let j = i + 1, inClass = false; j < src.length; j++) {
    const c = src[j];
    if (c === "\\") { j++; continue; }
    if (c === "\n") return j;
    if (c === "[") inClass = true;
    else if (c === "]") inClass = false;
    else if (c === "/" && !inClass) return j + 1;
  }
  return src.length;
}

function wordBefore(src, i) {
  let j = i;
  while (j > 0 && WORD.test(src[j - 1])) j--;
  return src.slice(j, i);
}

export function scanCode(src, o, literals = []) {
  const found = [];
  const opener = src.match(o.php ? /^\s*<\?php\b/ : /^#![^\n]*/);
  let i = opener ? opener[0].length : 0, lastSig = "", lastWord = "", codeSeen = false, heredoc = null;
  const push = (start, end, text, block, docstring = false) =>
    found.push({ start, end, text, block, docstring, beforeCode: !codeSeen });
  while (i < src.length) {
    const ch = src[i];
    if (ch === "\n" && heredoc) {
      literals.push([heredoc.from, heredoc.to]);
      i = heredoc.to; heredoc = null;
      continue;
    }
    if (/\s/.test(ch)) { i++; continue; }
    const block = (o.block || []).find(([open]) => src.startsWith(open, i));
    if (block) {
      const close = src.indexOf(block[1], i + block[0].length), end = close < 0 ? src.length : close + block[1].length;
      push(i, end, src.slice(i + block[0].length, close < 0 ? src.length : close), true);
      i = end;
      continue;
    }
    const lineToken = (o.line || []).find((t) => src.startsWith(t, i) && lineCommentStarts(src, i, t, o));
    if (lineToken) {
      const nl = src.indexOf("\n", i), end = nl < 0 ? src.length : nl;
      push(i, end, src.slice(i + lineToken.length, end), false);
      i = end;
      continue;
    }
    if (o.regex && ch === "/" && (REGEX_AFTER.has(lastSig) || REGEX_WORDS.has(lastWord))) {
      literals.push([i, i = skipRegex(src, i)]);
      lastSig = "/"; lastWord = ""; codeSeen = true;
      continue;
    }
    const opensHeredoc = o.heredoc && !heredoc && (ch === "<" || ch === "@") && heredocAt(src, i, o.heredoc);
    if (opensHeredoc) {
      heredoc = opensHeredoc;
      i += heredoc.openLength; lastSig = "a"; lastWord = ""; codeSeen = true;
      continue;
    }
    if (o.quotes.includes(ch) && quoteOpens(src, i)) {
      const triple = o.triple && src.startsWith(ch.repeat(3), i);
      let end;
      if (triple) { const close = src.indexOf(ch.repeat(3), i + 3); end = close < 0 ? src.length : close + 3; }
      else if (ch === "`" && o.template) end = skipTemplate(src, i);
      else if (ch === "`" && o.rawBacktick) { const close = src.indexOf("`", i + 1); end = close < 0 ? src.length : close + 1; }
      else end = skipQuoted(src, i, ch, o.multiline);
      const lineStart = src.lastIndexOf("\n", i - 1) + 1;
      if (triple && o.docstrings && !/\S/.test(src.slice(lineStart, i).replace(/[rbfu]+$/i, "")) && (lastSig === ":" || lastSig === "")) {
        push(i, end, src.slice(i + 3, Math.max(i + 3, end - 3)), true, true);
      } else { literals.push([i, end]); lastSig = ch; lastWord = ""; codeSeen = true; }
      i = end;
      continue;
    }
    if (WORD.test(ch) || ch === "$") {
      let j = i + 1;
      while (j < src.length && WORD.test(src[j])) j++;
      lastWord = src.slice(i, j); lastSig = "a"; codeSeen = true;
      i = j;
      continue;
    }
    lastSig = significant(src, i); lastWord = ""; codeSeen = true;
    i++;
  }
  return found;
}

function lineCommentStarts(src, i, token, o) {
  if (o.php && token === "#" && src[i + 1] === "[") return false;
  if (token === "//" && src[i - 1] === ":") return false;
  if ((token === "#" && o.hash) || o.lineNeedsSpace) return i === 0 || /[\s;]/.test(src[i - 1]);
  return true;
}

function quoteOpens(src, i) {
  if (i === 0 || !WORD.test(src[i - 1])) return true;
  return STRING_PREFIX.test(wordBefore(src, i));
}

const EMBEDDED = /<(script|style)\b([^>]*)>([\s\S]*?)<\/\1\s*>/gi;
const MARKUP_COMMENT = /<!--([\s\S]*?)(-->|$)|\{\{--([\s\S]*?)(--\}\}|$)/g;
const PROLOG = /<\?xml[^>]*>|<!doctype[^>]*>/gi;

function embeddedFamily(tag, attrs) {
  if (tag.toLowerCase() === "style") return familyOf(/lang\s*=\s*["']?(scss|less)/i.test(attrs) ? "x.scss" : "x.css");
  return familyOf(/lang\s*=\s*["']?ts/i.test(attrs) ? "x.ts" : "x.js");
}

function shifted(found, by) {
  return found.map((c) => ({ ...c, start: c.start + by, end: c.end + by, beforeCode: false }));
}

function scanHtml(src, file) {
  const found = [], masked = src.split("");
  for (const m of src.matchAll(EMBEDDED)) {
    const contentStart = m.index + m[0].indexOf(">") + 1, content = m[3];
    const fam = embeddedFamily(m[1], m[2]);
    found.push(...shifted(scanCode(content, fam), contentStart).map((c) => ({ ...c, label: Boolean(fam.stylesheet) })));
    for (let k = contentStart; k < contentStart + content.length; k++) if (masked[k] !== "\n") masked[k] = " ";
  }
  const frontmatterEnd = file.toLowerCase().endsWith(".astro") && src.startsWith("---") ? src.indexOf("\n---", 3) : -1;
  if (frontmatterEnd > 0) found.push(...shifted(scanCode(src.slice(3, frontmatterEnd), familyOf("x.ts")), 3));
  const text = masked.join("");
  for (const m of text.matchAll(MARKUP_COMMENT)) {
    const beforeCode = !/\S/.test(text.slice(0, m.index).replace(PROLOG, ""));
    found.push({ start: m.index, end: m.index + m[0].length, text: m[1] ?? m[3] ?? "", block: true, beforeCode, label: true });
  }
  return found.sort((a, b) => a.start - b.start);
}

// ---------- grouping and classification

const DIRECTIVES = [
  /^(eslint|jshint|tslint:|biome-ignore|oxlint-|stylelint-|prettier-ignore|deno-lint-ignore)/i,
  /^@ts-(ignore|expect-error|nocheck|check)\b/i,
  /^(istanbul|c8|v8) ignore/i,
  /^globals?\s+[\w$]+(\s*:\s*\w+)?(\s*,\s*[\w$]+(\s*:\s*\w+)?)*$/i,
  /^(webpack\w*\s*:|@vite-ignore|@vite\b|@refresh\b|@jsx(ImportSource|Runtime|Frag)?\b|@flow\b|@format\b|@generated\b)/i,
  /^([#@]__PURE__|[#@] source(MappingURL|URL)=|<reference\s|@license\b|@preserve\b|@deno-types|@(vitest|jest)-environment)/i,
  /^(phpcs:|@?phpstan-|@?psalm-|@codeCoverageIgnore|@?noinspection\b|@formatter:|language=)/i,
  /^suppress\s+[A-Z]\w*(\s*,\s*[A-Z]\w*)*$/,
  /^\{?@inheritDoc\}?$/i,
  /^(noqa\b|type:\s*ignore|pylint:|pyright:|mypy:|fmt:\s*(on|off|skip)|isort:|ruff:|pragma\b|(-\*-\s*)?(en)?coding[:=])/i,
  /^(nolint\b|go:\w+|\+build\s|nosec\b|NOSONAR|detekt\b|ktlint-disable|swiftlint:|clang-format|rubocop:|frozen_string_literal:)/i,
  /^(shellcheck\s|hadolint\s|syntax=|escape=|yaml-language-server:|checkov:|tfsec:|trivy:|cspell:|spell-checker:)/i,
];
const SECTION_KEYWORD = /^(#?\s*(end)?region\b|MARK:|#?pragma mark|<\/?editor-fold)/i;
const SECTION_DECOR = /^([-=*#~_+]{3,})?\s*(.*?)\s*([-=*#~_+]{3,})?$/;
const TAG = /^@[a-z][\w-]*(\s|$)/i;
const CHANGE_TAG = /^@(author|since|version|date|changelog|history|modified)\b/i;
const LICENSE_NAME = /\b(MIT|Apache|L?GPL|BSD|MPL|ISC)\b/i;
const isLicense = (text) => /copyright|spdx-license-identifier|\(c\)\s*\d{4}|licensed under/i.test(text)
  || (/licen[cs]e/i.test(text) && LICENSE_NAME.test(text));
const TODO = /^(todo|fixme|xxx|hack)\b/i;
const CHANGE_NOTES = [
  CHANGE_TAG,
  /^(fix(ed|es)?|changed?|added|removed|updated?|refactor(ed)?|replaced|renamed|moved|reverted|bumped|deprecated|old|new)\b/i,
  /\b(previously|formerly|no longer|used to|hotfix|workaround for|was (changed|added|removed|moved|replaced))\b/i,
  /\b(task[- #]?\d+|ticket|issue #?\d+|PR #?\d+)\b|#\d{2,}\b|\b\d{4}-\d{2}-\d{2}\b|\bv\d+\.\d+/i,
];
const CODE_KEYWORDS = [
  "if", "else", "elif", "for", "foreach", "while", "do", "switch", "case", "return", "break", "continue", "pass", "yield",
  "const", "let", "var", "val", "mut", "function", "def", "fun", "func", "fn", "class", "interface", "enum", "struct", "impl",
  "import", "export", "from", "require", "use", "namespace", "package", "include", "public", "private", "protected", "static",
  "final", "await", "async", "try", "catch", "except", "finally", "throw", "raise", "new", "self", "this", "super",
  "echo", "print", "println", "printf", "console", "dd", "dump", "var_dump", "unset", "assert",
];
const CODE_START = new RegExp(`^(${CODE_KEYWORDS.join("|")})\\b`);
const CODE_SHAPES = [
  /[;{}]$/,
  /^[)\]}]/,
  /^<\/?[a-z][\w.:-]*(\s[^>]*)?\/?>$/,
  /^[$\w.[\]'"]+\s*(=|\+=|-=|\*=|\/=|\.=|:=|\?\?=|=>|->)\s*\S/,
  /^[$\w.:>-]+\(.*\)[;,]?$/,
  /^@\w+(\(.*\))?$/,
];

export const ALLOWED = new Set(["directive", "license", "shortcut", "section", "tags"]);

const isDirective = (l) => DIRECTIVES.some((r) => r.test(l));
const isChangeNote = (l) => CHANGE_NOTES.some((r) => r.test(l));
const codeLike = (l) => CODE_SHAPES.some((r) => r.test(l)) || (CODE_START.test(l) && /[(){}[\];=:]|^\w+$/.test(l));
const words = (s) => s.split(/\s+/).filter((w) => /\p{L}/u.test(w)).length;

function isSection(l) {
  if (SECTION_KEYWORD.test(l)) return words(l) <= 9;
  const m = l.match(SECTION_DECOR);
  return Boolean(m && (m[1] || m[3]) && words(m[2]) <= 8);
}

const TYPE_WORDS = new Set([
  "int", "integer", "string", "bool", "boolean", "float", "double", "array", "mixed", "void", "null", "callable", "iterable",
  "object", "self", "static", "never", "true", "false", "list", "number", "any", "unknown", "resource", "of", "as",
]);
const BRACKET_GROUP = /[({<[][^(){}<>[\]]*[)}>\]]/g;

function collapseBrackets(text) {
  let prev;
  do { prev = text; text = text.replace(BRACKET_GROUP, "#G"); } while (text !== prev);
  return text;
}

function bracketBalance(text) {
  const plain = text.replace(/[-=]>/g, "");
  return (plain.match(/[({<[]/g) || []).length - (plain.match(/[)}>\]]/g) || []).length;
}

function isTypeTag(text) {
  const tokens = collapseBrackets(text.replace(/^@[\w-]+/, "")).split(/\s+/).filter(Boolean);
  const typeLike = (t) => /[$\\|?:#.[\]]/.test(t) || /^[A-Z]/.test(t) || TYPE_WORDS.has(t.toLowerCase());
  return tokens.filter((t) => !typeLike(t)).length <= 1;
}

function isMarkupLabel(text) {
  const titled = words(text) <= 4 && /^\p{Lu}/u.test(text) && !/[.!?:;,]$/.test(text);
  return titled || /^\p{Ll}[\p{L}-]*$/u.test(text);
}

function cleanLines(c) {
  return c.text.split("\n")
    .map((l, offset) => ({ offset, text: l.replace(/\r$/, "").trim().replace(/^\*+(?!\/)\s?/, "").replace(/^[/!]+\s?/, "").trim() }))
    .filter((l) => l.text);
}

function sortLines(lines, c) {
  const kinds = [], rest = [];
  let shortcutLeft = 0;
  for (let k = 0; k < lines.length; k++) {
    const text = lines[k].text;
    if (/^shortcut:/i.test(text)) { kinds.push("shortcut"); shortcutLeft = 1; continue; }
    const simple = isDirective(text) ? "directive" : isSection(text) ? "section" : null;
    if (simple) { kinds.push(simple); shortcutLeft = 0; continue; }
    if (c.block && !c.docstring && TAG.test(text) && !CHANGE_TAG.test(text)) {
      let tag = text, end = k;
      while (bracketBalance(tag) > 0 && end + 1 < lines.length) tag += " " + lines[++end].text;
      if (isTypeTag(tag)) kinds.push("tags"); else rest.push(...lines.slice(k, end + 1));
      k = end; shortcutLeft = 0;
      continue;
    }
    if (shortcutLeft > 0) shortcutLeft--; else rest.push(lines[k]);
  }
  return { kinds, rest };
}

export function classify(c) {
  const lines = cleanLines(c), all = lines.map((l) => l.text);
  if (!lines.length) return { kind: "empty", offset: 0 };
  if (c.beforeCode && isLicense(all.join(" "))) return { kind: "license", offset: 0 };
  const { kinds, rest } = sortLines(lines, c);
  if (!rest.length) return { kind: kinds[0], offset: 0 };
  const prose = rest.map((l) => l.text), offset = rest[0].offset;
  const isBoxedLabel = prose.length === 1 && kinds.filter((k) => k === "section").length >= 2 && words(prose[0]) <= 8;
  const isLabel = c.label && prose.length === 1 && isMarkupLabel(prose[0]) && !isChangeNote(prose[0]) && !TODO.test(prose[0]);
  if (isBoxedLabel || isLabel) return { kind: "section", offset: 0 };
  if (prose.filter(codeLike).length * 2 > prose.length) return { kind: "dead-code", offset };
  if (prose.some(isChangeNote)) return { kind: "change-note", offset };
  if (prose.some((l) => TODO.test(l))) return { kind: "todo", offset };
  return { kind: "explain", offset };
}
function lineIndex(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === "\n") starts.push(i + 1);
  return (pos) => {
    let lo = 0, hi = starts.length - 1;
    while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= pos) lo = mid; else hi = mid - 1; }
    return lo + 1;
  };
}

export function comments(src, file) {
  const fam = familyOf(file);
  if (!fam) return [];
  const raw = fam.html ? scanHtml(src, file) : scanCode(src, fam);
  const lineOf = lineIndex(src), groups = [];
  for (const c of raw) {
    const line = lineOf(c.start), endLine = lineOf(Math.max(c.start, c.end - 1));
    const trailing = /\S/.test(src.slice(src.lastIndexOf("\n", c.start - 1) + 1, c.start));
    const prev = groups[groups.length - 1];
    if (prev && !c.block && !prev.block && !trailing && !prev.trailing && prev.endLine + 1 === line) {
      prev.text += "\n" + c.text; prev.endLine = endLine;
      continue;
    }
    groups.push({ ...c, line, endLine, trailing, label: c.label || Boolean(fam.stylesheet) });
  }
  const srcLines = src.split("\n");
  return groups.map((g) => {
    const { kind, offset } = classify(g), line = g.line + offset;
    return { line, endLine: g.endLine, kind, text: offset ? srcLines[line - 1].trim() : src.slice(g.start, g.end) };
  });
}

// ---------- tidiness metrics

export function untidy(src, file) {
  const lines = src.split("\n"), long = [];
  if (!/\.(xml|svg)$/i.test(file)) lines.forEach((l, k) => { if (l.replace(/\r$/, "").length > 140) long.push(k + 1); });
  return { lines: lines.length, long, ...nestingOf(src, familyOf(file)) };
}

function nestingOf(src, fam) {
  if (fam?.docstrings) return indentNesting(maskCode(src, fam));
  const hasBraces = fam?.block?.some(([open]) => open === "/*") && !fam.line.includes("--");
  return hasBraces ? braceNesting(maskCode(src, fam)) : { maxDepth: 0, maxDepthLine: 0 };
}

function maskCode(src, fam) {
  const literals = [], chars = src.split("");
  for (const c of scanCode(src, fam, literals)) literals.push([c.start, c.end]);
  for (const [a, b] of literals) for (let k = a; k < b; k++) if (chars[k] !== "\n") chars[k] = " ";
  return chars.join("");
}

function braceNesting(code) {
  const open = [];
  let depth = 0, maxDepth = 0, maxDepthLine = 0, line = 1, prev = "";
  for (let k = 0; k < code.length; k++) {
    const c = code[k];
    if (c === "\n") { line++; continue; }
    if (c === "{") {
      const isBlock = !/[=(,:[?>]/.test(prev) && !/\breturn\s*$/.test(code.slice(Math.max(0, k - 12), k));
      open.push(isBlock);
      if (isBlock && ++depth > maxDepth) { maxDepth = depth; maxDepthLine = line; }
    } else if (c === "}" && open.pop()) depth--;
    if (!/\s/.test(c)) prev = c;
  }
  return { maxDepth, maxDepthLine };
}

function indentNesting(code) {
  let maxDepth = 0, maxDepthLine = 0;
  const lines = code.split("\n"), widths = lines.filter((l) => /\S/.test(l)).map((l) => l.match(/^ */)[0].length).filter(Boolean);
  const unit = Math.max(2, widths.length ? Math.min(...widths) : 4);
  lines.forEach((l, k) => {
    if (!/\S/.test(l) || /^\s*[)\]}]/.test(l)) return;
    const depth = Math.floor(l.replace(/\t/g, "    ").match(/^ */)[0].length / unit);
    if (depth > maxDepth) { maxDepth = depth; maxDepthLine = k + 1; }
  });
  return { maxDepth, maxDepthLine };
}

// ---------- files

const SKIP_ANYWHERE = ["node_modules", "generated", "third_party", "third-party", "\\.next", "\\.nuxt", "\\.git", "\\.gradle", "\\.cxx"];
const SKIP_AT_ROOT = [
  "vendor", "storage", "bootstrap/cache", "public/build", "build", "dist", "out", "target", "coverage", "\\.brain", "\\.claude",
  "lang/vendor", "resources/lang/vendor",
];
const SKIP_FILES = "\\.min\\.|\\.(lock|map)$|\\.d\\.ts$";
const SKIP = new RegExp(`(^|/)(${SKIP_ANYWHERE.join("|")})/|^(${SKIP_AT_ROOT.join("|")})/|${SKIP_FILES}`);
const GIT = ["-c", "core.quotePath=false"];

function readText(f) {
  try {
    if (fs.statSync(f).size > 2e6) return null;
    const b = fs.readFileSync(f);
    return b.includes(0) ? null : b.toString("utf8");
  } catch { return null; }
}

function listFiles(args) {
  const out = [];
  for (const a of args.length ? args : [".", ...projectRepos()]) {
    if (fs.existsSync(a) && fs.statSync(a).isFile()) { out.push(a); continue; }
    if (!fs.existsSync(a)) continue;
    const dir = a.replace(/\\/g, "/").replace(/\/+$/, ""), prefix = dir === "." ? 0 : dir.length;
    const g = spawnSync("git", [...GIT, "ls-files", "-co", "--exclude-standard"], { cwd: a, encoding: "utf8", maxBuffer: 256e6 });
    const tracked = g.status === 0 ? g.stdout.split("\n").filter(Boolean) : null;
    const listed = tracked ? tracked.map((f) => (prefix ? dir + "/" : "") + f) : walk(a);
    out.push(...listed.filter((f) => !SKIP.test(f.replace(/\\/g, "/").slice(prefix).replace(/^\/+/, ""))));
  }
  return [...new Set(out)].filter((f) => familyOf(f) && fs.existsSync(f));
}

function projectRepos() {
  try { return JSON.parse(fs.readFileSync(".brain/project.json", "utf8")).repos.map((r) => r.path).filter(Boolean); } catch { return []; }
}

function walk(dir) {
  const out = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) { if (!SKIP.test(p.replace(/\\/g, "/") + "/")) out.push(...walk(p)); } else out.push(p);
  }
  return out;
}

// ---------- gate: only lines added since a base revision

export function addedLines(diff) {
  const map = new Map();
  let file = null;
  for (const l of diff.split("\n")) {
    const h = l.match(/^\+\+\+ (?:b\/(.+?)|\/dev\/null)\s*$/);
    if (h) { file = h[1] || null; if (file && !map.has(file)) map.set(file, new Set()); continue; }
    const m = l.match(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/);
    if (!m || !file) continue;
    const from = +m[1], count = m[2] === undefined ? 1 : +m[2];
    for (let k = 0; k < count; k++) map.get(file).add(from + k);
  }
  return map;
}

function gate(base, files) {
  const code = files.map((f) => f.replace(/\\/g, "/")).filter((f) => familyOf(f) && !SKIP.test(f) && fs.existsSync(f));
  if (!code.length) return [];
  const diffArgs = [...GIT, "diff", "-U0", "--no-color", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/", base, "--", ...code];
  const diff = spawnSync("git", diffArgs, { encoding: "utf8", maxBuffer: 256e6 });
  const added = addedLines(diff.stdout || "");
  const bad = [];
  for (const f of code) {
    const isNew = spawnSync("git", ["cat-file", "-e", base + ":" + f]).status !== 0;
    const lines = added.get(f);
    if (!isNew && !lines) continue;
    const src = readText(f);
    if (src === null) continue;
    for (const c of comments(src, f)) {
      if (ALLOWED.has(c.kind)) continue;
      let hit = isNew;
      for (let k = c.line; !hit && k <= c.endLine; k++) hit = lines.has(k);
      if (hit) bad.push({ file: f, ...c });
    }
  }
  return bad;
}

// ---------- output

const show = (c) => c.file + ":" + c.line + "  " + c.kind.padEnd(11) + " " + c.text.split("\n")[0].trim().slice(0, 90);
const GATE_FIX = [
  "Fix: delete dead code, change notes and empty comments (git keeps history);",
  "make the code say what a comment explained (names, smaller functions, constants);",
  "a deliberate deferral is `shortcut: <limit>, <when to upgrade>`; section markers look like `// ---------- name`.",
].join(" ");

function countBy(list) {
  const counts = {};
  for (const c of list) counts[c.kind] = (counts[c.kind] || 0) + 1;
  return Object.entries(counts).sort((a, b) => b[1] - a[1]).map(([k, n]) => k + " " + n).join(", ");
}

const summary = (list) => list.length + (list.length ? " (" + countBy(list) + ")" : "");

function audit(args) {
  const all = args.includes("--all"), paths = args.filter((a) => !a.startsWith("--"));
  const missing = paths.filter((p) => !fs.existsSync(p));
  if (missing.length) { console.error("clean-code audit: not found: " + missing.join(", ")); return 2; }
  const files = listFiles(paths);
  const banned = [], allowed = [], untidyRows = [];
  let total = 0;
  for (const f of files) {
    const src = readText(f);
    if (src === null) continue;
    const rel = f.replace(/\\/g, "/");
    for (const c of comments(src, f)) (ALLOWED.has(c.kind) ? allowed : banned).push({ file: rel, ...c });
    const u = untidy(src, f);
    total += u.lines;
    const notes = [];
    if (u.lines > 600) notes.push(u.lines + " lines");
    if (u.maxDepth >= 6) notes.push("nesting " + u.maxDepth + " at :" + u.maxDepthLine);
    if (u.long.length) notes.push(u.long.length + " lines > 140 chars (first :" + u.long[0] + ")");
    if (notes.length) untidyRows.push(rel + ": " + notes.join("; "));
  }
  console.log("clean-code audit: " + files.length + " files, " + total + " lines");
  console.log("banned comments: " + summary(banned) + "; allowed: " + summary(allowed));
  const limit = all ? Infinity : 60;
  for (const c of banned.slice(0, limit)) console.log("  " + show(c));
  if (banned.length > limit) console.log("  ... " + (banned.length - limit) + " more (--all)");
  if (untidyRows.length) {
    console.log("untidy files: " + untidyRows.length);
    for (const r of untidyRows.slice(0, all ? Infinity : 30)) console.log("  " + r);
    if (!all && untidyRows.length > 30) console.log("  ... " + (untidyRows.length - 30) + " more (--all)");
  }
  return 0;
}

function main(argv) {
  const [cmd, ...rest] = argv;
  if (cmd === "audit") return audit(rest);
  if (cmd === "gate") {
    const b = rest.indexOf("--base");
    if (b < 0 || !rest[b + 1]) { console.error("usage: comments.mjs gate --base <rev> (files on stdin, one per line)"); return 2; }
    const files = fs.readFileSync(0, "utf8").split(/\r?\n/).filter(Boolean);
    const bad = gate(rest[b + 1], files);
    if (!bad.length) return 0;
    console.log("comment gate: " + bad.length + " banned comment(s) in lines this task added (clean-code skill):");
    for (const c of bad.slice(0, 40)) console.log("  " + show(c));
    if (bad.length > 40) console.log("  ... " + (bad.length - 40) + " more");
    console.log(GATE_FIX);
    return 1;
  }
  console.error("usage: comments.mjs audit [paths...] [--all] | comments.mjs gate --base <rev> < files");
  return 2;
}

const self = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]).toLowerCase() === self.toLowerCase()) process.exitCode = main(process.argv.slice(2));
