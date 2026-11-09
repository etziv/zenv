// brain: memory as connected neurons. Pure functions (no file or git access), used by memory.mjs and its tests.
// Every memory unit (a section, a D-/G- entry, a table row) is a node; edges come from what the unit's TEXT says:
// a markdown link, an id it names (D-012), a code symbol or repo path it names, the task it belongs to, the table it
// sits in. Edges used together strengthen (Hebbian co-recall) and fade when unused (half-life 30 days); nothing is
// ever deleted, weak edges only stop counting. Recall spreads activation from the best hits to related units.
export const BRAIN_VERSION = 1;

// Edge types and their base weights. One edge per node pair: the strongest type wins.
export const W = { link: 1.0, super: 0.9, id: 0.8, code: 0.7, path: 0.7, in: 0.6, task: 0.5, part: 0.4, topic: 0.5, space: 0.5 };

// A name worth linking: two or more word parts (camelCase, PascalCase, snake_case), so `status` or `update` never
// links but `TaskRejections` and `can_update_progress` do.
export const distinctive = (w) => w.length >= 5 && /[a-z][A-Z]|[A-Z][a-z]+[A-Z]|[a-z0-9]_[a-z]/.test(w);
export const slug = (h) => h.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").replace(/\s/g, "-");
const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
export const idOf = (head) => (head.replace(/^\s*-\s*/, "").match(/^([DGF]-\d+[a-z]?)\b/) || [])[1] || null;

