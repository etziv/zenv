#!/usr/bin/env node
// Task script: the only thing that writes to git. Agents edit files; this script does the git work.
// Usage (run from anywhere inside the project; <id> defaults to the current task branch):
//   node .claude/scripts/task.mjs start <title...>       new task with the next id, local branch task/<id> (no approval);
//                                                      creates the git repository and first commit if missing
//   node .claude/scripts/task.mjs init-repo              only the repository bootstrap (used by init-project)
//   node .claude/scripts/task.mjs check [--fresh]        run the verification commands only (no approval); on a task
//                                                        only where its code changed; code that already passed is reused
//   node .claude/scripts/task.mjs status                 current task, branch, changed files
//   node .claude/scripts/task.mjs ship [id] [--evidence "<proof>"] [--fresh]  verify (a pass on the same code is reused;
//                                                        --fresh after .env / database / package changes), commit, then push / PR / CI / merge
//                                                      by releaseMode (commit-only merges into base locally)
//   node .claude/scripts/task.mjs finish [id]            continue an approved ship after CI (no new approval)
//   node .claude/scripts/task.mjs abort [id] [--delete]  save work as a WIP commit on the task branch, go back to base
//   node .claude/scripts/task.mjs usage [id|all]         token use and cost per task and model (from the usage hook)
//   node .claude/scripts/task.mjs deploy [--sha <sha>] [--retest] | --rollback  start the deploy workflow (dev asks the boss first)
//   node .claude/scripts/task.mjs deploy-status [run]    follow a deploy run (no approval)
// Config: .memoir/project.json (base, releaseMode, mergeMethod, ciWaitMinutes, verify, repos).
// repos (optional): a product whose code lives in separate git repos under this home repo (kit + memory), e.g.
//   "repos": [{ "name": "web", "path": "src", "verify": [...] }, { "name": "mobile", "path": "mobile", "verify": [...] }]
// ship verifies each repo that changed and commits it on the branch it has checked out ([task-<id>], no branch,
// no merge, no push: the boss owns those repos' branches). Files that were already uncommitted when the task started
// are the boss's work and are never committed by ship.
// State: <git dir>/memoir/. No guards in this kit: when to ask the boss is dev's process (CLAUDE.md); this script
// only guarantees that nothing is committed or sent unless the verification commands pass.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const [sub, ...argv] = process.argv.slice(2);

function die(msg) { console.error("task: " + msg); process.exit(1); }
function run(bin, args, opts = {}) {
  const r = spawnSync(bin, args, { cwd: root, encoding: "utf8", ...opts });
  if (r.error) return { ok: false, out: "", err: String(r.error.message || r.error) };
  return { ok: r.status === 0, out: (r.stdout || "").trim(), err: (r.stderr || "").trim() };
}
function git(args, opts) { return run("git", args, opts); }
function mustGit(args, what) {
  const r = git(args);
  if (!r.ok) die((what || "git " + args.join(" ")) + " failed: " + (r.err || r.out));
  return r.out;
}
const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

const ID_RE = /^[A-Za-z0-9][\w.-]{0,40}$/;
const branchOf = (id) => "task/" + id;

function rawConfig() {
  const f = path.join(root, ".memoir", "project.json");
  if (!fs.existsSync(f)) return {};
  try { return JSON.parse(fs.readFileSync(f, "utf8")); } catch { die(".memoir/project.json is not valid JSON"); }
}
function config() {
  const c = rawConfig();
  return {
    base: c.base || "main",
    releaseMode: c.releaseMode || "merge",
    mergeMethod: c.mergeMethod || "squash",
    ciWaitMinutes: Number.isFinite(c.ciWaitMinutes) ? c.ciWaitMinutes : 5,
    deployWaitMinutes: Number.isFinite(c.deployWaitMinutes) ? c.deployWaitMinutes : 9,
    verify: verifyList(c.verify),
    repos: (Array.isArray(c.repos) ? c.repos : []).filter((r) => r && r.path).map((r) => {
      const dir = path.resolve(root, r.path);
      if (!fs.existsSync(path.join(dir, ".git"))) die("repos: " + r.path + " is not a git repository (.memoir/project.json)");
      return { name: r.name || r.path, path: r.path.replace(/\\/g, "/").replace(/\/+$/, ""), dir, verify: verifyList(r.verify) };
    }),
  };
}
function verifyList(v) { return Array.isArray(v) ? v.filter((x) => x && x.run && String(x.run).trim()) : []; }
const MODES = ["commit-only", "push", "pr", "merge"];

let stateDirCache = null; // one git spawn per run (~120 ms each on Windows, more under an antivirus)
function stateDir() {
  if (stateDirCache) return stateDirCache;
  const g = git(["rev-parse", "--absolute-git-dir"]);
  if (!g.ok) die("no git repository yet. `node .claude/scripts/task.mjs start <title>` (or `init-repo`) creates it; the boss does not need to.");
  const d = path.join(g.out, "memoir");
  fs.mkdirSync(d, { recursive: true });
  return (stateDirCache = d);
}
const readJson = (f) => { try { return JSON.parse(fs.readFileSync(f, "utf8")); } catch { return null; } };
const writeJson = (f, v) => fs.writeFileSync(f, JSON.stringify(v, null, 2));
const currentBranch = () => mustGit(["rev-parse", "--abbrev-ref", "HEAD"]);
const taskOf = (branch) => (String(branch).match(/^task\/(.+)$/) || [])[1]; // the task id of a task branch
function taskId(arg) {
  if (arg && !arg.startsWith("-")) { if (!ID_RE.test(arg)) die("invalid task id: " + arg); return arg; }
  const id = taskOf(currentBranch());
  if (!id) die("no task id given and not on a task branch.");
  return id;
}

