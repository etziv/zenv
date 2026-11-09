// Lint for memory facts that rot fast: test/pass counts, CI state, commit SHAs stated without a date or a
// pointer to their source ("as of YYYY-MM-DD"). Pure, no dependencies, never throws. ESM, Node 22.
//
// export function rotIssues(text, relFile) -> [{ line, kind, snippet }]
//   kind: "count" ("15/15 passed", "98 passed", "17 tests", "3 failed")
//         "ci"    ("CI green/red/passing/failed")
//         "sha"   (7-40 hex chars, at least one digit and one letter, word-bounded)
//
// A hit is fine (not returned) when:
//   - the line itself carries a YYYY-MM-DD date or the phrase "as of", OR
//   - the nearest heading (any #/##/### level) above the line carries a date or "as of"
//     (this covers "## D-012 ... 2026-10-02" entry headings too), OR
//   - the line sits under the nearest "## Test results" / "## Evidence" / "## History" section, OR
//   - the match is inside inline code (`...`) or a fenced code block (``` ... ```)
// relFile under archive/ or handoff/ -> [] (those are history, expected to rot).

const DATE_RE = /\b\d{4}-\d{2}-\d{2}\b/;
const AS_OF_RE = /\bas of\b/i;
const HEADING_RE = /^(#{1,6})\s+(.*)$/;
const FENCE_RE = /^\s*(```|~~~)/;
const SECTION_HEADING_RE = /^##\s+(.*)$/;
const SAFE_SECTIONS = /^(test results|evidence|history)\s*$/i;
const LIST_START_RE = /^\s*(?:[-*+]|\d+[.)])\s/;

// count: ratios and bare numbers next to test/pass/fail/green/red words. "tests?" dropped from the ratio form
// (so "200/403 test" - an HTTP status pair, not a pass count - does not match).
const COUNT_RE = /\b\d+\/\d+\s*(?:passed|passing|pass|failed|failing|fail|green|red)\b|\b\d+\s*(?:tests?\s+)?(?:passed|passing|failed|failing)\b|\b\d+\s+tests?\b/gi;

// ci: "CI" within a short window of a state word, either order.
const CI_RE = /\bci\b[^.\n]{0,20}\b(?:green|red|passing|failed|pass|fail)\b|\b(?:green|red|passing|failed)\b[^.\n]{0,20}\bci\b/gi;

// sha: 7-40 hex chars, word-bounded (so a longer hex run with no boundary inside [7,40] is skipped entirely).
const SHA_RE = /\b[0-9a-f]{7,40}\b/gi;
const UUID_RE = /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi; // its hex runs are no commit ids

function isArchiveOrHandoff(relFile) {
  if (typeof relFile !== "string") return false;
  const norm = relFile.replace(/\\/g, "/");
  return /(^|\/)archive\//.test(norm) || /(^|\/)handoff\//.test(norm);
}

// Mask `inline code` spans with a filler that cannot match any of our patterns, keeping line length/positions.
function maskInlineCode(line) {
  let out = "";
  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (ch === "`") {
      const end = line.indexOf("`", i + 1);
      if (end === -1) { out += line.slice(i); break; }
      out += "#".repeat(end - i + 1);
      i = end + 1;
    } else {
      out += ch;
      i++;
    }
  }
  return out;
}

function lineHasDateOrAsOf(line) {
  return DATE_RE.test(line) || AS_OF_RE.test(line);
}

// SHA false-positive guard: a hex token that looks like a content-hash fragment in an asset path or URL
// ("/assets/a1b2c3d.js", "https://cdn.example.com/abcdef1234.css") is a legitimate non-commit hash, not a rotting
// fact about repo state - skip it. Rule: preceded immediately by "/" AND followed immediately by "." + letters
// (a file extension), which is how bundlers/cdns name hashed assets; a bare SHA in prose never has that shape.
function looksLikeAssetHash(line, start, end) {
  const before = line[start - 1];
  const after = line.slice(end);
  return before === "/" && /^\.[a-z]{1,8}\b/i.test(after);
}

