#!/usr/bin/env node
// Stop hook: records token use per task and model into <git dir>/memoir/usage.jsonl. Zero model tokens: its only
// output is a systemMessage to the boss when the main context grows past 500K (then every further 150K), after
// `memory.mjs capture` has saved the working position so he can /clear at once.
// Sums the session transcript and its subagent transcripts (one entry per message id, last one wins) and appends the
// delta since the previous turn, attributed to the task branch that is checked out. Dollar cost is added later by
// "task.mjs usage" from the transcript's cost-state entry (written when the session ends).
// The transcript format is internal to Claude Code and may change between versions: on any error this does nothing.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

try {
  const p = JSON.parse(fs.readFileSync(0, "utf8") || "{}");
  const root = process.env.CLAUDE_PROJECT_DIR || p.cwd || process.cwd();
  const gd = path.join(root, ".git");
  if (!p.session_id || !fs.existsSync(gd) || !fs.statSync(gd).isDirectory()) process.exit(0);
  let tp = p.transcript_path;
  if (!tp || !fs.existsSync(tp)) tp = path.join(os.homedir(), ".claude", "projects", root.replace(/[^A-Za-z0-9]/g, "-"), p.session_id + ".jsonl");
  if (!fs.existsSync(tp)) process.exit(0);

  const files = [tp];
  const subDir = path.join(tp.replace(/\.jsonl$/, ""), "subagents");
  if (fs.existsSync(subDir)) for (const f of fs.readdirSync(subDir)) if (f.endsWith(".jsonl")) files.push(path.join(subDir, f));

  const sd = path.join(gd, "memoir");
  fs.mkdirSync(path.join(sd, "usage-state"), { recursive: true });
  const snapFile = path.join(sd, "usage-state", p.session_id + ".json");
  let st = null;
  try { st = JSON.parse(fs.readFileSync(snapFile, "utf8")); } catch { /* first turn */ }
  const prev = st && st.v === 2 ? st.total : st || {}; // v1 state: the totals alone
  // Only the bytes added since the last turn are read (a 270 MB transcript took ~1.9 s per turn read whole). A
  // message id repeats only on the line right after itself (streamed updates; 0 exceptions in 25,701 rows), so the
  // last row of each file is kept to replace its usage when the next read starts with the same id. Anything unusual
  // (no v2 state, a file that shrank, an id seen in two files) falls back to reading everything, as before.
  const add = (t, model, u, sign) => {
    const k = t[model] || (t[model] = { input: 0, cacheWrite: 0, cacheRead: 0, output: 0 });
    k.input += sign * (u.input_tokens || 0); k.cacheWrite += sign * (u.cache_creation_input_tokens || 0);
    k.cacheRead += sign * (u.cache_read_input_tokens || 0); k.output += sign * (u.output_tokens || 0);
  };
  const ctxOf = (u) => (u.input_tokens || 0) + (u.cache_read_input_tokens || 0) + (u.cache_creation_input_tokens || 0);
  const rows = (text, each) => {
    for (const line of text.split("\n")) {
      if (!line.includes('"usage"')) continue;
      let o; try { o = JSON.parse(line); } catch { continue; }
      const m = o && o.type === "assistant" && o.message;
      if (m && m.usage && m.model) each(m.id || o.requestId || o.uuid, m.model, m.usage);
    }
  };
  const full = () => { // the whole history: one entry per message id, the last one wins
    const byId = new Map(), fstate = {}; let ctx = 0;
    for (const f of files) {
      const text = fs.readFileSync(f, "utf8"), end = text.lastIndexOf("\n") + 1, fs_ = (fstate[f] = { off: Buffer.byteLength(text.slice(0, end)), last: null });
      rows(text.slice(0, end), (id, model, u) => { byId.set(id, { model, u }); fs_.last = { id, model, u }; if (f === tp && model !== "<synthetic>") ctx = ctxOf(u); });
    }
    const t = {}; for (const { model, u } of byId.values()) add(t, model, u, 1);
    return { total: t, files: fstate, mainCtx: ctx };
  };
  let now = null;
  if (st && st.v === 2) {
    try {
      const t = JSON.parse(JSON.stringify(st.total)), fstate = { ...st.files }, seen = new Set();
      let ctx = st.mainCtx || 0, ok = true;
      for (const f of files) {
        const size = fs.statSync(f).size, s = fstate[f] || { off: 0, last: null };
        if (size < s.off) { ok = false; break; }
        if (size === s.off) continue;
        const fd = fs.openSync(f, "r"), buf = Buffer.alloc(size - s.off);
        try { fs.readSync(fd, buf, 0, buf.length, s.off); } finally { fs.closeSync(fd); }
        const nl = buf.lastIndexOf(10); if (nl < 0) continue; // no complete line yet
        let last = s.last;
        rows(buf.subarray(0, nl + 1).toString("utf8"), (id, model, u) => {
          if (last && last.id === id) add(t, last.model, last.u, -1); else if (seen.has(id)) ok = false;
          seen.add(id); add(t, model, u, 1); last = { id, model, u };
          if (f === tp && model !== "<synthetic>") ctx = ctxOf(u);
        });
        fstate[f] = { off: s.off + nl + 1, last };
      }
      if (ok) now = { total: t, files: fstate, mainCtx: ctx };
    } catch { now = null; }
  }
  if (!now) now = full();
  const total = now.total, mainCtx = now.mainCtx;
  const delta = {};
  for (const [model, k] of Object.entries(total)) {
    const b = prev[model] || {};
    const d = {};
    let any = false;
    for (const x of ["input", "cacheWrite", "cacheRead", "output"]) { d[x] = k[x] - (b[x] || 0); if (d[x]) any = true; }
    if (any) delta[model] = d;
  }
  fs.writeFileSync(snapFile, JSON.stringify({ v: 2, total, files: now.files, mainCtx }));

  // Context warning for the boss (shown to him only, never to the model): from 500K, then every further 150K.
  const ctxFile = path.join(sd, "usage-state", p.session_id + ".ctx");
  let warned = 0;
  try { warned = Number(fs.readFileSync(ctxFile, "utf8")) || 0; } catch { /* none yet */ }
  if (mainCtx < warned) warned = 0; // compacted or cleared: start over
  if (mainCtx >= 500000 && mainCtx >= warned + 150000) {
    warned = mainCtx;
    // Save the working position first (same capture as before a compact), so a /clear right now loses nothing.
    let saved = false;
    try {
      const mem = path.join(root, ".claude", "skills", "memory", "scripts", "memory.mjs");
      if (fs.existsSync(mem)) {
        const r = spawnSync(process.execPath, [mem, "capture"], { cwd: root, input: JSON.stringify({ ...p, transcript_path: tp, hook_event_name: "Stop", reason: "context-warning" }), timeout: 20000 });
        saved = r.status === 0;
      }
    } catch { /* warn anyway */ }
    const k = Math.round(mainCtx / 1000);
    process.stdout.write(JSON.stringify({ systemMessage: saved
      ? `Konteks sesi ${k}K token. Posisi kerja sudah disimpan otomatis; ketik /clear sekarang atau setelah ship task ini (brief awal sesi memulihkannya).`
      : `Konteks sesi ${k}K token. Setiap balasan membaca ulang semuanya: selesaikan/ship task ini lalu /clear.` }));
  }
  fs.writeFileSync(ctxFile, String(warned));
  if (!Object.keys(delta).length) process.exit(0);

  const head = fs.readFileSync(path.join(gd, "HEAD"), "utf8");
  const m = head.match(/refs\/heads\/task\/(\S+)/);
  const row = { ts: new Date().toISOString(), session: p.session_id, transcript: tp, task: m ? m[1] : "none", agent: p.agent_type || "", models: delta };
  fs.appendFileSync(path.join(sd, "usage.jsonl"), JSON.stringify(row) + "\n");
} catch { /* never disturb the session */ }
process.exit(0);
