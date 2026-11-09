#!/usr/bin/env node
// The design library, searchable (Node 22+, local, no network, 0 tokens until printed). library/*.json holds what was
// distilled from real shipped work: 20 directions, 56 palettes, 44 type systems, 58 heroes, 185 sections, 202 studied
// sites, motions, and the app catalogue (styles, shells, screen patterns). The files are large (sites.json 850 KB),
// so read them through this, never whole.
//   node library.mjs search "<words>" [--kind <kind>] [--top 8]   one line per match: lib:<kind>/<id>  name - gist
//   node library.mjs show <kind> <id>                               the full entry
//   node library.mjs kinds                                          the kinds and their sizes
//   node library.mjs check .brain/design/proposal/research.md [--app]   the research pack gate (design SKILL.md)
// kinds: directions palettes type heroes sections sites motions app-styles app-shells app-patterns app-kinds
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const LIB = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "library");
const BRANDS = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "brands");
const load = (f) => { try { return JSON.parse(fs.readFileSync(path.join(LIB, f), "utf8")); } catch { return null; } };
let cache = null;
function kinds() {
  if (cache) return cache;
  const app = load("app.json") || {};
  const arr = (v) => (Array.isArray(v) ? v : []);
  cache = {
    directions: arr(load("directions.json")), palettes: arr(load("palettes.json")), type: arr(load("type.json")),
    heroes: arr(load("heroes.json")), sections: arr(load("sections.json")), sites: arr(load("sites.json")),
    motions: arr(load("motions.json")), "app-styles": arr(app.styles), "app-shells": arr(app.shells), "app-patterns": arr(app.patterns),
    "app-kinds": Object.entries(app.kinds || {}).map(([id, v]) => ({ id, name: id, ...v })),
  };
  return cache;
}
const words = (s) => String(s).toLowerCase().split(/[^a-z0-9#]+/).filter((w) => w.length > 1);
const gist = (k, x) => {
  if (k === "palettes") return ["bg", "surface", "ink", "accent"].map((f) => f + " " + (x[f] || "?")).join(", ") + (x.tags ? " | " + [].concat(x.tags).join(" ") : "");
  if (k === "type") return (x.class || "") + ": display " + JSON.stringify(x.display && (x.display.family || x.display)) + ", text " + JSON.stringify(x.text && (x.text.family || x.text));
  if (k === "sites") return (x.url || "") + " | " + (x.idea || "");
  return x.idea || x.blueprint || x.description || x.note || x.basis || "";
};
function search(q, kind, top) {
  const qs = [...new Set(words(q))], all = kinds(), hits = [];
  for (const [k, items] of Object.entries(all)) {
    if (kind && k !== kind) continue;
    for (const x of items) {
      const head = words([x.id, x.name, x.tags, x.tones, x.sectors, x.mood, x.archetype, x.role, x.class, x.suits, x.screen].flat().join(" "));
      const body = words(JSON.stringify(x));
      let s = 0; for (const w of qs) { if (head.includes(w)) s += 3; else if (body.includes(w)) s += 1; }
      if (s) hits.push({ k, x, s });
    }
  }
  hits.sort((a, b) => b.s - a.s);
  if (!hits.length) { console.log("[library] nothing matches \"" + q + "\"" + (kind ? " in " + kind : "") + ": try other words (a sector, a mood, a screen kind)"); return; }
  for (const { k, x } of hits.slice(0, top)) console.log(("lib:" + k + "/" + x.id).padEnd(46) + " " + String(x.name || "").slice(0, 40) + " - " + String(gist(k, x)).replace(/\s+/g, " ").slice(0, 170));
  if (hits.length > top) console.log("(+" + (hits.length - top) + " more: --top, or --kind " + Object.keys(all).join("|") + ")");
}
function show(kind, id) {
  const items = kinds()[kind]; if (!items) { console.log("[library] unknown kind " + kind + "; kinds: " + Object.keys(kinds()).join(" ")); process.exit(1); }
  const x = items.find((i) => i.id === id); if (!x) { console.log("[library] no " + kind + "/" + id + "; search for it first"); process.exit(1); }
  const t = JSON.stringify(x, null, 1); console.log(t.length > 8000 ? t.slice(0, 8000) + "\n(cut at 8000 chars)" : t);
}
// The research pack: proof that the look comes from a survey, the library and the rules, not from taste.
function check(file, app) {
  let t; try { t = fs.readFileSync(file, "utf8"); } catch { console.log("[check] FAIL: " + file + " does not exist. Write the research pack first (design SKILL.md, job 0)."); process.exit(1); }
  const sec = (name) => { const m = t.match(new RegExp("^##[ \\t]*" + name + "[^\\n]*\\n([\\s\\S]*?)(?=^##[ \\t]|(?![\\s\\S]))", "mi")); return m ? m[1] : null; };
  const out = [], fail = (s) => out.push("FAIL " + s), ok = (s) => out.push("ok   " + s);
  // 1. survey: real shipped products, with their addresses
  const sv = sec("Survey");
  if (sv == null) fail("## Survey is missing");
  else {
    const urls = [...new Set(sv.match(/https?:\/\/[^\s)>\]]+/g) || [])], hosts = new Set(urls.map((u) => u.split("/")[2]));
    const takes = (sv.match(/\btake:/gi) || []).length;
    (urls.length >= 8 && hosts.size >= 5 ? ok : fail)("Survey: " + urls.length + " URLs on " + hosts.size + " sites (need 8 on 5+)");
    (takes >= 8 ? ok : fail)("Survey: " + takes + " `take:` notes (what each reference gives this product; need 8)");
  }
  // 2. library picks that exist
  const all = kinds(), refs = [...t.matchAll(/lib:([a-z-]+)\/([\w.-]+)/g)].map((m) => ({ k: m[1], id: m[2] }));
  const bad = refs.filter((r) => !(all[r.k] || []).some((x) => x.id === r.id));
  (bad.length ? fail : ok)("Library ids: " + refs.length + " referenced" + (bad.length ? ", unknown: " + bad.map((r) => r.k + "/" + r.id).slice(0, 8).join(", ") + " (use `search`)" : ", all exist"));
  const has = (k) => refs.some((r) => r.k === k && !bad.includes(r));
  for (const k of ["directions", "palettes", "type"]) (has(k) ? ok : fail)("Library: a " + k + " pick (lib:" + k + "/<id>)");
  if (app) for (const k of ["app-styles", "app-shells", "app-patterns"]) (has(k) ? ok : fail)("Library (app screens): a " + k + " pick");
  else ((has("heroes") || has("sections")) ? ok : fail)("Library (pages): a heroes or sections pick");
  ((has("sites")) ? ok : fail)("Library: at least one studied site (lib:sites/<id>) next to the web survey");
  // 3. brands: read one, or say why none
  const br = sec("Brands");
  if (br == null) fail("## Brands is missing (a brands/<name>.md read, or `none: <reason>`)");
  else { const named = [...br.matchAll(/brands\/([\w.-]+)\.md/g)].map((m) => m[1]); const missing = named.filter((n) => !fs.existsSync(path.join(BRANDS, n + ".md")));
    ((named.length && !missing.length) || /none:/i.test(br) ? ok : fail)("Brands: " + (named.length ? named.join(", ") + (missing.length ? " (not in brands/: " + missing.join(", ") + ")" : "") : /none:/i.test(br) ? "none, with a reason" : "nothing named")); }
  // 4. UX rules from the catalogue
  const ux = sec("UX rules");
  if (ux == null) fail("## UX rules is missing");
  else { const qs = (ux.match(/search\.py/g) || []).length; (qs >= 3 ? ok : fail)("UX rules: " + qs + " catalogue queries recorded (search.py ... --domain/--stack; need 3)"); }
  // 5. every key screen: its question, its signature, its components from the library
  const sc = sec("Screens");
  if (sc == null) fail("## Screens is missing");
  else {
    const blocks = sc.split(/^###[ \t]+/m).slice(1);
    (blocks.length ? ok : fail)("Screens: " + blocks.length + " key screens (### <name> each)");
    for (const b of blocks) {
      const name = b.split("\n")[0].trim(), lib = (b.match(/lib:[a-z-]+\/[\w.-]+/g) || []).length;
      const q = /question:/i.test(b), sig = /signature:/i.test(b), comp = /components?:/i.test(b), st = /states?:[^\n]*empty[^\n]*loading[^\n]*error|states?:[^\n]*loading[^\n]*empty[^\n]*error/i.test(b);
      (q && sig && comp && st && lib ? ok : fail)("Screen " + name + ": " + [q ? "" : "no question:", sig ? "" : "no signature:", comp ? "" : "no components:", st ? "" : "no states: line naming empty, loading and error", lib ? lib + " lib refs" : "no lib: ref"].filter(Boolean).join(", "));
    }
  }
  const nFail = out.filter((l) => l.startsWith("FAIL")).length;
  console.log(out.join("\n") + "\n[check] " + (nFail ? nFail + " FAIL: the pack is not done; no drawing before it passes." : "research pack complete."));
  process.exit(nFail ? 1 : 0);
}
const [cmd, ...a] = process.argv.slice(2);
const opt = (n, d) => { const i = a.indexOf(n); return i >= 0 ? a[i + 1] : d; };
const pos = a.filter((x, i) => !x.startsWith("--") && !(i > 0 && a[i - 1].startsWith("--") && ["--kind", "--top"].includes(a[i - 1])));
if (cmd === "search") search(pos.join(" "), opt("--kind", null), +opt("--top", 8));
else if (cmd === "show") show(pos[0], pos[1]);
else if (cmd === "kinds") for (const [k, v] of Object.entries(kinds())) console.log(k.padEnd(14) + v.length);
else if (cmd === "check") check(pos[0], a.includes("--app"));
else console.log("usage: library.mjs search \"<words>\" [--kind k] [--top n] | show <kind> <id> | kinds | check <research.md> [--app]");
