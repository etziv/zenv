#!/usr/bin/env node
// figlive: plan first, then execute every step in ONE run (no model round-trip per step), fast or watched live.
//   node figlive.mjs <plan.json> [--check] [--live] [--no-viewer] [--wait 120] [--hold 60] [--pace 1]
//   node figlive.mjs --selftest
// Order (boss 2026-11-03): research first, then the plan, then the build. The plan names its research pack and must
// build every screen the pack lists in its default, empty, loading and error states, and its default state in light AND
// dark (boss 2026-11-10: every direction has both modes), or nothing runs (--check: only that test). --live opens the boss's FigCraft window on the session by itself (--no-viewer: print the link only).
// --live hosts a FigCraft live session on this machine and prints its join link: the boss opens it in FigCraft
// (or File > Join live session), and every step shows the agent's cursor gliding to the part it builds, with a chat
// label of what it is doing; the viewer follows the agent (spotlight). Without --live the steps run back to back.
// Plan (JSON): {"research": ".brain/design/proposal/research.md" | "none: <reason, no product screen>", "app": true
//   (pack checked with --app), "open": "<file .figcraft or REST .json>" (optional: a new file), "save": "<path .figcraft>"
//   (it exists: "open" it to edit, or "replace": true to rebuild; an edit builds only what it changes),
//   "steps": [{"label": "Header", "batch": [{"command", "params", "as"}...]} |
//             {"label": "Card", "draw": {"tool": "rectangle", "from": [x, y], "to": [x, y]}, "props": {node.set keys}} |
//             {"label": "Title", "text": {"text": "...", "x": 0, "y": 0, "parent": id?, "props": {...}}} |
//             {"label": "...", "call": {"tool": "<MCP tool>", "args": {...}}}], each step may add "cursor": [x, y],
//   "screen": "<### name in the pack>", "state": "empty" | "loading" | "error" (none = the default state) and
//   "mode": "light" | "dark" (none = light)}, "fonts": ["<.ttf/.otf from design/scripts/fonts.mjs>"] (loaded into this
//   engine and the boss's window: FigCraft knows only its built-ins and the system's), "defaultFonts": "<why a built-in
//   or system face is right here>" (otherwise a plan drawing in one is refused: they are what every AI design looks like).
// Coordinates are page units. A step's cursor target defaults to the middle of what it made.
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const CLI = process.env.FIGCRAFT_CLI || "C:/Program Files/ArtCraft/FigCraft/figcraft-cli.exe";
const LIB = path.join(path.dirname(fileURLToPath(import.meta.url)), "../../design/scripts/library.mjs");
const STATES = ["default", "empty", "loading", "error"], MODES = ["light", "dark"];
// FigCraft's built-in faces (Program Files/ArtCraft/FigCraft/OFL-*) and the system UI faces: the silent substitutes
const DEFAULT_FACES = ["inter", "roboto", "open sans", "lato", "montserrat", "nunito sans", "poppins", "playfair display",
  "merriweather", "source sans 3", "source serif 4", "liberation sans", "liberation serif", "carlito", "caladea", "arial",
  "helvetica", "segoe ui", "segoe ui variable", "system-ui", "sf pro", "sf pro text", "sf pro display"];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function fig(args) { // a minimal MCP stdio client
  const p = spawn(CLI, ["mcp", ...args], { stdio: ["pipe", "pipe", "pipe"] });
  let buf = "", id = 0; const wait = new Map();
  p.stdout.on("data", (d) => { buf += d; let i; while ((i = buf.indexOf("\n")) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); let m; try { m = JSON.parse(l); } catch { continue; } if (m.id != null && wait.has(m.id)) { wait.get(m.id)(m); wait.delete(m.id); } } });
  p.on("error", (e) => { console.error("figlive: cannot start figcraft-cli (" + e.message + "); set FIGCRAFT_CLI"); process.exit(2); });
  const rpc = (method, params) => new Promise((res) => { const n = ++id; wait.set(n, res); p.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: n, method, params }) + "\n"); });
  return {
    async init() { await rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "figlive", version: "1" } }); p.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n"); },
    async call(name, args) { const r = await rpc("tools/call", { name, arguments: args }); const t = ((r.result && r.result.content) || []).map((c) => c.text || "").join("\n"); if (r.error || (r.result && r.result.isError)) throw new Error(name + ": " + (t || JSON.stringify(r.error)).slice(0, 300)); try { return JSON.parse(t); } catch { return t; } },
    cmd(command, params = {}) { return this.call("run_command", { command, params }); },
    close() { try { p.stdin.end(); p.kill(); } catch { /* gone */ } },
  };
}
const centre = (b) => (b && b.width != null ? [b.x + b.width / 2, b.y + b.height / 2] : null);
async function boundsOf(f, ids) {
  for (const i of [].concat(ids || []).slice(0, 1)) { try { const n = await f.call("get_node", { id: i, depth: 0 }); const b = (n.node || n).absoluteBounds; if (b) return centre(b); } catch { /* not a layer */ } }
  return null;
}
const idsIn = (r) => { const out = []; (function w(v) { if (!v || typeof v !== "object") return; if (Array.isArray(v)) return v.forEach(w); if (v.id != null && (typeof v.id === "number" || /^\d+$/.test(v.id))) out.push(v.id); if (Array.isArray(v.ids)) out.push(...v.ids); for (const k of Object.keys(v)) if (k !== "node") w(v[k]); })(r); return out; };

