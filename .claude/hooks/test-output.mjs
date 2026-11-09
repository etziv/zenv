#!/usr/bin/env node
// PreToolUse hook (Bash, PowerShell): a test run prints one line per passing test by default (1,000+ lines for a big
// suite), and every line stays in context and is re-read on each later turn. This adds the runner's own compact flag,
// which still prints every failure in full: Laravel/Pest `--compact`, Vitest `--reporter=dot`. Gradle is left alone
// (`-q` hides failure details). A command with a pipe, a redirect or its own reporter flag is never touched.
// Zero tokens; on any doubt or error it changes nothing.
import fs from "node:fs";

try {
  const p = JSON.parse(fs.readFileSync(0, "utf8") || "{}");
  const input = p.tool_input || {};
  const cmd = String(input.command || "");
  if (!/^(Bash|PowerShell)$/.test(p.tool_name || "") || !cmd || /[|>]|task\.mjs|\s--(compact|teamcity|log-junit|reporter)\b/.test(cmd)) process.exit(0);
  // One test command, optionally after `cd <dir> &&` / `cd <dir>;` (a repo of a multi-repo home); no other chain.
  const parts = cmd.split(/\s*(?:&&|;)\s*/);
  const last = parts[parts.length - 1];
  if (parts.slice(0, -1).some((s) => !/^(cd|Set-Location|pushd)\s+\S+$/i.test(s.trim())) || /&/.test(last)) process.exit(0);
  let flag = null;
  if (/(^|[\s"'\\/])(artisan(\.php)?["']?\s+test|pest(\.bat)?["']?)(\s|$)/.test(last)) flag = " --compact"; // php artisan test / vendor/bin/pest
  else if (/(^|\s)(npx\s+)?vitest(\s+run)?(\s|$)/.test(last) && !/\swatch\b/.test(last)) flag = " --reporter=dot";
  if (!flag) process.exit(0);
  const next = cmd + flag;
  process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "PreToolUse", updatedInput: { ...input, command: next } } }));
} catch { /* never disturb the session */ }
process.exit(0);