function sectionAt(lines, idx) {
  for (let i = idx; i >= 0; i--) {
    const m = SECTION_HEADING_RE.exec(lines[i]);
    SECTION_HEADING_RE.lastIndex = 0;
    if (m) return m[1].trim();
  }
  return "";
}

function nearestHeadingDated(lines, idx) {
  for (let i = idx; i >= 0; i--) {
    const m = HEADING_RE.exec(lines[i]);
    if (m) return lineHasDateOrAsOf(m[2]);
  }
  return false;
}

// A list item's own line often carries the date for the whole entry, and a later line only wraps it (hard-wrapped
// prose, no blank line between bullets). Walk back from idx to the start of its block: either the nearest list
// marker line (the bullet head, even across several consecutive un-blanked bullets) or the first line after a
// blank line / heading. Check that head line for a date, not just the literal current line.
function blockStartDated(lines, idx) {
  let j = idx;
  while (j > 0 && !LIST_START_RE.test(lines[j])) {
    const prev = lines[j - 1];
    if (prev.trim() === "" || HEADING_RE.test(prev)) break;
    j--;
  }
  return lineHasDateOrAsOf(lines[j]);
}

// Count matches that are acceptance-criteria thresholds, not stated results, are not rot: "- [ ] >= 40 tests",
// ">= 1 test per new event", "StyleMapper 1 test per field" describe a target/rate, never a thing that was true at
// some past moment and can go stale.
function isThresholdNotResult(masked, start, end) {
  const before = masked.slice(Math.max(0, start - 4), start);
  if (/[<>]=?\s*$/.test(before)) return true;
  // a bare "N test(s)" match sitting right after a "/", e.g. "Pest 200/403 test" - an HTTP status pair, not a
  // ratio-with-keyword (that is alt1) and not a pass/fail count.
  if (masked[start - 1] === "/") return true;
  const after = masked.slice(end, end + 6);
  if (/^\s*per\b/i.test(after)) return true;
  return false;
}

function snippet(line) {
  const t = line.trim();
  return t.length <= 80 ? t : t.slice(0, 77) + "...";
}

export function rotIssues(text, relFile) {
  try {
    if (typeof text !== "string" || text.length === 0) return [];
    if (isArchiveOrHandoff(relFile)) return [];

    const lines = text.split(/\r\n|\n/);
    const issues = [];
    let inFence = false;

    for (let i = 0; i < lines.length; i++) {
      const raw = lines[i];

      if (FENCE_RE.test(raw)) { inFence = !inFence; continue; }
      if (inFence) continue;

      if (lineHasDateOrAsOf(raw)) continue;
      if (SAFE_SECTIONS.test(sectionAt(lines, i))) continue;
      if (nearestHeadingDated(lines, i)) continue;
      if (blockStartDated(lines, i)) continue;

      const masked = maskInlineCode(raw).replace(UUID_RE, (u) => " ".repeat(u.length));
      const kinds = [
        ["count", COUNT_RE],
        ["ci", CI_RE],
        ["sha", SHA_RE],
      ];
      for (const [kind, re] of kinds) {
        re.lastIndex = 0;
        let m;
        while ((m = re.exec(masked))) {
          const token = m[0];
          if (kind === "sha") {
            if (!/[0-9]/.test(token) || !/[a-f]/i.test(token)) continue;
            if (looksLikeAssetHash(masked, m.index, m.index + token.length)) continue;
            if (masked[m.index - 1] === "#") continue; // a hex colour, not a commit
          }
          if (kind === "count" && isThresholdNotResult(masked, m.index, m.index + token.length)) continue;
          issues.push({ line: i + 1, kind, snippet: snippet(raw) });
          break; // one hit per kind per line is enough
        }
      }
    }
    return issues;
  } catch {
    return [];
  }
}
