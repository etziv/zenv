#!/usr/bin/env node
// PostToolUse hook (every tool), per agent (main session and each subagent apart). Zero tokens unless it fires:
// - loop: the same tool with the same input 5 times in a row -> the result will not change; rethink or report the blocker.
// - scope: this agent changed 20 (then 40, 60, ...) different files -> check the work against the plan.
// Idea from ECC's context monitor (LOOP_THRESHOLD, FILES_WARNING_COUNT). State in <git dir>/memoir/watch.json.
// Fail-open: on any error it prints nothing.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

try {
  const p = JSON.parse(fs.readFileSync(0, "utf8") || "{}");
  const root = process.env.CLAUDE_PROJECT_DIR || p.cwd || process.cwd();
  let gd = path.join(root, ".git");
  if (fs.existsSync(gd) && fs.statSync(gd).isFile()) { const m = fs.readFileSync(gd, "utf8").match(/gitdir:\s*(.+)/); gd = m ? path.resolve(root, m[1].trim()) : ""; }
  if (!gd || !fs.existsSync(gd)) process.exit(0);
  const sf = path.join(gd, "memoir", "watch.json");
  let all = {}; try { all = JSON.parse(fs.readFileSync(sf, "utf8")); } catch { /* first call */ }
  const key = (p.session_id || "?") + "/" + (p.agent_id || "main");
  const st = all[key] || { last: [], warned: [], files: [], filesWarned: 0, at: 0 };
  st.at = Date.now(); all[key] = st;

  const input = { ...(p.tool_input || {}) }; delete input.description; delete input.timeout;
  const stable = (v) => (v && typeof v === "object" && !Array.isArray(v) ? Object.keys(v).sort().reduce((o, k) => ((o[k] = stable(v[k])), o), {}) : v);
  const sig = (p.tool_name || "?") + ":" + crypto.createHash("sha1").update(JSON.stringify(stable(input))).digest("hex").slice(0, 12);
  st.last = [...st.last, sig].slice(-5);
  const msgs = [];
  if (st.last.length === 5 && st.last.every((s) => s === sig) && !st.warned.includes(sig)) {
    st.warned = [...st.warned, sig].slice(-20);
    msgs.push("watch: you ran the same " + p.tool_name + " call 5 times in a row with the same input; its result will not change. Stop and rethink: read the last error or output, change the approach, or report the blocker. (Waiting on a server or a job: wait longer between checks.)");
  }
  if (/^(Edit|Write|MultiEdit|NotebookEdit)$/.test(p.tool_name || "") && input.file_path) {
    const f = String(input.file_path).split("\\").join("/").toLowerCase();
    if (!st.files.includes(f)) st.files.push(f);
    const step = Math.floor(st.files.length / 20) * 20;
    if (step >= 20 && step > st.filesWarned) {
      st.filesWarned = step;
      msgs.push("watch: this agent has changed " + st.files.length + " different files. Check them against the plan's Files list and the request: anything outside it is scope creep to drop, or a plan change to write down first.");
    }
  }
  for (const k of Object.keys(all).sort((a, b) => all[b].at - all[a].at).slice(40)) delete all[k]; // keep the 40 newest agents
  fs.mkdirSync(path.dirname(sf), { recursive: true });
  // tmp + rename: every agent of every session shares this file, a torn write would reset everyone's tracking
  const tmp = sf + "." + process.pid + ".tmp"; fs.writeFileSync(tmp, JSON.stringify(all)); fs.renameSync(tmp, sf);
  if (msgs.length) process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "PostToolUse", additionalContext: msgs.join("\n") } }));
} catch { /* never disturb the session */ }
process.exit(0);