export function classify(u) {
  const f = u.file;
  if (/(^|\/)(archive|handoff)\//.test(f)) return "task";
  if (u.kind === "gotcha" || /^G-\d/.test(idOf(u.head) || "")) return "gotcha";
  if (/^D-\d/.test(idOf(u.head) || "")) return "decision";
  if (/^F-\d/.test(idOf(u.head) || "") || /FLOWS\.md$/.test(f)) return "flow";
  if (/(^|\/)contracts\//.test(f)) return "contract";
  if (/MODULES\.md$/.test(f)) return "module";
  if (/STATUS\.md$/.test(f)) return "status";
  return "note";
}
export const shortLabel = (n) => (n.type === "code" || n.type === "file" || n.type === "hub" || n.type === "place" ? n.label : idOf(n.label) || n.label.replace(/^\|\s*/, "").slice(0, 48));

// units: [{ key, file (repo-relative, "/"), line, head, kind, body: [lines], section, date (ms or 0) }]
// code:  { defs: { name: [[file, line], ...] }, files: [path, ...] } (from the code graph; may be empty)
export function buildBrain(units, code = {}) {
  const symbols = code.defs || {}, files = new Set(code.files || []);
  const nodes = new Map(), edges = new Map();
  const node = (id, n) => { if (!nodes.has(id)) nodes.set(id, { id, ...n }); return nodes.get(id); };
  const edge = (a, b, t) => {
    if (a === b) return;
    const k = a < b ? a + "\u0001" + b : b + "\u0001" + a, e = edges.get(k);
    if (!e || W[t] > e.w) edges.set(k, { a: a < b ? a : b, b: a < b ? b : a, w: W[t], t });
  };
  const text = (u) => u.head + "\n" + (u.section || "") + "\n" + u.body.join("\n");
  for (const u of units) {
    const body = u.body.map((l) => l.trim()).filter(Boolean).join(" ").slice(0, 240);
    // a decision or gotcha is dated by its heading (`[active] 2026-10-27`), not by the newest date anywhere in its text
    const ty = classify(u), hd = (u.head.match(/\b20\d\d-\d\d-\d\d\b/g) || []).pop();
    node(u.key, { type: ty, label: u.head.replace(/^\s*-\s*/, "").slice(0, 160), file: u.file, line: u.line,
      date: (ty === "decision" || ty === "gotcha") && hd ? Date.parse(hd) : u.date || 0, cold: /(^|\/)(archive|handoff)\//.test(u.file), snippet: body, superseded: /\[\s*superseded/i.test(u.head) });
  }
  // Owners of ids: the unit whose heading starts with the id; a live one wins over an archived one.
  const owner = new Map();
  for (const u of units) {
    const id = idOf(u.head); if (!id) continue;
    const prev = owner.get(id);
    if (!prev || (nodes.get(prev.key).cold && !nodes.get(u.key).cold)) owner.set(id, u);
  }
  const byFile = new Map();
  for (const u of units) { if (!byFile.has(u.file)) byFile.set(u.file, []); byFile.get(u.file).push(u); }
  const resolveLink = (from, target) => {
    const [p, anchor] = target.split("#");
    let f = from;
    if (p) {
      const parts = (from.includes("/") ? from.slice(0, from.lastIndexOf("/") + 1) + p : p).split("/"), st = [];
      for (const x of parts) { if (x === "..") st.pop(); else if (x && x !== ".") st.push(x); }
      f = st.join("/");
    }
    const us = byFile.get(f); if (!us) return null;
    if (!anchor) return us[0];
    const a = anchor.toLowerCase();
    const m = us.find((x) => { const s = slug(x.head); return s === a || s.startsWith(a) || (a.startsWith(s) && s.length >= 8); });
    if (m) return m;
    const id = (a.match(/^([dgf]-\d+[a-z]?)(?:-|$)/) || [])[1];
    return (id && owner.get(id.toUpperCase())) || us[0];
  };
  const fileFor = (p) => {
    if (files.has(p)) return p;
    const hits = []; for (const f of files) if (f.endsWith("/" + p)) { hits.push(f); if (hits.length > 1) break; }
    return hits.length === 1 ? hits[0] : null;
  };
  const tasks = new Map();
  for (const u of units) {
    const t = text(u), me = idOf(u.head);
    // explicit markdown links to other memory files / sections
    for (const m of t.matchAll(/\]\(([^)\s]+?\.md(?:#[^)\s]*)?|#[^)\s]+)\)/g)) {
      const v = resolveLink(u.file, m[1]); if (v) edge(u.key, v.key, "link");
    }
    // ids named in the text; `[superseded by D-012]` is its own, stronger edge
    const sup = (u.head.match(/\[\s*superseded by\s+([DG]-\d+[a-z]?)/i) || [])[1];
    if (sup && owner.get(sup.toUpperCase())) edge(u.key, owner.get(sup.toUpperCase()).key, "super");
    for (const m of t.matchAll(/\b([DGF]-\d+[a-z]?)\b/g)) {
      const id = m[1].toUpperCase(); if (id === me) continue;
      const v = owner.get(id); if (v) edge(u.key, v.key, "id");
    }
    // code symbols and repo paths named in the text
    let nc = 0;
    for (const w of new Set(t.match(/[A-Za-z_][A-Za-z0-9_]{4,}/g) || [])) {
      if (nc >= 8 || !distinctive(w) || !own(symbols, w)) continue;
      const locs = symbols[w];
      node("code:" + w, { type: "code", label: w, file: locs[0][0], line: locs[0][1], locs: locs.slice(0, 3), date: 0, cold: false, snippet: locs.map((l) => l[0] + ":" + l[1]).slice(0, 3).join(", ") });
      edge(u.key, "code:" + w, "code"); nc++;
    }
    let np = 0;
    for (const m of t.matchAll(/(?:[\w.-]+\/)+[\w.-]+\.[A-Za-z]{1,5}\b/g)) {
      if (np >= 6) break;
      const f = fileFor(m[0].replace(/^\.\//, "")); if (!f) continue;
      node("file:" + f, { type: "file", label: f, file: f, line: 1, date: 0, cold: false, snippet: "" });
      edge(u.key, "file:" + f, "path"); np++;
    }
    // the task a unit belongs to: an archive/handoff file of task N, or `task 15` / `[task-15]` in its text
    const ids = new Set();
    const fm = u.file.match(/(?:^|\/)archive\/(\d+)\/|(?:^|\/)handoff\/(?:task-)?(\d+)-/); if (fm) ids.add(fm[1] || fm[2]);
    for (const m of t.matchAll(/\btask[- ]?(\d{1,4})\b/gi)) ids.add(String(+m[1]));
    for (const id of ids) { if (!tasks.has(id)) tasks.set(id, new Set()); tasks.get(id).add(u.key); }
    // a table row belongs to its table's section
    if (u.kind === "row") { const s = (byFile.get(u.file) || []).find((x) => x.kind === "section" && x.head === u.section); if (s) edge(u.key, s.key, "part"); }
  }
  // Decisions on the same subject (idf-weighted word cosine), so a newer decision that replaces part of an older one
  // without naming it is still connected: pankon D-039 rewrote who creates tasks, D-017 still said kasubbid. Edge only
  // for a pair in each other's top 3 neighbours and above the project's 98th percentile (at least 0.08).
  const decs = units.filter((u) => classify(u) === "decision" && !/(^|\/)(archive|handoff)\//.test(u.file));
  if (decs.length >= 4) {
    const STOPW = new Set("the and for with from that this not are was were has have had its our you your one two new old get set use used uses when then than into via per only all any can must should will decision why active approved superseded task boss yang dan di ke dari untuk dengan tidak ini itu".split(" "));
    const ws = decs.map((u) => new Set(((u.head + " " + u.body.join(" ")).toLowerCase().match(/[a-z][a-z0-9_]{2,}/g) || []).filter((w) => !STOPW.has(w))));
    const df = new Map(); for (const s of ws) for (const w of s) df.set(w, (df.get(w) || 0) + 1);
    const idf2 = (w) => Math.log(decs.length / (df.get(w) || 1)) ** 2, norm = ws.map((s) => Math.sqrt([...s].reduce((a, w) => a + idf2(w), 0)) || 1);
    const sim = [];
    for (let i = 0; i < decs.length; i++) { sim[i] = []; for (let j = 0; j < decs.length; j++) {
      if (i === j) { sim[i][j] = 0; continue; }
      if (j < i) { sim[i][j] = sim[j][i]; continue; }
      let s = 0; for (const w of ws[i]) if (ws[j].has(w)) s += idf2(w);
      sim[i][j] = s / (norm[i] * norm[j]);
    } }
    const flat = sim.flatMap((r, i) => r.filter((_, j) => j > i)).sort((a, b) => b - a);
    const cut = Math.max(0.08, flat[Math.floor(flat.length * 0.02)] || 0);
    const top3 = sim.map((r) => r.map((s, j) => [s, j]).sort((a, b) => b[0] - a[0]).slice(0, 3).map(([, j]) => j));
    for (let i = 0; i < decs.length; i++) for (const j of top3[i]) if (sim[i][j] >= cut && top3[j].includes(i)) edge(decs[i].key, decs[j].key, "topic");
  }
  // SPACE: every entity has a place. Modules (MODULES.md rows) hold the memory that names their id or sits in their
  // section; code areas (a symbol's or file's folder, 4 levels deep) hold the code. Memory reaches an area through
  // the code it names. Places are hubs: activation passes through them, they are never returned as "related".
  const mods = (code.modules || []).filter((m) => m && /^[A-Za-z]{1,4}\d+[a-z]?$/.test(m.id));
  if (mods.length) {
    const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const re = new RegExp("\\b(" + mods.map((m) => esc(m.id)).join("|") + ")\\b", "g");
    for (const m of mods) node("place:mod:" + m.id, { type: "place", label: m.id + " " + String(m.name || "").split(/[:(]/)[0].trim().slice(0, 60), file: ".memoir/MODULES.md", line: 0, date: 0, cold: false, snippet: "module " + m.id + (m.status ? " [" + m.status + "]" : "") });
    for (const u of units) {
      if (/(^|\/)(archive|handoff)\//.test(u.file)) continue;
      const hits = new Set();
      const own = (u.head.match(/^\s*#*\s*([A-Za-z]{1,4}\d+[a-z]?)\b/) || [])[1];
      if (own && mods.some((m) => m.id === own) && /MODULES\.md$/.test(u.file)) hits.add(own);
      for (const m of (u.head + "\n" + u.body.join("\n")).matchAll(re)) hits.add(m[1]);
      for (const id of [...hits].slice(0, 4)) edge(u.key, "place:mod:" + id, "space");
    }
  }
  const areaOf = (f) => { const p = String(f || "").split("/"); p.pop(); return p.slice(0, 4).join("/"); };
  for (const n of [...nodes.values()]) {
    if (n.type !== "code" && n.type !== "file") continue;
    const a = areaOf(n.file); if (!a) continue;
    node("place:area:" + a, { type: "place", label: a, file: a, line: 0, date: 0, cold: false, snippet: "code area" });
    edge(n.id, "place:area:" + a, "space");
  }
  // MOTION: a flow keeps the order of its steps (the code nodes it passes through), so it can be followed step by step
  for (const u of units) {
    if (classify(u) !== "flow" || !/^F-\d/.test(idOf(u.head) || "")) continue;
    const steps = [];
    for (const r of parseFlow(u).refs) {
      const id = nodes.has("code:" + r.name) ? "code:" + r.name : r.owner && nodes.has("code:" + r.owner) ? "code:" + r.owner : null;
      if (id && !steps.includes(id)) steps.push(id);
    }
    if (steps.length) nodes.get(u.key).steps = steps;
  }
  // a code symbol named in memory links to its file when that file is named too
  for (const n of nodes.values()) if (n.type === "code" && nodes.has("file:" + n.file)) edge(n.id, "file:" + n.file, "in");
  for (const [id, ks] of tasks) {
    if (ks.size < 2) continue;
    node("task:" + id, { type: "hub", label: "task " + id, file: "", line: 0, date: 0, cold: false, snippet: ks.size + " notes" });
    for (const k of ks) edge(k, "task:" + id, "task");
  }
  return { version: BRAIN_VERSION, nodes: [...nodes.values()], edges: [...edges.values()] };
}


// ---------- spreading activation
// From the seeds (best hits first), activation flows over edges: a x weight x 0.5 per hop, divided on hubs with many
// edges (sqrt fan-out), summed where paths meet: a unit two hits both point to is more related than one either does.
// Returns the related nodes, strongest first, each with the bridge that brought it ("via").
export function spread(brain, seeds, { exclude = [], hops = 2, cold = false, limit = 4, min = 0.08, files = false } = {}) {
  const byId = new Map(brain.nodes.map((n) => [n.id, n])), adj = new Map();
  for (const e of brain.edges) {
    if (!adj.has(e.a)) adj.set(e.a, []); if (!adj.has(e.b)) adj.set(e.b, []);
    adj.get(e.a).push([e.b, e.w, e.t]); adj.get(e.b).push([e.a, e.w, e.t]);
  }
  const act = new Map(), via = new Map(), skip = new Set([...seeds, ...exclude]);
  let front = new Map(seeds.filter((s) => byId.has(s)).map((s, i) => [s, 1 - 0.2 * Math.min(i, 3)]));
  for (let h = 0; h < hops && front.size; h++) {
    const next = new Map();
    for (const [n, a] of front) {
      const ns = adj.get(n) || [], fan = ns.length > 6 ? Math.sqrt(6 / ns.length) : 1;
      for (const [m, w, t] of ns) {
        if (seeds.includes(m)) continue;
        const add = a * w * 0.5 * fan;
        next.set(m, (next.get(m) || 0) + add);
        const best = via.get(m);
        if (!best || add > best.add) via.set(m, { add, from: n, t, hop: h });
      }
    }
    for (const [m, a] of next) act.set(m, (act.get(m) || 0) + a);
    front = next;
  }
  const res = [];
  for (const [id, a] of act) {
    const n = byId.get(id);
    if (!n || skip.has(id) || n.type === "hub" || n.type === "place" || (n.type === "file" && !files) || (n.cold && !cold) || a < min) continue;
    res.push({ node: n, act: n.type === "code" ? a * 0.8 : a, via: describeVia(byId, via, id) });
  }
  return res.sort((x, y) => y.act - x.act).slice(0, limit);
}
const VIA = { co: "recalled together", link: "link", super: "superseded", id: "id", code: "code", path: "file", part: "same table", in: "file", task: "task" };
function describeVia(byId, via, id) {
  const v = via.get(id); if (!v) return "";
  const from = byId.get(v.from); if (!from) return VIA[v.t] || v.t;
  // the node the activation came through is the association: a seed (one hop) or a bridge (code, file, task, note)
  return "via " + shortLabel(from) + (from.type === "hub" || from.type === "place" ? " (same place)" : " (" + (VIA[v.t] || v.t) + ")");
}

// ---------- flows: how the system works now (`## F-<n> <name>` in .memoir/FLOWS.md)
// Steps name their code in backticks (`TaskBoardViewModel.reject`, `TaskController@reject`, `RejectionService`);
// `History:` lines (`- 2026-10-30 task 15: rejection needs a reason`) record how the flow got here, newest date =
// when the flow text was last brought up to date.
const NOT_CODE = /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|true|false|null)$/;
export function parseFlow(u) {
  const id = idOf(u.head), body = u.body;
  const hi = body.findIndex((l) => /^\s*(?:[-*]\s*)?\**History\**\s*:/i.test(l));
  const hist = [];
  if (hi >= 0) for (const l of body.slice(hi)) for (const m of l.matchAll(/(20\d\d-\d\d-\d\d)\s*:?\s*([^|]*)/g)) hist.push({ date: m[1], text: m[2].trim().replace(/^[-:]\s*/, "") });
  const steps = hi >= 0 ? body.slice(0, hi) : body;
  const refs = [];
  for (const l of steps) for (const m of l.matchAll(/`([^`]+)`/g)) {
    const s = m[1].trim().replace(/\(\)$/, "");
    if (!/^[A-Za-z_][\w]*(?:(?:\.|@|::|#)[A-Za-z_][\w]*)*$/.test(s) || NOT_CODE.test(s)) continue;
    if (/\.(php|js|jsx|ts|tsx|mjs|vue|kt|kts|java|rs|py|go|rb|cs|swift|dart|md|json|ya?ml|html?|css|sql|xml|blade)$/i.test(s)) continue; // a file name, not Owner.method
    const parts = s.split(/\.|@|::|#/);
    if (!refs.some((r) => r.text === s)) refs.push({ text: s, owner: parts.length > 1 ? parts[parts.length - 2] : null, name: parts[parts.length - 1] });
  }
  const last = hist.map((h) => h.date).sort().pop() || null;
  return { id, title: u.head.replace(/^F-\d+[a-z]?\s*/, ""), refs, history: hist, last, text: u.head + "\n" + body.join("\n") };
}
// Which named code no longer exists. `defs` = { name: [[file, line], ...] } of every callable in the code graph.
// Only clear cases count: `Owner.method` whose owner exists but the method does not, or a class-like name
// (PascalCase with two parts) that is gone. snake_case (tables, columns) and one-word names are never flagged.
// `methods` = { "Owner.method": [[file, line]] } when the graph knows who owns what; `seen` = names that resolved at an
// earlier check: a class-like name is reported gone only if it once existed (a vendor class never did). No graph
// (empty defs): nothing is reported.
export function missingRefs(flow, defs, methods = null, seen = null) {
  const has = (n) => own(defs, n), hasM = (k) => methods && own(methods, k);
  if (!Object.keys(defs).length) return [];
  const byOwner = methods && Object.keys(methods).length;
  // only a known type can lack a method: `payload::build` where `payload` is a function elsewhere is not "gone"
  const types = byOwner ? new Set(Object.keys(methods).map((k) => k.split(".")[0])) : null;
  return flow.refs.filter((r) => r.owner
    ? has(r.owner) && (byOwner ? types.has(r.owner) && !hasM(r.owner + "." + r.name) : !has(r.name))
    : /^[A-Z][a-z0-9]+[A-Z]\w*$/.test(r.name) && !has(r.name) && !!(seen && seen.has(r.name)));
}
// The code a flow depends on, as line spans: the method for `Owner.method` (in the owner's file), the definition
// otherwise; a span runs to the next definition in that file (`lines` = { file: [sorted start lines] }). A step that
// is another flow's entry point (its first named code) is that flow's to keep current, so it is left out (`skip`).
export function flowSpans(flow, defs, lines = {}, skip = new Set(), methods = null) {
  const has = (n) => own(defs, n), res = [], seen = new Set();
  const ownM = (k) => (methods && own(methods, k) ? methods[k] : null);
  // an owner that owns no methods is a module or namespace (Rust `campaigns::create`): take the definition in the
  // file named after it (campaigns.rs), never a same-named function somewhere else
  const types = methods && Object.keys(methods).length ? new Set(Object.keys(methods).map((k) => k.split(".")[0])) : null;
  const base = (f) => String(f).split("/").pop().replace(/\.[^.]+$/, "");
  const span = (ref, file, line) => {
    const k = file + ":" + line; if (seen.has(k)) return; seen.add(k);
    const next = (lines[file] || []).find((l) => l > line);
    res.push({ ref: ref.text, file, from: line, to: next ? next - 1 : null });
  };
  for (const r of flow.refs) {
    if (skip.has(r.text)) continue;
    if (r.owner && types && !types.has(r.owner)) {
      const m = has(r.name) ? defs[r.name].filter((d) => base(d[0]) === r.owner) : [];
      for (const [file, line] of m.slice(0, 3)) span(r, file, line);
      continue;
    }
    if (r.owner && has(r.owner)) {
      // the owner's own method when the index has it (a common name like `update` has dozens of definitions)
      const ownFiles = new Set(defs[r.owner].map((d) => d[0]));
      const m = ownM(r.owner + "." + r.name) || (has(r.name) ? defs[r.name].filter((d) => ownFiles.has(d[0])) : []);
      for (const [file, line] of m.length ? m : defs[r.owner].slice(0, 3)) span(r, file, line);
    } else if (!r.owner && has(r.name) && (distinctive(r.name) || /^[A-Z]/.test(r.name))) {
      for (const [file, line] of defs[r.name].slice(0, 3)) span(r, file, line);
    }
  }
  return res;
}
export const flowFiles = (flow, defs, lines, skip, methods) => [...new Set(flowSpans(flow, defs, lines, skip, methods).map((s) => s.file))];

// Steps the code takes that the flow never mentions: what the flow's entry method calls in another project class
// (two-part class name), when neither that class nor the method appears anywhere in the flow text. Catches a step
// added to the code in the same task as the flow edit but left out of the text (dates cannot see that).
// `calls` = { "Owner.method": ["Callee.method", ...] } from the code graph.
export function unmentionedCalls(flow, calls) {
  const e = flow.refs[0]; if (!e || !e.owner || !calls) return [];
  const k = e.owner + "." + e.name, cs = own(calls, k) ? calls[k] : [];
  const text = flow.text || flow.refs.map((r) => r.text).join(" ");
  return cs.filter((c) => { const [o, m] = c.split("."); return o && m && o !== e.owner && distinctive(o) && /^[A-Z]/.test(o) && !text.includes(o) && !text.includes(m + "("); }).slice(0, 3);
}
