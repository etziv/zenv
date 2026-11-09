// Cross-project lessons: a G- dead end / gotcha recorded in one project's .brain/DECISIONS.md is findable from the
// other projects on this machine. Standalone module; memory.mjs wires it in (lessons command, brief registration).
//   registerProject(root, info)  adds/updates { path, name, stack, seen } in <home>/.memoir/projects.json
//   lessons(q, { self, budget, timeoutMs })  G- lines of the OTHER registered projects that match q, labelled
// Each project is searched by its OWN memory.mjs `mem` (its index and ranking; this kit's copy when it has none).
// Never throws: any error returns null / "".
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SELF_FILE = fileURLToPath(import.meta.url);
const KIT_CLI = path.join(HERE, "memory.mjs");
const MAX_ENTRIES = 100; // registry size; the oldest `seen` drops first
const MAX_SEARCHED = 20; // projects searched per call (parallel processes)
const LINE_MAX = 220; // other projects' text is printed to an agent: short lines only

const home = () => process.env.MEMORY_HOME || os.homedir();
const registryFile = () => path.join(home(), ".memoir", "projects.json");
const norm = (p) => { const r = path.resolve(p); return process.platform === "win32" ? r.toLowerCase() : r; };
const today = () => new Date().toISOString().slice(0, 10);
const tok = (s) => Math.ceil(s.length / 4);
// Control, C1, zero-width, line/paragraph separator and bidi-override characters out; whitespace runs collapsed.
const clean = (s) => String(s).replace(/[\u0000-\u001f\u007f-\u009f\u200b-\u200f\u2028-\u202e\u2060-\u206f\ufeff]/g, " ").replace(/\s+/g, " ").trim();

export function readRegistry() {
  try {
    const j = JSON.parse(fs.readFileSync(registryFile(), "utf8"));
    if (!j || !Array.isArray(j.projects)) return [];
    return j.projects.filter((p) => p && typeof p.path === "string" && p.path)
      .map((p) => ({ path: p.path, name: typeof p.name === "string" ? p.name : path.basename(p.path), stack: Array.isArray(p.stack) ? p.stack.filter((w) => typeof w === "string") : [], seen: typeof p.seen === "string" ? p.seen : "" }));
  } catch { return []; } // missing or corrupt: start fresh
}

function writeRegistry(projects) {
  const file = registryFile(), tmp = file + "." + process.pid + ".tmp";
  try {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(tmp, JSON.stringify({ projects }, null, 1) + "\n");
    fs.renameSync(tmp, file); // atomic replace: a reader never sees half a file
    return true;
  } catch { try { fs.unlinkSync(tmp); } catch { /* nothing to remove */ } return false; }
}

