// Standalone module (no deps): (1) decisionCue nudges the dev to record a D- entry or
// ruling when the boss's chat message states a decision/standing rule; (2) logPrompt /
// promptStats measure what the UserPromptSubmit hook did (memory injected, or why it
// abstained, how long it took) without ever storing the prompt text itself.
// Never throws: every exported function returns an empty/null value on any error.

import fs from 'node:fs';
import path from 'node:path';

const ROTATE_BYTES = 512 * 1024;

const CUE = '[memory] this message states a decision: record it (D- entry quoting the boss with the date, or a ruling) if it is new';

// Clauses starting with these (Indonesian question words) are treated as questions even
// without a trailing "?", since casual chat often drops punctuation.
const QUESTION_START = /^\s*(kenapa|knp|kok|apakah|gimana|bagaimana|gmn|mana)\b/i;

// "jangan2" / "jangan-jangan" is the Indonesian idiom for "I bet/probably", not a
// prohibition ("jangan" = don't) - must not be read as a standing rule.
const JANGAN_IDIOM = /jangan-?2\b|jangan-jangan\b/i;

// Markers for a decision/standing rule (not an open question, not a one-off work
// request, not a bare approval). Checked against one clause at a time.
// "pakai" and its colloquial short form "pake" both mean "use".
const PAKE = "pak(?:ai|e)";

const DECISION_PATTERNS = [
  new RegExp(`\\b${PAKE}\\b[^?\\n]*\\b(saja|aja|ajalah|ajah|terus|ini)\\b`, 'i'), // "pakai/pake X saja/aja/ini" - use X (only/from here on)
  new RegExp(`\\b(mau|ingin)\\s+${PAKE}\\b`, 'i'), // "mau pakai X" - wants to use X going forward
  /\btetap\s+(pakai|pake|gunakan)\b/i, // keep using
  /\bcukup\s+pak(ai|e)\b/i, // "just use" (switch to the simpler option)
  /\bganti\s+(ke|jadi|pakai|pake)\b/i, // switch to
  /\btidak\s+(usah|perlu)\b/i, // no need to (stop doing X)
  /\b(mulai|dari)\s+sekarang\b/i, // from now on
  /\b(aku|saya)\s+(putuskan|memutuskan)\b/i, // I decide(d)
  /\bkeputusan(nya)?\b/i, // decision
  /\b(izinkan|diizinkan)\b/i, // permit/allow (standing permission)
  /\bsecara\s+default\b/i, // "by default" - sets the standing default behaviour
  // "should (always)", but not "harusnya ketika/kalau X maka Y" (explaining how an
  // existing business process normally works, not instructing the agent).
  /\bralat\b/i, // "correction:" - revising a prior rule
  /\bfrom now on\b/i,
  /\bgoing forward\b/i,
  /\bstick (with|to)\b/i,
  /\bdecided to\b/i,
  /\b(let'?s|we'?ll)\s+(use|go with|stick with)\b/i,
  /\bonly use\b/i,
  /\buse\b[^?\n]*\bonly\b/i,
];

// "selalu / always / never / harusnya" describe a bug as often as a rule ("aplikasinya selalu crash", "the export always
// fails"): they count only in a directive clause (who should act, or what to use/do) that reports no failure.
const WEAK = /\b(selalu|always|never|harusnya)\b(?!\s+(ketika|kalau|kalo|jika)\b)/i;
const DIRECTIVE = /\b(kamu|kau|lu|kita|agent|dev|you|we|pakai|pake|gunakan|use|tulis|buat|jalankan|run|tanya|ask|commit|push|cek|check|uji|test|kerjakan|catat|record|simpan|balas|reply|minta|do)\b/i;
const FAILURE = /\b(crash|crashes|error|errors|gagal|fail|fails|failed|failing|bug|rusak|hang|stuck|macet|lambat|slow|muncul|keluar|hilang)\b|\b(tidak|ga|gak|nggak) bisa\b/i;
const weakDecision = (c) => WEAK.test(c) && DIRECTIVE.test(c) && !FAILURE.test(c);

function splitClauses(prompt) {
  const clauses = [];
  for (const line of prompt.split(/\r?\n/)) {
    // split on sentence punctuation AND commas, so a decision clause joined by a
    // comma to a trailing question ("jangan X, gimana Y?") is scored on its own.
    const parts = line.match(/[^.!?,]+[.!?,]?/g);
    if (parts && parts.length) clauses.push(...parts);
    else if (line.trim()) clauses.push(line);
  }
  return clauses.map((c) => c.trim()).filter(Boolean);
}

function isQuestionClause(clause) {
  if (/\?\s*$/.test(clause)) return true;
  if (QUESTION_START.test(clause)) return true;
  return false;
}

// "jangan lupa X" ("don't forget X") is a reminder for the current task, not a
// standing rule.
const JANGAN_LUPA = /\bjangan\s+lupa\b/i;

function hasJangan(clause) {
  if (JANGAN_IDIOM.test(clause) || JANGAN_LUPA.test(clause)) return false;
  return /\bjangan\b/i.test(clause);
}