// The boss's own FigCraft window joins with no Join click: the window on VIEWER_PORT (control port, no automation
// roots, which would switch live sessions off) is told to join; `figcraft.exe <link>` would only prefill the Join
// dialog. Every live run reuses that one window and opens it only when nothing answers there (one window per change
// piled up, boss 2026-11-09).
const VIEWER_PORT = String(process.env.FIGCRAFT_VIEWER_PORT || 7800);
async function attachViewer() { // the boss's window, or null when no FigCraft answers on VIEWER_PORT
  const a = fig(["--attach", VIEWER_PORT, "--timeout", "30"]);
  if (await Promise.race([a.init().then(() => true, () => false), sleep(3000).then(() => false)])) return a;
  a.close(); return null;
}
let joinedDoc = null; // the viewer tab of this run's live session, closed when the run ends
async function joinViewer(link, fonts = []) { // "joined", an error text, or null when no FigCraft answers on VIEWER_PORT
  const a = await attachViewer(); if (!a) return null;
  let r;
  try { await loadFonts(a, fonts, true); await a.cmd("collab.join", { link }); r = "joined"; joinedDoc = ((await a.cmd("doc.list", {})).find((t) => t.active) || {}).id; } catch (e) { r = e.message; }
  a.close(); return r;
}
// One tab per design in the boss's window (boss 2026-11-11: runs piled up "Untitled" tabs). Before the engine opens the
// file: its tab, found by path (`open_file` returns the open tab, unsaved edits kept), is saved when he has unsaved edits
// in it, so an edit starts from them; leftover clean "Untitled" tabs (old live copies) are closed. A tab with unsaved
// work is never closed, and another file's tab is never touched. true when a window answered.
async function tidyViewer(out, log) {
  const a = await attachViewer(); if (!a) return false;
  try {
    if (out && fs.existsSync(out) && (await a.call("open_file", { path: out })).document.dirty) { await a.cmd("file.save", {}); log("saved your unsaved edits in " + out + " first"); }
    for (const t of (await a.cmd("doc.list", {})).reverse()) if (!t.dirty && t.name === "Untitled") await a.cmd("file.close", { index: t.index });
  } catch (e) { log("could not tidy the FigCraft window (" + e.message + ")"); }
  a.close(); return true;
}
// After the run: when the design is in the file (saved now, or unchanged after a failed run), the live tab goes (a window
// marks it unsaved, but its content is in the file) and the window shows the file, reloaded. No file: the live tab is
// the only copy and stays.
async function showInViewer(out, fonts, log) {
  const a = await attachViewer(); if (!a) return;
  const inFile = out && fs.existsSync(out);
  try {
    const t = (await a.cmd("doc.list", {})).find((x) => x.id === joinedDoc);
    if (t && inFile) { try { await a.cmd("doc.switch", { index: t.index }); await a.cmd("collab.leave", {}); await a.cmd("file.close", { index: t.index, force: true }); } catch (e) { log("the live tab stays open (" + e.message + ")"); } }
    else if (t) log("no \"save\" in the plan: the live tab is the only copy of this design; save it from the window");
    if (inFile) { await loadFonts(a, fonts, true); await a.call("open_file", { path: out, reload: true }); log("your FigCraft window shows " + out); }
  } catch (e) { log("could not open " + out + " in the FigCraft window (" + e.message + "): open it yourself"); }
  joinedDoc = null; a.close();
}
// a loaded font lives only in that app session (not saved in the file): the engine and the viewer each load it
async function loadFonts(app, fonts, asData) { // the families loaded
  const fams = [];
  for (const p of fonts) fams.push(...((await app.cmd("text.loadFont", asData ? { data: fs.readFileSync(p).toString("base64") } : { path: path.resolve(p) })).families || []));
  return fams;
}
async function openViewer(link, log, fonts) {
  const say = (r, how) => log(r === "joined" ? how + " and joined the session" : "FigCraft window open, join failed (" + r + "): open the link yourself");
  let r = await joinViewer(link, fonts);
  if (r) return say(r, "reused the open FigCraft window");
  try { spawn(path.join(path.dirname(CLI), "figcraft.exe"), ["--control", VIEWER_PORT], { detached: true, stdio: "ignore" }).unref(); }
  catch (e) { log("could not open FigCraft (" + e.message + "): open the link yourself"); return; }
  for (let k = 0; k < 30; k++) { await sleep(700); if ((r = await joinViewer(link, fonts))) return say(r, "opened a FigCraft window"); }
  log("FigCraft did not answer on port " + VIEWER_PORT + ": open the link yourself");
}
// every screen of the pack built in every state, its default state in both modes (an edit: only what it touches, the
// rest is already in the file), and no step for a screen the pack does not have
export function coverage(steps, pack, edit = false) {
  const sc = (pack.match(/^##[ \t]*Screens[^\n]*\n([\s\S]*?)(?=^##[ \t]|(?![\s\S]))/mi) || [])[1] || "";
  const names = sc.split(/^###[ \t]+/m).slice(1).map((b) => b.split("\n")[0].trim()), key = (n) => String(n).trim().toLowerCase();
  const fails = []; if (!names.length) fails.push("the pack lists no ### screens");
  if (!edit) for (const n of names) { const have = new Set(steps.filter((s) => s.screen && key(s.screen) === key(n)).map((s) => s.state || "default"));
    const miss = STATES.filter((x) => !have.has(x)); if (miss.length) fails.push("screen " + n + ": no step builds its " + miss.join(", ") + " state");
    const modes = new Set(steps.filter((s) => s.screen && key(s.screen) === key(n) && (s.state || "default") === "default").map((s) => s.mode || "light"));
    const noMode = MODES.filter((m) => !modes.has(m)); if (noMode.length) fails.push("screen " + n + ": no step builds its default state in " + noMode.join(", ") + " mode"); }
  for (const s of steps) if (s.mode && !MODES.includes(s.mode)) fails.push("step " + (s.label || "?") + ": mode \"" + s.mode + "\" is not light or dark");
  for (const s of steps) if (s.screen && !names.some((n) => key(n) === key(s.screen))) fails.push("step " + (s.label || "?") + ": screen \"" + s.screen + "\" is not in the pack");
  return fails;
}
// families the plan draws in (steps, and a figrest .json it opens)
export function fontsUsed(plan) {
  let s = JSON.stringify(plan.steps || []);
  if (/\.json$/i.test(plan.open || "") && fs.existsSync(plan.open)) s += fs.readFileSync(plan.open, "utf8");
  return [...new Set([...s.matchAll(/"fontFamily"\s*:\s*"([^"]+)"/g)].map((m) => m[1]))];
}
export function fontGate(plan) {
  const fails = (plan.fonts || []).filter((p) => !fs.existsSync(p)).map((p) => "font file " + p + " does not exist (design/scripts/fonts.mjs fetches it)");
  const dflt = fontsUsed(plan).filter((x) => DEFAULT_FACES.includes(x.trim().toLowerCase()));
  if (dflt.length && !/\S/.test(plan.defaultFonts || "")) fails.push("draws in " + dflt.join(", ") + ": a FigCraft built-in or system face, the look of every AI design. Fetch the pack's type (design/scripts/fonts.mjs) and list the files in \"fonts\", or write why this face is right in \"defaultFonts\"");
  return fails;
}
// an edit opens the saved .figcraft and changes only what changes (boss 2026-11-11: continue the design, not a new file)
const same = (a, b) => { const n = (x) => (process.platform === "win32" ? path.resolve(x).toLowerCase() : path.resolve(x)); return n(a) === n(b); };
export const isEdit = (plan) => !!(plan.open && plan.save && /\.figcraft$/i.test(plan.open) && same(plan.open, plan.save));
export function checkPlan(plan) {
  const steps = plan.steps || [], fails = steps.length ? [] : ["the plan has no steps"];
  if (plan.save && fs.existsSync(plan.save) && !isEdit(plan) && !plan.replace) fails.push(plan.save + " exists: \"open\" that same file to edit the design (steps change only what changes), or \"replace\": true to rebuild it (the old design is overwritten)");
  if (/^none:\s*\S/i.test(plan.research || "")) return fails;
  fails.push(...fontGate(plan));
  if (!plan.research) return fails.concat('"research" is missing: write the research pack first (design skill, job 0), then name it here');
  const r = spawnSync(process.execPath, [LIB, "check", plan.research, ...(plan.app ? ["--app"] : [])], { encoding: "utf8" });
  if (r.status !== 0) return fails.concat("research pack not complete (library.mjs check):\n  " + (r.stdout || r.stderr || "").trim().split("\n").filter((l) => /FAIL/.test(l)).join("\n  "));
  return fails.concat(coverage(steps, fs.readFileSync(plan.research, "utf8"), isEdit(plan)));
}

export async function run(plan, opt = {}) {
  const live = !!opt.live, pace = opt.pace || 1, log = opt.log || console.log;
  const out = plan.save ? path.resolve(plan.save) : null;
  // a live session needs the network, which automation roots switch off; without --live the session is confined
  const roots = live ? [] : [...new Set([process.cwd(), ...(out ? [path.dirname(out)] : []), ...(plan.open ? [path.dirname(path.resolve(plan.open))] : [])])].flatMap((d) => ["--automation-write-root", d]);
  if (out) fs.mkdirSync(path.dirname(out), { recursive: true });
  const f = fig(roots); await f.init(); const loaded = await loadFonts(f, plan.fonts || []);
  const win = opt.viewer !== false && await tidyViewer(out, log);
  const t0 = Date.now(); let cur = [0, 0], link = null, ms = 0, fontFails = [];
  const results = [];
  try {
  if (plan.open) await f.call("open_file", { path: path.resolve(plan.open) }); else await f.cmd("file.new", {});
  if (live) {
    const h = await f.cmd("collab.host", { bind: "local" }); link = h.link;
    log("LIVE: open this link in FigCraft (or File > Join live session):\n  " + link);
    if (opt.onLink) opt.onLink(link);
    if (opt.viewer !== false) await openViewer(link, log, plan.fonts || []);
    const until = Date.now() + (opt.wait ?? 120) * 1000;
    while (Date.now() < until) { const p = await f.cmd("collab.peers", {}); if ((p.peers || []).length) { log("viewer joined: " + p.peers.map((x) => x.name).join(", ")); break; } await sleep(500); }
    try { await f.cmd("collab.spotlight", { on: true }); } catch { /* older app */ }
  }
  const glide = async (to) => { // the cursor travels, so the viewer sees where the agent goes next
    if (!live || !to) return; const n = 8;
    for (let i = 1; i <= n; i++) { const x = cur[0] + (to[0] - cur[0]) * i / n, y = cur[1] + (to[1] - cur[1]) * i / n; await f.cmd("collab.cursor", { x, y }); await sleep(25 * pace); }
    cur = to;
  };
  for (const [i, s] of (plan.steps || []).entries()) {
    const label = s.label || "step " + (i + 1);
    if (live) await f.cmd("collab.cursorChat", { text: label.slice(0, 100) });
    let target = s.cursor || null, r;
    if (s.draw) {
      const d = s.draw; target = target || [(d.from[0] + d.to[0]) / 2, (d.from[1] + d.to[1]) / 2];
      await glide(d.from);
      await f.cmd("tool.set", { tool: d.tool || "rectangle" });
      r = await f.call("pointer", { kind: "drag", x: d.from[0], y: d.from[1], to: d.to, steps: live ? 14 : 2 });
      if ((s.props || s.name) && r.selection && r.selection.length) await f.call("set_properties", { ids: r.selection, props: { ...(s.props || {}), ...(s.name ? { name: s.name } : {}) } });
    } else if (s.text) {
      const t = s.text; await glide(target || [t.x || 0, t.y || 0]);
      r = await f.call("create_node", { type: "text", x: t.x || 0, y: t.y || 0, text: t.text, ...(t.parent ? { parent: t.parent } : {}), ...(t.name ? { name: t.name } : {}), ...(t.props ? { props: t.props } : {}) });
    } else if (s.batch) {
      if (target) await glide(target);
      r = await f.call("batch", { commands: s.batch, ...(s.onError ? { onError: s.onError } : {}) });
    } else if (s.call) {
      if (target) await glide(target);
      r = await f.call(s.call.tool, s.call.args || {});
    } else throw new Error("step " + (i + 1) + ": no draw / text / batch / call");
    if (!s.cursor && !s.draw) await glide(await boundsOf(f, idsIn(r)));
    results.push({ step: i + 1, label, ids: idsIn(r).slice(0, 5) });
    if (live) await sleep(120 * pace);
  }
  if (live) await f.cmd("collab.cursorChat", { text: "Selesai: " + (plan.steps || []).length + " langkah" });
  // a family FigCraft cannot draw falls back silently; say it loudly instead
  const used = (await f.cmd("file.fontReport", {})).fonts || [], fallback = used.filter((x) => !x.available).map((x) => x.family);
  if (fallback.length) log("FONT FALLBACK: " + fallback.join(", ") + " not loaded, drawn in a substitute: add the file to \"fonts\" (design/scripts/fonts.mjs)");
  // allow-list: a product screen draws only the faces it loaded (a text without fontFamily is Inter, styles and variables count too)
  const gated = !/^none:\s*\S/i.test(plan.research || "") && !/\S/.test(plan.defaultFonts || "");
  const unpicked = gated ? used.filter((x) => x.available && !loaded.includes(x.family)).map((x) => x.family) : [];
  if (unpicked.length) log("FONT NOT PICKED: " + unpicked.join(", ") + " drawn but not loaded from \"fonts\" (a built-in, system or default face): give that text a picked face, or say why in \"defaultFonts\"");
  fontFails = [...fallback, ...unpicked];
  ms = Date.now() - t0;
  log("figlive: " + results.length + " steps in " + ms + " ms");
  // the hold comes before the save: what the boss draws while the session stays open is kept
  if (live && opt.hold) { log("session open for " + opt.hold + " s"); await sleep(opt.hold * 1000); }
  if (out) { await f.call("save_file", { path: out }); log("saved " + out); }
  } catch (e) { log("run failed, nothing saved" + (out && fs.existsSync(out) ? ": " + out + " is unchanged" : "")); throw e; }
  finally {
    if (live) { try { await f.cmd("collab.stop", {}); } catch { /* ended */ } }
    f.close();
    if (opt.viewer !== false && (live || win)) await showInViewer(out, plan.fonts || [], log);
  }
  return { results, ms, link, fontFails };
}

// --selftest: a viewer joins the live session headless and must see the cursor, the chat and every layer.
async function selftest() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "figlive-")), save = path.join(dir, "selftest.figcraft");
  const plan = { save, steps: [
    { label: "Bingkai layar", batch: [{ command: "frame.create", params: { x: 0, y: 0, width: 800, height: 500, name: "Screen" }, as: "f" }] },
    { label: "Menggambar kartu", draw: { tool: "rectangle", from: [80, 80], to: [360, 260] }, name: "Card" },
    { label: "Judul", text: { text: "Halo bos", x: 100, y: 100, name: "Title" } },
  ] };
  let viewer = null; const seen = { chat: new Set(), cursor: 0 };
  const watch = async (link) => {
    viewer = fig([]); await viewer.init(); await viewer.cmd("collab.join", { link });
    (async () => { for (let k = 0; k < 400 && viewer; k++) { try { const p = await viewer.cmd("collab.peers", {}); for (const x of p.peers || []) { if (x.chat) seen.chat.add(x.chat); if (x.presence && x.presence.cursor) seen.cursor++; } } catch { /* joining */ } await sleep(40); } })();
  };
  const gate = [
    ["missing pack refused", checkPlan({ research: path.join(dir, "none.md"), steps: [{ label: "x" }] }).length === 1],
    ["no research named refused", checkPlan({ steps: [{ label: "x" }] }).length === 1],
    ["research none: <reason> accepted", checkPlan({ research: "none: architecture diagram", steps: [{ label: "x" }] }).length === 0],
  ];
  const pack = "## Screens\n### Editor\nquestion: q\n### Inbox\nquestion: q\n## Notes\n", all = (n) => [...STATES.map((st) => ({ screen: n, ...(st === "default" ? {} : { state: st }) })), { screen: n, mode: "dark" }];
  gate.push(["every screen in every state: ok", coverage([...all("editor "), ...all("Inbox")], pack).length === 0],
    ["a missing loading state fails", coverage([...all("Editor"), ...all("Inbox").filter((s) => s.state !== "loading")], pack).some((f) => /Inbox.*loading/.test(f))],
    ["a screen without its dark default fails", coverage([...all("Editor"), ...all("Inbox").filter((s) => s.mode !== "dark")], pack).some((f) => /Inbox.*dark mode/.test(f))],
    ["a dark error state is not the dark default", coverage([...all("Editor"), ...all("Inbox").filter((s) => s.mode !== "dark"), { screen: "Inbox", state: "error", mode: "dark" }], pack).some((f) => /Inbox.*dark mode/.test(f))],
    ["dark-first (no light default) fails", coverage([...all("Editor"), ...all("Inbox").filter((s) => s.state || s.mode), { screen: "Inbox", mode: "dark" }].filter((s) => !(s.screen === "Inbox" && !s.state && !s.mode)), pack).some((f) => /Inbox.*light mode/.test(f))],
    ["an unknown mode fails", coverage([...all("Editor"), ...all("Inbox"), { label: "Dim", screen: "Inbox", mode: "dim" }], pack).some((f) => /dim/.test(f))],
    ["a built-in face is refused", fontGate({ steps: [{ batch: [{ params: { props: { fontFamily: "Montserrat" } } }] }] }).some((f) => /Montserrat/.test(f))],
    ["a built-in face with a reason passes", fontGate({ defaultFonts: "desktop exe, 0 KB fonts", steps: [{ text: { props: { fontFamily: "Segoe UI Variable" } } }] }).length === 0],
    ["a picked face passes", fontGate({ steps: [{ text: { props: { fontFamily: "Hanken Grotesk" } } }] }).length === 0],
    ["a missing font file fails", fontGate({ fonts: [path.join(dir, "nope.ttf")], steps: [] }).some((f) => /nope\.ttf/.test(f))],
    ["an explicit state default counts for dark", coverage([...all("Editor"), ...all("Inbox").filter((s) => s.mode !== "dark"), { screen: "Inbox", state: "default", mode: "dark" }], pack).length === 0],
    ["a screen not in the pack fails", coverage([...all("Editor"), ...all("Inbox"), { label: "Ghost", screen: "Settings" }], pack).some((f) => /Settings/.test(f))],
    ["an edit touches one screen state without rebuilding the rest", coverage([{ screen: "Inbox", state: "error" }], pack, true).length === 0],
    ["an edit of a screen not in the pack fails", coverage([{ label: "Ghost", screen: "Settings" }], pack, true).some((f) => /Settings/.test(f))]);
  fs.writeFileSync(path.join(dir, "old.figcraft"), "x");
  const old = { research: "none: test", save: path.join(dir, "old.figcraft"), steps: [{ label: "x" }] };
  gate.push(["an existing design is not silently rebuilt into a new file", checkPlan(old).some((f) => /exists/.test(f))],
    ["opening the existing design (edit) passes", checkPlan({ ...old, open: old.save }).length === 0],
    ["\"replace\": true rebuilds it", checkPlan({ ...old, replace: true }).length === 0],
    ["another file opened and saved over the design needs \"replace\"", checkPlan({ ...old, open: path.join(dir, "template.figcraft") }).some((f) => /exists/.test(f))],
    ["a template .figcraft saved as a new file is not an edit (every state checked)", !isEdit({ open: path.join(dir, "template.figcraft"), save: path.join(dir, "new.figcraft") })],
    ["the same file in other letter case is an edit", process.platform !== "win32" || isEdit({ open: old.save.toUpperCase(), save: old.save })]);
  const r = await run(plan, { live: true, viewer: false, wait: 30, hold: 2, onLink: (l) => { watch(l); }, log: () => {} });
  await sleep(300);
  const doc = viewer ? JSON.stringify(await viewer.call("inspect_document", { depth: 4 })) : "";
  if (viewer) { viewer.close(); viewer = null; }
  const checks = [...gate,
    ["steps ran", r.results.length === 3],
    ["viewer saw the cursor move", seen.cursor > 3],
    ["viewer saw the chat labels", ["Bingkai layar", "Menggambar kartu", "Judul"].some((c) => seen.chat.has(c))],
    ["viewer has the frame, the dragged card and the text", /"FRAME"/.test(doc) && /"RECTANGLE"/.test(doc) && /"TEXT"/.test(doc)],
    ["a face not loaded from fonts is reported (default text = built-in)", r.fontFails.length > 0],
    ["file saved", fs.existsSync(save) && fs.statSync(save).size > 0],
  ];
  for (const [n, ok] of checks) console.log((ok ? "ok   " : "FAIL ") + n);
  console.log("[figlive selftest] " + (checks.every((c) => c[1]) ? "pass" : "FAIL") + " (" + r.ms + " ms)");
  process.exit(checks.every((c) => c[1]) ? 0 : 1);
}

const argv = process.argv.slice(2);
const num = (n, d) => { const i = argv.indexOf(n); return i >= 0 ? Number(argv[i + 1]) : d; };
if (argv.includes("--selftest")) selftest();
else if (argv[0] && !argv[0].startsWith("--")) {
  const plan = JSON.parse(fs.readFileSync(argv[0], "utf8")), fails = checkPlan(plan);
  for (const f of fails) console.log("FAIL " + f);
  if (fails.length) { console.log("[figlive] not built: research first, then a plan that builds every researched screen in every state, its default in light and dark."); process.exit(1); }
  if (argv.includes("--check")) { console.log("[figlive] plan ok: " + (isEdit(plan) ? "an edit of " + plan.open : /^none:/i.test(plan.research || "") ? "no product screen" : "research pack complete, every screen in every state, light and dark") + "."); process.exit(0); }
  run(plan, { live: argv.includes("--live"), viewer: !argv.includes("--no-viewer"), wait: num("--wait", 120), hold: num("--hold", 60), pace: num("--pace", 1) })
    .then((r) => process.exit(r.fontFails.length ? 1 : 0)).catch((e) => { console.error("figlive: " + e.message); process.exit(1); });
} else console.log("usage: figlive.mjs <plan.json> [--check] [--live] [--no-viewer] [--wait 120] [--hold 60] [--pace 1] | --selftest");
