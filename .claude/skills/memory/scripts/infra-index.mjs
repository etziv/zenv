#!/usr/bin/env node
// Local infrastructure index (no LLM, 0 tokens to build). Scans Docker, Compose, Kubernetes, Helm, Terraform,
// Ansible, CI and Makefiles and writes a compact `.memoir/infra.md` of headings (one per service/resource) that
// graphify then indexes, so dev can query "where is X deployed" cheaply instead of reading every infra file.
// Idempotent: writes only when the content actually changes (no churn). No infra files -> writes nothing, exits 0.
// Secret VALUES are never read or written; only variable NAMES. Run through: node .claude/skills/memory/scripts/memory.mjs infra
import fs from "node:fs";
import path from "node:path";

const root = process.argv[2] || process.env.CLAUDE_PROJECT_DIR || process.cwd();
const SKIP = new Set(["node_modules", "vendor", "dist", "build", ".gradle", ".git", "graphify-out", ".memoir",
  ".claude", "coverage", ".playwright-mcp", ".taste-skill", "out"]);
const files = [];
(function walk(dir, depth) {
  if (depth > 6) return;
  let ents;
  try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
  for (const e of ents) {
    if (e.name.startsWith(".") && ![".github", ".circleci", ".gitlab-ci.yml"].includes(e.name)) continue;
    if (e.isDirectory()) { if (!SKIP.has(e.name)) walk(path.join(dir, e.name), depth + 1); continue; }
    files.push(path.join(dir, e.name));
  }
})(root, 0);

const rel = (f) => path.relative(root, f).split(path.sep).join("/");
const read = (f) => { try { return fs.readFileSync(f, "utf8"); } catch { return ""; } };
const sections = []; // { h: heading, lines: [..] }
const uniq = (a) => [...new Set(a)];

for (const f of files) {
  const b = path.basename(f), r = rel(f), t = () => read(f);
  // Dockerfile
  if (/^Dockerfile(\..+)?$/i.test(b)) {
    const s = t(); const from = (s.match(/^\s*FROM\s+(\S+)/im) || [])[1];
    const expose = [...s.matchAll(/^\s*EXPOSE\s+(.+)$/gim)].flatMap((m) => m[1].split(/\s+/)).filter(Boolean);
    const env = uniq([...s.matchAll(/^\s*ENV\s+([A-Z][A-Z0-9_]*)/gim)].map((m) => m[1]));
    sections.push({ h: "image " + r, lines: [from && "from " + from, expose.length && "expose " + expose.join(", "), env.length && "env " + env.join(", ")].filter(Boolean) });
  }
  // docker-compose
  else if (/^(docker-)?compose.*\.ya?ml$/i.test(b)) {
    const s = t();
    const svcBlock = (s.split(/\n(?=services:)/).pop() || "").split(/\n(?=[A-Za-z_][\w-]*:)/)[0]; // up to the next top-level key
    const svcs = [...svcBlock.matchAll(/^\s{2}([A-Za-z0-9_.-]+):\s*$/gm)].map((m) => m[1]);
    const imgs = uniq([...s.matchAll(/^\s*image:\s*(\S+)/gim)].map((m) => m[1]));
    const ports = uniq([...s.matchAll(/^\s*-\s*["']?(\d+:\d+)/gim)].map((m) => m[1]));
    sections.push({ h: "compose " + r, lines: [svcs.length && "services " + svcs.join(", "), imgs.length && "images " + imgs.slice(0, 8).join(", "), ports.length && "ports " + ports.join(", ")].filter(Boolean) });
  }
  // Kubernetes / Helm templates
  else if (/\.ya?ml$/i.test(b) && /(^|\/)(k8s|kube|kubernetes|helm|deploy|manifests|charts)(\/|$)/i.test(r)) {
    const s = t();
    for (const m of s.split(/^---\s*$/m)) {
      const kind = (m.match(/^\s*kind:\s*(\S+)/im) || [])[1], name = (m.match(/^\s*name:\s*(\S+)/im) || [])[1];
      if (!kind) continue;
      const img = uniq([...m.matchAll(/^\s*image:\s*(\S+)/gim)].map((x) => x[1]));
      sections.push({ h: kind + " " + (name || "?") + " (" + r + ")", lines: [img.length && "image " + img.join(", ")].filter(Boolean) });
    }
  }
  // Terraform / OpenTofu
  else if (/\.tf$/i.test(b)) {
    const s = t();
    for (const m of s.matchAll(/^\s*(resource|module|data)\s+"([^"]+)"(?:\s+"([^"]+)")?/gim)) {
      sections.push({ h: m[1] + " " + [m[2], m[3]].filter(Boolean).join(".") + " (" + r + ")", lines: [] });
    }
  }
  // Ansible
  else if (/\.ya?ml$/i.test(b) && /(^|\/)(ansible|playbooks?|roles)(\/|$)/i.test(r)) {
    const names = uniq([...t().matchAll(/^\s*-?\s*name:\s*(.+)$/gim)].map((m) => m[1].trim())).slice(0, 10);
    if (names.length) sections.push({ h: "ansible " + r, lines: ["tasks " + names.join("; ")] });
  }
  // CI workflows
  else if (/(^|\/)\.github\/workflows\/.+\.ya?ml$/i.test(r) || /(^|\/)(\.gitlab-ci\.yml|azure-pipelines\.yml|\.circleci\/config\.yml|Jenkinsfile)$/i.test(r)) {
    const s = t(); const name = (s.match(/^\s*name:\s*(.+)$/im) || [])[1];
    const jb = /(^|\n)jobs:\s*\n/.test(s) ? s.split(/(?:^|\n)jobs:\s*\n/)[1].split(/\n(?=\S)/)[0] : s;
    const ind = jb === s ? "" : ((jb.match(/^( +)\S/m) || [])[1] || "  ");
    const jobs = [...jb.matchAll(jb === s ? /^([A-Za-z0-9_.-]+):/gm : new RegExp("^" + ind + "([A-Za-z0-9_-]+):\\s*$", "gm"))].map((m) => m[1]).filter((j) => !/^(on|name|jobs|permissions|env|concurrency|defaults|stages|variables|version|workflows|orbs|image|include|default|cache|before_script|after_script)$/i.test(j));
    sections.push({ h: "ci " + r, lines: [name && "name " + name, jobs.length && "jobs " + uniq(jobs).slice(0, 12).join(", ")].filter(Boolean) });
  }
}

const out = path.join(root, ".memoir", "infra.md");
if (!sections.length) {
  // nothing to index: do not create an empty file; remove a stale one so the graph stays honest
  try { if (fs.existsSync(out)) fs.rmSync(out); } catch { /* ignore */ }
  process.exit(0);
}
sections.sort((a, b) => a.h.localeCompare(b.h));
const body = "# INFRA (auto-generated by the memory skill (memory.mjs infra); do not edit by hand — it is overwritten)\n\n"
  + "Query it with graphify (headings are indexed) or read a section. Rebuilt on ship when infra files change. No secret values.\n\n"
  + sections.map((s) => "## " + s.h + (s.lines.length ? "\n" + s.lines.map((l) => "- " + l).join("\n") : "")).join("\n\n") + "\n";

let current = "";
try { current = fs.readFileSync(out, "utf8"); } catch { /* new */ }
if (current === body) process.exit(0); // idempotent: no change, no write, no churn
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, body);
console.log("infra index: " + sections.length + " resource(s) -> .memoir/infra.md");