// Stack words: the `## Chosen stack` section of ARCHITECTURE.md (what the project decided, not the rejected options)
// matched against a fixed vocabulary, plus marker files at the root and one level down (multi-repo projects keep
// web/ and mobile/ side by side). A fixed vocabulary keeps the words comparable across projects.
const VOCAB = [
  ["laravel", /\blaravel\b/i], ["filament", /\bfilament\b/i], ["livewire", /\blivewire\b/i], ["inertia", /\binertia\b/i],
  ["php", /\bphp\b/i], ["symfony", /\bsymfony\b/i], ["wordpress", /\bwordpress\b/i],
  ["react", /\breact\b/i], ["vue", /\bvue\b/i], ["svelte", /\bsvelte\b/i], ["nextjs", /\bnext\.?js\b/i], ["angular", /\bangular\b/i],
  ["typescript", /\btypescript\b/i], ["node", /\bnode(\.?js)?\b/i], ["vite", /\bvite\b/i], ["tailwind", /\btailwind/i],
  ["kotlin", /\bkotlin\b/i], ["compose", /\b(jetpack compose|compose (compiler|bom|ui))\b/i], ["android", /\bandroid\b/i],
  ["gradle", /\bgradle\b/i], ["java", /\b(java|jdk)\b/i], ["swift", /\bswift(ui)?\b/i], ["flutter", /\bflutter\b/i],
  ["rust", /\brust\b/i], ["golang", /\bgolang\b/i], ["python", /\bpython\b/i], ["django", /\bdjango\b/i], ["fastapi", /\bfastapi\b/i],
  ["dotnet", /(\.net\b|\bc#)/i], ["electron", /\belectron\b/i],
  ["mysql", /\bmysql\b/i], ["mariadb", /\bmariadb\b/i], ["postgres", /\bpostgres/i], ["sqlite", /\bsqlite\b/i],
  ["redis", /\bredis\b/i], ["mongodb", /\bmongo/i], ["docker", /\bdocker\b/i],
];
const MARKERS = [["composer.json", "php"], ["artisan", "laravel"], ["build.gradle", "gradle"], ["build.gradle.kts", "gradle"],
  ["settings.gradle.kts", "gradle"], ["package.json", "node"], ["Cargo.toml", "rust"], ["go.mod", "golang"],
  ["pyproject.toml", "python"], ["requirements.txt", "python"], ["Dockerfile", "docker"]];
const SKIP_DIRS = new Set(["node_modules", "vendor", "build", "dist", "archive", "graphify-out"]);

export function deriveStack(root) {
  const words = new Set();
  try {
    const lines = fs.readFileSync(path.join(root, ".brain", "ARCHITECTURE.md"), "utf8").split(/\r?\n/);
    const at = lines.findIndex((l) => /^#{2,3}\s*chosen stack/i.test(l));
    if (at >= 0) {
      const sec = [];
      for (let i = at + 1; i < lines.length && i < at + 60 && !/^#{1,2}\s/.test(lines[i]); i++) sec.push(lines[i]);
      const text = sec.join("\n");
      for (const [w, re] of VOCAB) if (re.test(text)) words.add(w);
    }
  } catch { /* no architecture yet */ }
  try {
    const dirs = [root];
    for (const e of fs.readdirSync(root, { withFileTypes: true })) if (e.isDirectory() && !e.name.startsWith(".") && !SKIP_DIRS.has(e.name)) dirs.push(path.join(root, e.name));
    for (const d of dirs.slice(0, 30)) for (const [f, w] of MARKERS) if (fs.existsSync(path.join(d, f))) words.add(w);
  } catch { /* unreadable root */ }
  return [...words].slice(0, 12);
}

export function registerProject(root, info = {}) {
  try {
    if (!root) return null;
    const abs = path.resolve(root), key = norm(abs);
    const entry = { path: abs, name: clean(info.name || path.basename(abs)).replace(/[[\]]/g, "").slice(0, 40) || "project",
      stack: Array.isArray(info.stack) ? info.stack.map((w) => clean(w).toLowerCase()).filter(Boolean).slice(0, 12) : deriveStack(abs), seen: today() };
    const list = readRegistry(), i = list.findIndex((p) => norm(p.path) === key);
    if (i >= 0 && JSON.stringify(list[i]) === JSON.stringify(entry)) return entry; // unchanged: no write on every session
    if (i >= 0) list[i] = entry; else list.push(entry);
    // a deleted or moved project leaves the registry at the next write (else it pushes real projects out)
    for (let j = list.length - 1; j >= 0; j--) if (list[j] !== entry && !fs.existsSync(path.join(list[j].path, ".brain"))) list.splice(j, 1);
    list.sort((a, b) => (b.seen > a.seen) - (b.seen < a.seen));
    return writeRegistry(list.slice(0, MAX_ENTRIES)) ? entry : null;
  } catch { return null; }
}

// One `mem` line of another project: "<file>:<line>  G-x ..." (listed) or the same with "# " / "[section] " before G-.
const G_LINE = /^(\S+?:\d+)\s+(?:#\s+|\[[^\]]*\]\s+|-\s+)?(G-\d+\b.*)$/; // "- G-7 ..." is the template's bullet form
// mem lists its top matches whatever their score; from another project that is mostly noise. A lesson is kept when
// its own text shares query words with q: 2 when q has 4+ content words, else 1 (measured on 5 real questions, see
// handoff 17-dev-lessons). Exact words with a plural "s" stripped; no prefix stemming (it let "data" match "database").
const STOP = new Set(("the a an and or of on in at to for from by with as is are was be been do does did not no it its this that " +
  "these those my our your we you can could should would will how what why when where which who too also than then there " +
  "here into out about yang di ke dan atau untuk dari apa kenapa bagaimana tidak ga gak ini itu ada pakai").split(" "));
const qWords = (s) => new Set((String(s).toLowerCase().match(/[\p{L}\p{N}]+/gu) || []).filter((w) => w.length > 1 && !STOP.has(w))
  .map((w) => (w.length > 3 && w.endsWith("s") ? w.slice(0, -1) : w)));

export function lessons(q, { self, budget = 600, timeoutMs = 3000 } = {}) {
  try {
    if (!q || !String(q).trim()) return "";
    const list = readRegistry(), selfKey = self ? norm(self) : null;
    const selfStack = new Set(selfKey ? ((list.find((p) => norm(p.path) === selfKey) || {}).stack || deriveStack(self)) : []);
    const shared = (p) => p.stack.filter((w) => selfStack.has(w)).length;
    const seenKeys = new Set();
    // Shared stack words first (a Laravel trap matters most to another Laravel project), then the newest seen. The
    // others still run: many traps (Windows, adb, git, Docker) cross stacks. Order matters for output and the cap.
    const cand = list.filter((p) => { const k = norm(p.path); if (k === selfKey || seenKeys.has(k)) return false; seenKeys.add(k); return fs.existsSync(path.join(p.path, ".brain")); })
      .sort((a, b) => shared(b) - shared(a) || (b.seen > a.seen) - (b.seen < a.seen));
    const qw = qWords(q), need = qw.size >= 4 ? 2 : 1;
    if (!cand.length || !qw.size) return "";
    const jobs = cand.slice(0, MAX_SEARCHED).map((p) => {
      const own = path.join(p.path, ".claude", "skills", "memory", "scripts", "memory.mjs");
      return { name: clean(p.name).replace(/[[\]]/g, "").slice(0, 40) || path.basename(p.path), root: p.path, cli: process.env.MEMORY_LESSONS_TEST_OWN === "1" && fs.existsSync(own) ? own : KIT_CLI }; // tests only: canned per-project output
    });
    const deadline = Date.now() + Math.max(200, timeoutMs - 150); // the fan-out child stops its searches here
    const r = spawnSync(process.execPath, [SELF_FILE, "--fanout"], { input: JSON.stringify({ q: String(q), budget, deadline, jobs }),
      encoding: "utf8", timeout: timeoutMs + 2000, windowsHide: true, maxBuffer: 8 << 20 });
    let res; try { res = JSON.parse(r.stdout); } catch { return ""; }
    const skipped = [...(res.skipped || []), ...cand.slice(MAX_SEARCHED).map((p) => p.name)];
    const out = []; let used = 0, more = 0;
    for (const { i, text } of res.results || []) {
      for (const l of String(text).split(/\r?\n/)) {
        const m = G_LINE.exec(l); if (!m || /<X>/.test(m[2])) continue; // "Do not <X> because <Y>." is the template
        let hit = 0; for (const w of qWords(m[2])) if (qw.has(w)) hit++;
        if (hit < need) continue;
        let line = "[" + jobs[i].name + "] " + clean(m[1]) + "  " + clean(m[2]);
        if (line.length > LINE_MAX) line = line.slice(0, LINE_MAX - 3) + "...";
        if (used + tok(line) > budget) { more++; continue; }
        out.push(line); used += tok(line) + 1;
      }
    }
    if (more) out.push("(+" + more + " more lessons over the budget)");
    if (skipped.length) out.push("(lessons: not searched within " + timeoutMs + " ms: " + skipped.map((s) => clean(s)).join(", ") + ")");
    return out.join("\n");
  } catch { return ""; }
}

// Fan-out child: one spawnSync from lessons() runs every project's search in parallel here, so the total wall time is
// the slowest project (capped by the deadline), not the sum. Whatever still runs at the deadline is killed and named.
function fanout() {
  let input = ""; process.stdin.setEncoding("utf8");
  process.stdin.on("data", (d) => { input += d; });
  process.stdin.on("end", () => {
    let job; try { job = JSON.parse(input); } catch { process.stdout.write("{}"); return; }
    const results = [], running = new Map(); let finished = false;
    const finish = () => {
      if (finished) return; finished = true;
      const skipped = [];
      for (const [i, c] of running) { skipped.push(job.jobs[i].name); try { c.kill(); } catch { /* already gone */ } }
      results.sort((a, b) => a.i - b.i);
      process.stdout.write(JSON.stringify({ results, skipped }), () => process.exit(0));
    };
    const timer = setTimeout(finish, Math.max(0, job.deadline - Date.now()));
    job.jobs.forEach((j, i) => {
      let text = "";
      try {
        const c = spawn(process.execPath, [j.cli, "mem", job.q, "--budget", String(job.budget)], { cwd: j.root, windowsHide: true,
          stdio: ["ignore", "pipe", "ignore"], env: { ...process.env, MEMORY_ROOT: j.root, MEMORY_NO_ACCESS: "1", MEMORY_NO_EMBED: "1" } });
        running.set(i, c);
        c.stdout.setEncoding("utf8"); c.stdout.on("data", (d) => { if (text.length < 200000) text += d; });
        const done = () => { if (!running.has(i)) return; running.delete(i); results.push({ i, text }); if (!running.size) { clearTimeout(timer); finish(); } };
        c.on("close", done); c.on("error", done);
      } catch { /* spawn failed: nothing from this project */ }
    });
    if (!running.size) { clearTimeout(timer); finish(); }
  });
}

if (process.argv[2] === "--fanout" && path.resolve(process.argv[1] || "") === SELF_FILE) fanout();