// Strip one leading interjection ("loh", "lah", "nah", "oh", "ya", "eh") so a short
// imperative like "loh pake wireless debugging" is still read as "pake X".
function stripInterjection(clause) {
  return clause.replace(/^(loh|lah|nah|oh|ya|eh)\b[,\s]*/i, '');
}

/**
 * decisionCue(prompt) -> a one-line reminder string, or null.
 * Fires when some clause of the prompt states a decision/standing rule and that
 * clause is not itself a question. Questions ("pakai X atau Y?"), plain work
 * requests ("kerjakan", "cek lagi") and bare approvals ("ya", "oke", "APPROVE")
 * contain none of the markers below and so return null.
 */
export function decisionCue(prompt) {
  try {
    if (!prompt || typeof prompt !== 'string') return null;
    const clauses = splitClauses(prompt);
    for (const clause of clauses) {
      if (isQuestionClause(clause)) continue;
      if (hasJangan(clause)) return CUE;
      // Short sentence-initial "pakai/pake X ..." imperative (e.g. "pakai c xampp
      // station") has no "saja/aja" qualifier but is still a direct standing choice.
      const bare = stripInterjection(clause);
      if (new RegExp(`^${PAKE}\\s+\\S`, 'i').test(bare) && bare.split(/\s+/).length <= 8) return CUE;
      if (weakDecision(clause)) return CUE;
      for (const re of DECISION_PATTERNS) {
        if (re.test(clause)) return CUE;
      }
    }
    return null;
  } catch {
    return null;
  }
}

function toArray(v) {
  if (Array.isArray(v)) return v;
  if (v === undefined || v === null || v === '') return [];
  return [v];
}

/**
 * logPrompt(file, rec) appends one JSON line { at, chars, injected, reason, ms }
 * to `file`, never the prompt text itself. Creates the directory if missing,
 * rotates the current file to `<file>.1` once it would pass 512 KB. Never throws.
 */
export function logPrompt(file, rec) {
  try {
    if (!file || typeof file !== 'string') return;
    const dir = path.dirname(file);
    if (dir && dir !== '.') fs.mkdirSync(dir, { recursive: true });
    const safe = {
      at: rec && typeof rec.at === 'string' ? rec.at : new Date().toISOString(),
      chars: rec && Number.isFinite(rec.chars) ? rec.chars : 0,
      injected: toArray(rec && rec.injected),
      cue: !!(rec && rec.cue), // the decision reminder fired (not a memory injection)
      reason: rec && typeof rec.reason === 'string' ? rec.reason : '',
      ms: rec && Number.isFinite(rec.ms) ? rec.ms : 0,
    };
    const line = JSON.stringify(safe) + '\n';
    let size = 0;
    try {
      size = fs.statSync(file).size;
    } catch {
      size = 0;
    }
    if (size > 0 && size + Buffer.byteLength(line) > ROTATE_BYTES) {
      try {
        fs.rmSync(file + '.1', { force: true });
      } catch {}
      try {
        fs.renameSync(file, file + '.1');
      } catch {}
    }
    fs.appendFileSync(file, line);
  } catch {
    // never throw
  }
}

/**
 * promptStats(file) -> short text summary of the log, or "" when there is no log
 * (or it is empty/unreadable).
 */
export function promptStats(file) {
  try {
    if (!file || typeof file !== 'string' || !fs.existsSync(file)) return '';
    const raw = (fs.existsSync(file + '.1') ? fs.readFileSync(file + '.1', 'utf8') + '\n' : '') + fs.readFileSync(file, 'utf8'); // the rotated part counts
    const lines = raw.split('\n').filter((l) => l.trim());
    if (!lines.length) return '';
    let total = 0;
    let injectedCount = 0;
    const reasonCounts = new Map();
    const timings = [];
    for (const line of lines) {
      let rec;
      try {
        rec = JSON.parse(line);
      } catch {
        continue;
      }
      total++;
      const injected = (Array.isArray(rec.injected) ? rec.injected : []).filter((x) => x !== 'cue'); // the decision cue is no memory injection
      if (injected.length > 0) {
        injectedCount++;
      } else {
        const reason = typeof rec.reason === 'string' && rec.reason ? rec.reason : '(none)';
        reasonCounts.set(reason, (reasonCounts.get(reason) || 0) + 1);
      }
      if (Number.isFinite(rec.ms)) timings.push(rec.ms);
    }
    if (!total) return '';
    const pct = Math.round((injectedCount / total) * 100);
    const topReasons = [...reasonCounts.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([r, c]) => `${r} (${c})`)
      .join(', ');
    timings.sort((a, b) => a - b);
    const mean = timings.length ? Math.round(timings.reduce((a, b) => a + b, 0) / timings.length) : 0;
    const p90 = timings.length ? timings[Math.min(timings.length - 1, Math.floor(timings.length * 0.9))] : 0;
    return `[promptlog] ${total} prompts, ${pct}% injected, top abstain: ${topReasons || 'n/a'}, mean ${mean}ms, p90 ${p90}ms`;
  } catch {
    return '';
  }
}