function changedFiles(since, cwd = root) {
  // quotePath off: a non-ASCII name comes back as itself, not as a quoted octal string
  const tracked = git(["-c", "core.quotePath=false", "diff", "--name-only", since], { cwd }).out.split("\n");
  const untracked = git(["-c", "core.quotePath=false", "ls-files", "--others", "--exclude-standard"], { cwd }).out.split("\n");
  return [...new Set([...tracked, ...untracked].map((s) => s.trim()).filter(Boolean))].sort();
}
// Uncommitted changes (staged, unstaged, deleted, untracked). Not parsed from `git status --porcelain`:
// run() trims the output, which would cut the leading status column of the first line. With no HEAD yet the diff
// fails with empty output, which leaves the untracked files, as wanted.
const dirtyFiles = (cwd = root) => changedFiles("HEAD", cwd);

// ---------- code repos of a multi-repo product (config "repos")
// The content hash of a file in a repo, or "-" when it is deleted: tells the boss's untouched WIP from a file the task changed.
function fileHash(dir, f) { return fs.existsSync(path.join(dir, f)) ? git(["hash-object", "--", f], { cwd: dir }).out || "?" : "-"; }
function repoStart(r) {
  const baseline = {};
  for (const f of dirtyFiles(r.dir)) baseline[f] = fileHash(r.dir, f);
  return { head: git(["rev-parse", "HEAD"], { cwd: r.dir }).out, branch: git(["rev-parse", "--abbrev-ref", "HEAD"], { cwd: r.dir }).out, baseline };
}
// This task's files in a repo: changed since the task started, minus the boss's uncommitted work from before it.
// A WIP file the task also changed cannot be split safely: it is a conflict the boss resolves.
function repoTaskFiles(r, rs) {
  const files = [], conflicts = [];
  for (const f of changedFiles(rs.head, r.dir)) {
    if (!(f in rs.baseline)) files.push(f);
    else if (fileHash(r.dir, f) !== rs.baseline[f]) conflicts.push(f);
  }
  return { files, conflicts };
}
// The home repo must never track a code repo (git would add it as an embedded repo): each repo path is ignored here.
function ensureRepoIgnores(cfg) {
  // repos older than this ignore line: decompiled third-party evidence (reverse-engineer-app) must never be committed
  const evidence = fs.existsSync(path.join(root, ".craft-anything")) && !git(["check-ignore", "-q", ".craft-anything/"]).ok;
  if (evidence) { ensureGitignore([".craft-anything/"]); console.log("home .gitignore: added .craft-anything/"); }
  const missing = cfg.repos.filter((r) => !git(["check-ignore", "-q", r.path + "/"]).ok).map((r) => r.path + "/");
  if (!missing.length) return evidence;
  const f = path.join(root, ".gitignore"), have = fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "";
  fs.appendFileSync(f, (have && !have.endsWith("\n") ? "\n" : "") + "# code repos of this product (separate git repos)\n" + missing.join("\n") + "\n");
  console.log("home .gitignore: added " + missing.join(", "));
  return true;
}
function taskRepos(cfg, st) {
  return cfg.repos.map((r) => {
    const rs = (st.repos || {})[r.name];
    if (!rs) die("task " + st.id + " started before repo " + r.name + " was configured; ship it, then start a new task.");
    return { ...r, rs, ...repoTaskFiles(r, rs) };
  });
}
function secretFiles(files) {
  return files.filter((f) => {
    const b = f.split("/").pop();
    return (/^\.env(\..+)?$/i.test(b) && !/\.(example|sample|template)$/i.test(b))
      || /\.(jks|keystore|p12|pfx|pem)$/i.test(b) || /^(keystore|local)\.properties$/i.test(b);
  });
}
function graphifyUpdate() {
  if (config().repos.length) return memoryTool("graph"); // per-repo graphs merged into one (memory skill)
  if (!fs.existsSync(path.join(root, "graphify-out", "graph.json"))) return;
  const r = run("graphify", ["update", "."]);
  console.log(r.ok ? "graph updated (graphify, no LLM)" : "note: graphify update failed: " + (r.err || r.out).split("\n").pop());
}
// The code a verify run proves: HEAD's tree, the content hash of every changed or untracked (not ignored) file outside
// .memoir/ (memory and handoffs change between check and ship; anything a verify may read stays in), and the commands.
// Read-only: nothing is staged and `hash-object` without -w writes no object. null (no reuse) when a path cannot be
// hashed, e.g. an untracked nested repo.
// shortcut: ignored files (.env, vendor/, node_modules/) and the database are not in it; `--fresh` after changing them.
function codeStamp(cwd, cmds) {
  const tree = git(["rev-parse", "-q", "--verify", "HEAD^{tree}"], { cwd }).out || "no-commit";
  // no commit yet: every file is a change (changedFiles would miss staged files, which are neither diffed nor untracked)
  const all = tree === "no-commit" ? git(["-c", "core.quotePath=false", "ls-files", "-c", "-o", "--exclude-standard"], { cwd }).out.split("\n").filter(Boolean) : changedFiles("HEAD", cwd);
  const files = [...new Set(all)].sort().filter((f) => !/^\.memoir\//.test(f));
  const present = files.filter((f) => fs.existsSync(path.join(cwd, f)));
  const h = present.length ? git(["hash-object", "--stdin-paths"], { cwd, input: present.join("\n") + "\n", maxBuffer: 256e6 }) : { ok: true, out: "" };
  if (!h.ok) return null;
  const hashes = h.out.split("\n"), of = new Map(present.map((f, i) => [f, hashes[i]]));
  const delta = files.map((f) => f + "\0" + (of.get(f) || "-")).join("\n");
  return crypto.createHash("sha1").update(JSON.stringify(cmds) + "\n" + tree + "\n" + delta).digest("hex");
}
// One verify list, run in `cwd` (the home repo, or a code repo for its own list). `label` prefixes the names.
// A list that passed on exactly this code is not run again (qa's check, then ship): the full suite runs once.
function runVerify(cfg, cwd = root, label = "") {
  if (!cfg.verify.length) return { ok: true, results: [] };
  const sf = path.join(stateDir(), "verify-stamps.json"), stamps = readJson(sf) || {};
  const stamp = codeStamp(cwd, cfg.verify), prev = stamps[cwd];
  if (!argv.includes("--fresh") && stamp && prev && prev.stamp === stamp) {
    console.log("verify " + (label || "home/") + "*: OK, reused (same code passed at " + prev.at + "; --fresh reruns)");
    return { ok: true, results: cfg.verify.map((v) => ({ name: label + v.name, ok: true, reused: true })) };
  }
  const results = [];
  for (const v of cfg.verify) {
    const r = spawnSync(v.run, { cwd, shell: true, encoding: "utf8", timeout: 10 * 60 * 1000 });
    const ok = r.status === 0, name = label + v.name;
    results.push({ name, ok });
    console.log("verify " + name + ": " + (ok ? "OK" : "FAILED") + "  (" + v.run + ")");
    if (!ok) return { ok: false, results, tail: ((r.stdout || "") + (r.stderr || "")).trim().split("\n").slice(-15).join("\n") };
  }
  // stamped only when the code did not change during the run (the pass then proves exactly this code)
  if (stamp && codeStamp(cwd, cfg.verify) === stamp) { stamps[cwd] = { stamp, at: new Date().toISOString().slice(0, 16).replace("T", " ") + " UTC" }; writeJson(sf, stamps); }
  return { ok: true, results };
}
// What a ship must verify depends on what changed. verify (build/lint/test) proves the CODE; a task that changed
// only design docs, memory or framework files (a kickoff, before any code exists) has nothing for it to build,
// so it is skipped and the commit says why.
const NON_CODE = /^(\.memoir\/|\.claude\/)|\.md$|^(\.gitignore|\.gitattributes|\.graphifyignore)$/i;
function verifyFor(cfg, files) {
  const results = [];
  const code = files.filter((f) => !NON_CODE.test(f));
  if (code.length) {
    const v = runVerify(cfg);
    if (!v.ok) return v;
    results.push(...v.results);
  } else console.log("verify skipped: no code changed (docs, memory and framework files only)");
  if (!results.length) results.push({ name: code.length ? "verify (no commands configured in .memoir/project.json)" : "verify (no code changed)", ok: true, skipped: true });
  return { ok: true, results };
}
// A task's verification (ship, and check on a task branch): the home list when code changed there, then each code
// repo whose task files include code.
function verifyTask(cfg, files0, repos) {
  const v = verifyFor(cfg, files0);
  if (!v.ok) return v;
  for (const r of repos) {
    if (!r.files.some((f) => !NON_CODE.test(f))) { if (r.files.length) console.log("verify " + r.name + " skipped: no code changed there"); continue; }
    if (!r.verify.length) { v.results.push({ name: r.name + "/verify (no commands configured)", ok: true, skipped: true }); continue; }
    const rv = runVerify(r, r.dir, r.name + "/");
    if (!rv.ok) return { ...rv, repo: r.name };
    v.results.push(...rv.results);
  }
  return v;
}

// A complex change (3+ code files: the MEDIUM+ line of dev.md) must have gone through a plan and tests. Its plan file
// needs the sections Plan, Test plan and Test results, each with real content (template placeholders do not count).
function planCheck(id, files) {
  const code = files.filter((f) => !NON_CODE.test(f));
  if (code.length < 3) return;
  const f = [path.join(root, ".memoir", "handoff", id + "-dev.md"), path.join(root, ".memoir", "archive", id, id + "-dev.md")].find((p) => fs.existsSync(p));
  const text = f ? fs.readFileSync(f, "utf8").replace(/<!--[\s\S]*?-->/g, "") : "";
  const section = (name) => {
    const m = text.match(new RegExp("^##[ \\t]*" + name + "[ \\t]*$([\\s\\S]*?)(?=^##[ \\t]|(?![\\s\\S]))", "mi"));
    return m ? m[1].split(/\r?\n/).filter((l) => /\w/.test(l) && !/<[^>]+>/.test(l)) : [];
  };
  const missing = ["Plan", "Test plan", "Test results"].filter((n) => !section(n).length);
  if (missing.length) die("complex change (" + code.length + " code files) without " + missing.join(", ") + " in .memoir/handoff/" + id
    + "-dev.md. Plan it, test it and record the results first (dev.md, Every task step 4), then ship again. Nothing was committed.");
}

// Memory upkeep through the memory skill (no LLM, 0 tokens): `maintain` rebuilds .memoir/infra.md, archives cold
// superseded decisions and reports hygiene issues; `infra` only the index. Never blocks a ship.
function memoryTool(cmd) {
  try {
    const s = path.join(root, ".claude", "skills", "memory", "scripts", "memory.mjs");
    if (!fs.existsSync(s)) return;
    const r = spawnSync(process.execPath, [s, cmd], { cwd: root, encoding: "utf8", timeout: 120000 });
    const o = (r.stdout || "").trim();
    if (o) console.log(o);
  } catch { /* fail-open: memory upkeep is a convenience, never a gate */ }
}

// ---------- repository bootstrap: git init + first commit (local and reversible, so no approval)
// App installers (an APK/XAPK kept for analysis) are references, not project code: never in git.
// the same list the init scripts write (a template folder copied by hand gets it here too)
function ensureGitignore(lines = ["graphify-out/", ".playwright-mcp/", ".craft-anything/", ".taste-skill/", ".env", ".env.*", "!.env.example", "*.apk", "*.xapk", "*.apks", "*.aab", "*.ipa"]) {
  const f = path.join(root, ".gitignore");
  const have = fs.existsSync(f) ? fs.readFileSync(f, "utf8").split(/\r?\n/) : [];
  const add = lines.filter((l) => !have.includes(l));
  if (add.length) fs.appendFileSync(f, (have.length && have[have.length - 1] !== "" ? "\n" : "") + add.join("\n") + "\n");
}
function ensureRepo() {
  const cfg = config();
  const inside = git(["rev-parse", "--is-inside-work-tree"]);
  if (/ENOENT/.test(inside.err)) die("git is not installed or not on PATH. The boss installs Git for Windows (option: add to PATH), then asks again.");
  if (!inside.ok) {
    if (!git(["init", "-q", "-b", cfg.base]).ok) { mustGit(["init", "-q"]); mustGit(["symbolic-ref", "HEAD", "refs/heads/" + cfg.base]); }
    console.log("created a local git repository (branch " + cfg.base + ")");
  }
  if (!git(["rev-parse", "--verify", "HEAD"]).ok) {
    if (!git(["config", "user.name"]).out || !git(["config", "user.email"]).out) {
      die("git does not know who makes the commits on this machine. The boss runs once:\n  git config --global user.name \"<nama>\"\n  git config --global user.email \"<email GitHub>\"\nthen asks again.");
    }
    ensureGitignore();
    ensureRepoIgnores(cfg);
    // graphify hooks BEFORE the first commit: hook install edits .gitattributes, which must be in the commit
    if (run("graphify", ["--help"]).ok) {
      if (!fs.existsSync(path.join(root, "graphify-out", "graph.json"))) run("graphify", ["update", "."]);
      const h = run("graphify", ["hook", "install"]);
      console.log(h.ok ? "graphify git hooks installed (graph rebuilds after each commit, no LLM)" : "note: graphify hook install failed");
    }
    const files = git(["ls-files", "--others", "--exclude-standard"]).out.split("\n").filter(Boolean);
    const secrets = secretFiles(files);
    if (secrets.length) die("these secret files would be committed; add them to .gitignore first:\n  " + secrets.join("\n  "));
    mustGit(["add", "-A"]);
    mustGit(["commit", "-q", "--allow-empty", "-m", "[setup] initial commit"], "first commit");
    console.log("first commit created on " + currentBranch() + " (" + files.length + " files; .env and graphify-out/ ignored)");
  }
}
// init-repo only: commit the framework's own files (installed or updated by the init/update scripts),
// staged BY NAME so the boss's own uncommitted work is never included.
const SETUP_PATHS = [".claude", ".memoir", "CLAUDE.md", "CLAUDE.kit.md", "AGENTS.md", ".mcp.json", ".graphifyignore", ".gitignore", ".gitattributes",
  // present when the whole template folder was copied into the project instead of using init-project
  "scripts/init-project.ps1", "scripts/init-project.sh"];
function setupCommit() {
  // Never while a task is open: its branch holds the agents' work in progress (memory, handoffs); framework
  // updates made now simply ship with that task. Handoffs and archives are never setup files.
  const cur = currentBranch();
  if (cur.startsWith("task/")) { console.log("task branch " + cur + " is open: framework changes will ship with that task (nothing committed now)."); return; }
  const dirty = dirtyFiles().filter((f) => SETUP_PATHS.some((p) => f === p || f.startsWith(p + "/"))
    && !f.startsWith(".memoir/handoff/") && !f.startsWith(".memoir/archive/"));
  if (!dirty.length) return;
  const secrets = secretFiles(dirty);
  if (secrets.length) die("secret files among the setup files: " + secrets.join(", "));
  mustGit(["add", "--", ...dirty]);
  mustGit(["commit", "-q", "-m", "[setup] agent team files"], "setup commit");
  console.log("committed " + dirty.length + " framework file(s) as [setup] agent team files");
}

// ---------- start
function start(titleWords) {
  ensureRepo();
  const sd = stateDir(), cfg = config();
  const cur = currentBranch(), open = taskOf(cur);
  if (open) { console.log("already on task " + open + " (" + cur + "); keep working on it or ship it first."); return; }
  const ignoresAdded = ensureRepoIgnores(cfg);
  const outside = dirtyFiles().filter((f) => !f.startsWith(".memoir/") && !(ignoresAdded && f === ".gitignore"));
  if (outside.length) die("uncommitted changes outside .memoir/ (commit or stash them yourself first):\n  " + outside.join("\n  "));
  if (cur !== cfg.base) die("start new tasks from " + cfg.base + " (you are on " + cur + "). Change \"base\" in .memoir/project.json if needed.");
  const cf = path.join(sd, "counter");
  let id = (parseInt(fs.existsSync(cf) ? fs.readFileSync(cf, "utf8") : "0", 10) || 0) + 1;
  while (git(["rev-parse", "--verify", "refs/heads/" + branchOf(id)]).ok) id++;
  fs.writeFileSync(cf, String(id));
  id = String(id);
  mustGit(["switch", "-c", branchOf(id)]);
  const repos = {};
  for (const r of cfg.repos) {
    repos[r.name] = repoStart(r);
    const n = Object.keys(repos[r.name].baseline).length;
    console.log("repo " + r.name + " (" + r.path + ") on " + repos[r.name].branch + (n ? ": " + n + " uncommitted file(s) from before this task are the boss's work; ship never commits them" : ""));
  }
  writeJson(path.join(sd, id + ".json"), { id, title: titleWords.join(" ").trim() || "task " + id, base: cfg.base,
    startCommit: mustGit(["rev-parse", "HEAD"]), startedAt: new Date().toISOString(), ...(cfg.repos.length ? { repos } : {}) });
  console.log("task " + id + " started on " + branchOf(id) + " (local, no approval needed)");
}

// ---------- resume (abort keeps the branch)
function resume(id) {
  if (!ID_RE.test(id || "")) die("usage: task.mjs start --resume <id>");
  if (dirtyFiles().filter((f) => !f.startsWith(".memoir/")).length) die("uncommitted changes outside .memoir/; commit or stash them first.");
  mustGit(["switch", branchOf(id)]);
  console.log("resumed task " + id);
}

// ---------- status / check
function status() {
  const sd = stateDir(), cur = currentBranch(), id = taskOf(cur);
  if (!id) { console.log("no active task (on " + cur + ")"); return; }
  const st = readJson(path.join(sd, id + ".json"));
  console.log("task " + id + ": " + (st ? st.title : "?") + "\nbranch " + cur);
  const files = st ? changedFiles(st.startCommit) : dirtyFiles();
  console.log("changed files (" + files.length + "):" + (files.length ? "\n  " + files.join("\n  ") : " none"));
  let repoFiles = 0;
  if (st && st.repos) for (const r of taskRepos(config(), st)) {
    repoFiles += r.files.length;
    console.log("repo " + r.name + " (" + r.path + "), task files (" + r.files.length + "):" + (r.files.length ? "\n  " + r.files.join("\n  ") : " none")
      + (r.conflicts.length ? "\n  CONFLICT (boss's uncommitted work also changed by this task): " + r.conflicts.join(", ") : ""));
  }
  if (!st || !(files.length + repoFiles)) return;
  console.log("ship: the verification commands run first; nothing is committed or sent if they fail");
}
function check() {
  const cfg = config();
  if (!cfg.verify.length && !cfg.repos.some((r) => r.verify.length)) { console.log("no verification commands in .memoir/project.json"); return; }
  const id = taskOf(git(["symbolic-ref", "--short", "HEAD"]).out), st = id && readJson(path.join(stateDir(), id + ".json")); // symbolic-ref: also on an unborn branch
  if (st) { // on a task: what ship will verify, so qa's check and the ship share one run
    const v = verifyTask(cfg, changedFiles(st.startCommit), st.repos ? taskRepos(cfg, st) : []);
    if (!v.ok) die("verification failed" + (v.repo ? " in " + v.repo : "") + ":\n" + v.tail);
    return;
  }
  const v = runVerify(cfg);
  if (!v.ok) die("verification failed:\n" + v.tail);
  for (const r of cfg.repos) { const rv = runVerify(r, r.dir, r.name + "/"); if (!rv.ok) die("verification failed in " + r.name + ":\n" + rv.tail); }
}

// ---------- ship (the single approval) / finish
function prNumber(br) {
  const r = run("gh", ["pr", "view", br, "--json", "number", "-q", ".number"]);
  return r.ok && r.out ? r.out : null;
}
function waitChecks(n, minutes) {
  const startedAt = Date.now();
  const until = startedAt + minutes * 60 * 1000;
  // Right after a PR is opened GitHub may not have registered its checks yet. If the project has
  // workflows, "no checks" counts as pending for a grace period instead of "no CI".
  const wf = path.join(root, ".github", "workflows");
  const expectChecks = fs.existsSync(wf) && fs.readdirSync(wf).some((f) => /\.ya?ml$/i.test(f));
  for (;;) {
    const r = run("gh", ["pr", "checks", n, "--json", "bucket,name"]);
    let list = null;
    if (!r.ok && /no checks/i.test(r.err + r.out)) list = [];
    else { try { list = JSON.parse(r.out); } catch { /* not ready yet */ } }
    if (Array.isArray(list)) {
      if (!list.length) {
        if (!expectChecks || Date.now() - startedAt >= 90 * 1000) return "none";
        if (Date.now() > until) return "pending";
        sleep(10000);
        continue;
      }
      const bad = list.filter((c) => ["fail", "cancel"].includes(c.bucket));
      if (bad.length) return "fail:" + bad.map((c) => c.name).join(", ");
      if (list.every((c) => ["pass", "skipping"].includes(c.bucket))) return "pass";
    }
    if (Date.now() > until) return "pending";
    sleep(15000);
  }
}
const vsum = (results) => results.map((x) => x.name + (x.skipped ? " SKIPPED" : x.reused ? " OK (reused pass, same code)" : x.ok ? " OK" : " FAILED")).join(", ");
// The commit message goes in on stdin (`-F -`): no temp file to write and clean up.
const commitWithMessage = (msg, args, cwd = root) => git(["commit", "-q", "-F", "-", ...args], { cwd, input: msg });
function prBody(id, st, files, verify) {
  const f = path.join(root, ".memoir", "archive", id, id + "-pr.md");
  if (fs.existsSync(f)) {
    const lines = fs.readFileSync(f, "utf8").split(/\r?\n/);
    const i = lines.findIndex((l) => l.trim());
    return { title: lines[i].replace(/^#+\s*/, "").trim().slice(0, 100), body: lines.slice(i + 1).join("\n").trim() };
  }
  const v = verify.length ? vsum(verify) : "none configured";
  return { title: st.title, body: "Files:\n- " + files.join("\n- ") + "\n\nVerification: " + v };
}
function ship(id, evidence) {
  const sd = stateDir(), cfg = config(), br = branchOf(id);
  if (!MODES.includes(cfg.releaseMode)) die("releaseMode must be one of " + MODES.join(", "));
  if (currentBranch() !== br) die("not on " + br + ".");
  const st = readJson(path.join(sd, id + ".json"));
  if (!st) die("no state for task " + id + ".");
  if (cfg.releaseMode !== "commit-only" && !git(["remote", "get-url", "origin"]).ok) die("releaseMode " + cfg.releaseMode + " needs a remote named origin (or set releaseMode to commit-only). Nothing was sent.");
  if (["pr", "merge"].includes(cfg.releaseMode) && !run("gh", ["auth", "status"]).ok) die("the GitHub CLI is missing or not logged in (the boss runs: gh auth login). Nothing was sent.");
  const approval = "dev (verification above)";

  memoryTool("maintain"); // infra.md, archive cold superseded decisions, hygiene report (local, 0 tokens); fail-open
  const files0 = changedFiles(st.startCommit);
  const repos = st.repos ? taskRepos(cfg, st) : [];
  const conflicts = repos.flatMap((r) => r.conflicts.map((f) => r.path + "/" + f));
  if (conflicts.length) die("these files were the boss's uncommitted work when the task started and the task changed them too; ship cannot split them. The boss commits or stashes his part, then ship again. Nothing was committed:\n  " + conflicts.join("\n  "));
  if (!files0.length && !repos.some((r) => r.files.length)) die("nothing changed on " + br + (repos.length ? " or in " + repos.map((r) => r.name).join(", ") : "") + ".");
  const secrets = [...secretFiles(files0), ...repos.flatMap((r) => secretFiles(r.files).map((f) => r.path + "/" + f))];
  if (secrets.length) die("secret files would be committed; add them to .gitignore. Nothing was sent:\n  " + secrets.join("\n  "));
  planCheck(id, [...files0, ...repos.flatMap((r) => r.files.map((f) => r.path + "/" + f))]);
  const v = verifyTask(cfg, files0, repos);
  if (!v.ok) die("verification failed" + (v.repo ? " in " + v.repo : "") + ". Nothing was committed or sent. Fix it and ship again.\n" + v.tail);

  // archive this task's handoffs only once verification passed (a failed ship leaves everything in place)
  const hd = path.join(root, ".memoir", "handoff"), ad = path.join(root, ".memoir", "archive", id);
  if (fs.existsSync(hd)) for (const f of fs.readdirSync(hd).filter((f) => f.startsWith(id + "-") || f.startsWith("task-" + id + "-"))) {
    fs.mkdirSync(ad, { recursive: true });
    fs.renameSync(path.join(hd, f), path.join(ad, f));
  }
  const files = changedFiles(st.startCommit);

  const pb = prBody(id, st, [...files, ...repos.flatMap((r) => r.files.map((f) => r.path + "/" + f))], v.results);
  const title = pb.title;
  const vline = "Verification: " + vsum(v.results);
  const body = pb.body + (/^Verification:/m.test(pb.body) ? "" : "\n\n" + vline) + (evidence ? "\n\nAcceptance evidence: " + evidence : "") + "\n\nApproval: " + approval;
  // Code repos first, each on the branch it has checked out, only this task's files (never the boss's earlier WIP).
  const repoCommits = {}, msg = "[task-" + id + "] " + title + "\n\n" + body + "\n";
  for (const r of repos.filter((x) => x.files.length)) {
    const already = Object.keys(repoCommits).length ? "\nAlready committed: " + JSON.stringify(repoCommits) : "";
    const a = git(["add", "-A", "--", ...r.files], { cwd: r.dir });
    if (!a.ok) die("staging in " + r.name + " failed: " + a.err + already);
    const c = commitWithMessage(msg, ["--", ...r.files], r.dir);
    if (!c.ok) die("commit in " + r.name + " failed: " + (c.err || c.out) + already);
    repoCommits[r.name] = git(["rev-parse", "HEAD"], { cwd: r.dir }).out;
    console.log("committed " + repoCommits[r.name].slice(0, 8) + " in " + r.name + " (" + r.path + ", branch " + git(["rev-parse", "--abbrev-ref", "HEAD"], { cwd: r.dir }).out + "): " + r.files.length + " file(s); push/merge stays with the boss");
  }
  if (Object.keys(repoCommits).length) {
    fs.mkdirSync(ad, { recursive: true });
    writeJson(path.join(ad, id + "-commits.json"), repoCommits); // where this task's code went, kept with its memory
  }
  ensureRepoIgnores(cfg);
  mustGit(["add", "-A"]);
  const staged = git(["diff", "--cached", "--name-only"]).out.split("\n").filter(Boolean);
  if (secretFiles(staged).length) { git(["reset", "-q"]); die("secret files staged; nothing was sent."); }
  if (staged.length) { const c = commitWithMessage(msg, []); if (!c.ok) die("commit failed: " + (c.err || c.out)); }
  const p = { id, title, body, base: st.base, releaseMode: cfg.releaseMode, mergeMethod: cfg.mergeMethod, files };
  writeJson(path.join(sd, id + "-plan.json"), p);
  const approved = { id, commit: mustGit(["rev-parse", "HEAD"]), at: new Date().toISOString(), pushed: false, merged: false };
  writeJson(path.join(sd, id + "-approved.json"), approved);
  console.log("committed " + approved.commit.slice(0, 8) + " [task-" + id + "] " + title + "  (approval: " + approval + ")");
  deliver(id, p, approved);
}
function deliver(id, p, approved) {
  const br = branchOf(id), sd = stateDir();
  if (p.releaseMode === "commit-only") return localMerge(id, p.base, br);
  if (!approved.pushed) {
    mustGit(["push", "-u", "origin", br], "push");
    approved.pushed = true; writeJson(path.join(sd, id + "-approved.json"), approved);
    console.log("pushed " + br);
  }
  if (p.releaseMode === "push") return backToBase(id, p.base, "pushed " + br);
  let n = prNumber(br);
  if (!n) {
    const r = run("gh", ["pr", "create", "--base", p.base, "--head", br, "--title", p.title, "--body", p.body]);
    if (!r.ok) die("opening the pull request failed: " + r.err);
    n = prNumber(br);
    console.log("pull request: " + r.out);
  }
  if (p.releaseMode === "pr") return backToBase(id, p.base, "pull request #" + n + " is open");
  // read checks only once GitHub shows the approved commit as the PR head (never old results)
  for (let i = 0; i < 12; i++) {
    if (run("gh", ["pr", "view", n, "--json", "headRefOid", "-q", ".headRefOid"]).out === approved.commit) break;
    if (i === 11) die("GitHub does not show commit " + approved.commit.slice(0, 8) + " on PR #" + n + " yet; run finish " + id + " later.");
    sleep(5000);
  }
  const checks = waitChecks(n, config().ciWaitMinutes);
  if (checks === "pending") { console.log("CI is still running. Later run: node .claude/scripts/task.mjs finish " + id + " (no new approval needed)"); return; }
  if (checks.startsWith("fail")) die("CI failed (" + checks.slice(5) + "). Not merged. Fix on the task branch and ship again (new approval).");
  const method = { squash: "--squash", merge: "--merge", rebase: "--rebase" }[p.mergeMethod] || "--squash";
  const r = run("gh", ["pr", "merge", n, method, "--delete-branch"]);
  if (!r.ok) die("merge failed: " + r.err);
  approved.merged = true; writeJson(path.join(sd, id + "-approved.json"), approved);
  backToBase(id, p.base, "merged pull request #" + n + " into " + p.base + " (checks: " + checks + ")");
}
// commit-only has no remote, so it finishes the task LOCALLY: fast-forward base to the task branch (or a merge
// commit if base moved on). This keeps base current so the next `start` branches from the shipped work, with no
// manual git for the boss. The task branch is kept (nothing is lost).
function localMerge(id, base, br) {
  if (currentBranch() !== base) mustGit(["switch", base]);
  if (!git(["merge", "--ff-only", br]).ok) {
    const mg = git(["merge", "--no-edit", br]);
    if (!mg.ok) {
      git(["merge", "--abort"]);
      die("could not merge " + br + " into " + base + " locally (base moved and the merge conflicts). Resolve it, or switch releaseMode to merge once a GitHub remote exists.\n" + (mg.err || mg.out).trim());
    }
  }
  graphifyUpdate();
  console.log("DONE: task " + id + " merged into " + base + " locally (commit-only). Branch " + br + " kept.\nNext task: /clear first (a fresh context is cheaper). Usage: node .claude/scripts/task.mjs usage " + id);
}
function backToBase(id, base, msg) {
  if (currentBranch() !== base) mustGit(["switch", base]);
  if (git(["remote", "get-url", "origin"]).ok) { git(["pull", "--ff-only", "origin", base]); git(["fetch", "--prune", "origin"]); }
  graphifyUpdate();
  console.log("DONE: " + msg + "\nNext task: /clear first (a fresh context is cheaper). Usage: node .claude/scripts/task.mjs usage " + id);
}
function finish(id) {
  const sd = stateDir();
  const p = readJson(path.join(sd, id + "-plan.json"));
  const a = readJson(path.join(sd, id + "-approved.json"));
  if (!p || !a) die("task " + id + " has no approved ship.");
  if (a.merged) { console.log("already merged"); return; }
  const head = git(["rev-parse", "refs/heads/" + branchOf(id)]);
  if (head.ok && head.out !== a.commit) die("the task branch moved after the approval; ship again.");
  if (head.ok && currentBranch() !== branchOf(id)) mustGit(["switch", branchOf(id)]);
  deliver(id, p, a);
}

// ---------- abort
function abort(id, del) {
  const sd = stateDir(), cfg = config(), br = branchOf(id);
  const st = readJson(path.join(sd, id + ".json"));
  const base = (st && st.base) || cfg.base;
  if (currentBranch() === br) {
    const dirty = dirtyFiles();
    if (dirty.length) {
      if (secretFiles(dirty).length) die("secret files in the working tree. Add them to .gitignore first.");
      mustGit(["add", "-A"]);
      mustGit(["commit", "-q", "-m", "[task-" + id + "] WIP (aborted)"]);
      console.log("saved work as a WIP commit on " + br);
    }
    mustGit(["switch", base]);
  }
  if (st && st.repos) for (const r of taskRepos(cfg, st)) if (r.files.length)
    console.log("repo " + r.name + ": " + r.files.length + " changed file(s) stay uncommitted in the working tree (" + r.path + "); nothing is committed in a code repo on abort");
  if (del) {
    git(["branch", "-D", br]);
    for (const f of [id + ".json", id + "-plan.json", id + "-approved.json"]) fs.rmSync(path.join(sd, f), { force: true });
    console.log("deleted " + br);
  } else console.log("back on " + base + "; " + br + " is kept (resume: task.mjs start --resume " + id + ")");
}

// ---------- deploy (web projects with .github/workflows/deploy.yml; dev asks the boss in chat before running it)
// Dispatches the deploy workflow for the commit the boss approved (default: origin/<base> as fetched by
// the last ship), then follows the run. The workflow reuses a green ci run of that commit on main (binary
// templates) or re-runs the tests (always with --retest), and the server runs deploy.sh --ref <sha>. Never deploys from a task branch.
function deploy(argv) {
  const sd = stateDir(), cfg = config();
  if (!fs.existsSync(path.join(root, ".github", "workflows", "deploy.yml"))) die("no .github/workflows/deploy.yml; this project has no deploy set up (see the web-deploy skill).");
  const rollback = argv.includes("--rollback");
  const i = argv.indexOf("--sha");
  let sha = i >= 0 ? argv[i + 1] || "" : "";
  if (!rollback) {
    const want = sha || "origin/" + cfg.base;
    if (sha && !/^[0-9a-f]{4,40}$/.test(sha)) die("--sha must be a commit sha (got: " + sha + ")");
    // actions/checkout needs the full 40-char sha: a short one fails after its retries
    sha = git(["rev-parse", "--verify", "--quiet", want + "^{commit}"]).out;
    if (!/^[0-9a-f]{40}$/.test(sha)) die("no commit " + want + " in this repo to deploy");
  }
  if (!run("gh", ["auth", "status"]).ok) die("the GitHub CLI is missing or not logged in (the boss runs: gh auth login).");
  const since = new Date(Date.now() - 60 * 1000).toISOString();
  const args = ["workflow", "run", "deploy.yml", "--ref", cfg.base, ...(rollback ? ["-f", "rollback=true"] : ["-f", "sha=" + sha, ...(argv.includes("--retest") ? ["-f", "retest=true"] : [])])];
  const r = run("gh", args);
  if (!r.ok) die("starting the deploy workflow failed: " + (r.err || r.out));
  let id = null;
  for (let k = 0; k < 12 && !id; k++) {
    sleep(5000);
    const l = run("gh", ["run", "list", "--workflow", "deploy.yml", "--event", "workflow_dispatch", "--limit", "5", "--json", "databaseId,createdAt"]);
    try { const runs = JSON.parse(l.out).filter((x) => x.createdAt >= since); if (runs.length) id = String(runs[0].databaseId); } catch { /* retry */ }
  }
  if (!id) { console.log("deploy started; its run is not listed yet. Check: gh run list --workflow deploy.yml"); return; }
  writeJson(path.join(sd, "deploy-pending.json"), { sha: rollback ? "rollback" : sha, run: id, at: new Date().toISOString() });
  console.log((rollback ? "rollback" : "deploy of " + sha.slice(0, 8)) + " started: " + run("gh", ["run", "view", id, "--json", "url", "-q", ".url"]).out);
  deployStatus(id);
}
function deployStatus(id) {
  const sd = stateDir(), cfg = config();
  const pend = readJson(path.join(sd, "deploy-pending.json")) || {};
  id = id || pend.run;
  if (!id) die("no deploy run to follow.");
  const until = Date.now() + cfg.deployWaitMinutes * 60 * 1000;
  for (;;) {
    let s = {};
    try { s = JSON.parse(run("gh", ["run", "view", id, "--json", "status,conclusion"]).out); } catch { /* retry */ }
    if (s.status === "completed") {
      fs.rmSync(path.join(sd, "deploy-pending.json"), { force: true });
      if (s.conclusion !== "success") die("deploy run " + id + " ended: " + s.conclusion + ". Details: gh run view " + id + " --log-failed. Rollback needs the boss: task.mjs deploy --rollback");
      if (pend.sha && pend.sha !== "rollback") writeJson(path.join(sd, "deploy.json"), { sha: pend.sha, run: id, at: new Date().toISOString() });
      console.log("DEPLOYED: run " + id + " succeeded" + (pend.sha ? " (" + pend.sha.slice(0, 8) + ")" : ""));
      return;
    }
    if (Date.now() > until) { console.log("deploy still running. Later: node .claude/scripts/task.mjs deploy-status " + id); return; }
    sleep(15000);
  }
}

// ---------- usage (written by .claude/hooks/usage.mjs, zero tokens)
// Tokens come from the usage hook. Dollars come from each finished session's cost-state entry, split over that
// session's tasks by weighted tokens (input 1, cache write 2, cache read 0.1, output 5: relative prices only).
const money = (x) => "$" + x.toFixed(3);
const WEIGHT = { input: 1, cacheWrite: 2, cacheRead: 0.1, output: 5 };
const weighted = (u) => Object.entries(WEIGHT).reduce((s, [k, w]) => s + (u[k] || 0) * w, 0);
function usageRows() {
  const f = path.join(stateDir(), "usage.jsonl");
  if (!fs.existsSync(f)) return [];
  return fs.readFileSync(f, "utf8").split("\n").filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
}
function sessionCost(transcript) {
  // The last cost-state line sits near the end: read the last 4 MB first, the whole file only when it is not there.
  const last = (text) => {
    const lines = text.split("\n");
    for (let i = lines.length - 1; i >= 0; i--) {
      if (!lines[i].includes('"cost-state"')) continue;
      let o; try { o = JSON.parse(lines[i]); } catch { continue; } // the first line of a tail can be cut
      if (o.type === "cost-state" && o.modelUsage) return Object.fromEntries(Object.entries(o.modelUsage).map(([m, u]) => [m, u.costUSD || 0]));
    }
    return null;
  };
  try {
    const size = fs.statSync(transcript).size, n = Math.min(size, 4 * 1024 * 1024);
    if (n < size) {
      const fd = fs.openSync(transcript, "r"), buf = Buffer.alloc(n);
      try { fs.readSync(fd, buf, 0, n, size - n); } finally { fs.closeSync(fd); }
      const hit = last(buf.toString("utf8"));
      if (hit) return hit;
    }
    return last(fs.readFileSync(transcript, "utf8"));
  } catch { /* transcript gone or format changed */ }
  return null;
}
function costed(rows) {
  const sessions = {};
  for (const r of rows) {
    const s = sessions[r.session] || (sessions[r.session] = { transcript: r.transcript, w: {} });
    for (const [m, u] of Object.entries(r.models || {})) s.w[m] = (s.w[m] || 0) + weighted(u);
  }
  for (const s of Object.values(sessions)) s.cost = sessionCost(s.transcript);
  return rows.map((r) => {
    const s = sessions[r.session];
    let cost = s.cost ? 0 : null;
    if (s.cost) for (const [m, u] of Object.entries(r.models || {})) if (s.w[m]) cost += (s.cost[m] || 0) * weighted(u) / s.w[m];
    return { ...r, cost };
  });
}
function aggregate(rows) {
  const t = { cost: 0, open: false, models: {} };
  for (const r of rows) {
    if (r.cost === null) t.open = true; else t.cost += r.cost;
    for (const [m, u] of Object.entries(r.models || {})) {
      const k = t.models[m] || (t.models[m] = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 });
      for (const x of Object.keys(k)) k[x] += u[x] || 0;
    }
  }
  return t;
}
function line(label, a) {
  const parts = Object.entries(a.models).sort((x, y) => weighted(y[1]) - weighted(x[1]))
    .map(([m, u]) => m.replace(/^claude-/, "") + ": out " + u.output + ", cache-write " + u.cacheWrite + ", cache-read " + u.cacheRead);
  return label + ": " + money(a.cost) + (a.open ? " + open session (cost known after it ends)" : "") + "  | " + parts.join(" | ");
}
function usage(arg) {
  const rows = costed(usageRows());
  if (!rows.length) { console.log("no usage recorded yet (the usage hook writes it after each turn)."); return; }
  const tasks = arg && arg !== "all" ? [arg] : [...new Set(rows.map((r) => r.task))];
  for (const t of tasks) console.log(line(t === "none" ? "outside tasks" : "task " + t, aggregate(rows.filter((r) => r.task === t))));
  if (tasks.length > 1) console.log(line("total", aggregate(rows)));
}

switch (sub) {
  case "start": argv[0] === "--resume" ? resume(argv[1]) : start(argv); break;
  case "init-repo": ensureRepo(); setupCommit(); console.log("repository ready"); break;
  case "check": check(); break;
  case "infra": memoryTool("infra"); break;
  case "status": status(); break;
  case "ship": {
    const i = argv.indexOf("--evidence");
    ship(taskId(argv[0]), i >= 0 ? (argv[i + 1] || "").slice(0, 500) : "");
    break;
  }
  case "finish": finish(taskId(argv[0])); break;
  case "abort": abort(taskId(argv[0]), argv.includes("--delete")); break;
  case "usage": usage(argv[0]); break;
  case "deploy": deploy(argv); break;
  case "deploy-status": deployStatus(argv[0]); break;
  default:
    console.log("usage: node .claude/scripts/task.mjs <start|init-repo|check|status|ship|finish|abort|usage|deploy|deploy-status|infra> ...");
    process.exit(sub ? 1 : 0);
}
