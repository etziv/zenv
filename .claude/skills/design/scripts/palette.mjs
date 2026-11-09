#!/usr/bin/env node
// Palette check: WCAG 2 contrast for every pair a UI actually renders, and for a failing pair the nearest
// shade of the SAME hue that passes (lightness moved in OKLCH, so the colour keeps its character).
// Zero dependencies, no network.
//
//   node palette.mjs bg=#F6F1E8 surface=#FFFFFF ink=#1B1A17 muted=#6B655C accent=#0F6E5A on-accent=#FFFFFF [--json]
//   node palette.mjs palette.json        (a {"light": {...roles}, "dark": {...roles}} file, same role names)
//
// Pairs checked when the roles exist: ink, muted, accent on bg and on surface (text, 4.5:1); on-accent on accent
// (button label, 4.5:1); accent on bg as a non-text UI part (3:1); border on bg (3:1, only if a border role is given).
import fs from "node:fs";

const args = process.argv.slice(2);
const asJson = args.includes("--json");
let sets = {};
const file = args.find((a) => /\.json$/i.test(a));
if (file) sets = JSON.parse(fs.readFileSync(file, "utf8"));
else sets.palette = Object.fromEntries(args.filter((a) => a.includes("=")).map((a) => a.split("=")).map(([k, v]) => [k.trim(), v.trim()]));
if (!Object.values(sets).some((s) => Object.keys(s).length)) {
  console.log("usage: node palette.mjs bg=#hex surface=#hex ink=#hex muted=#hex accent=#hex on-accent=#hex [border=#hex] [--json]");
  process.exit(0);
}

const hex = (h) => { const m = String(h).replace("#", "").match(/^([0-9a-f]{3}|[0-9a-f]{6})$/i); if (!m) throw new Error("not a hex colour: " + h);
  const s = m[1].length === 3 ? m[1].split("").map((c) => c + c).join("") : m[1]; return [0, 2, 4].map((i) => parseInt(s.slice(i, i + 2), 16) / 255); };
const toHex = (rgb) => "#" + rgb.map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255).toString(16).padStart(2, "0")).join("").toUpperCase();
const lin = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const unlin = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const lum = (rgb) => { const [r, g, b] = rgb.map(lin); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const ratio = (a, b) => { const [x, y] = [lum(hex(a)), lum(hex(b))].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
// sRGB <-> OKLCH (Björn Ottosson's OKLab)
function toOklch(rgb) {
  const [r, g, b] = rgb.map(lin);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(A, B), Math.atan2(B, A)];
}
function fromOklch([L, C, H]) {
  const A = C * Math.cos(H), B = C * Math.sin(H);
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3, m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3, s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s].map(unlin);
}
// Nearest passing shade of fg against bg: walk lightness away from bg in small steps, keep hue, ease chroma into gamut.
function fix(fg, bg, need) {
  const [L, C, H] = toOklch(hex(fg)), dir = lum(hex(fg)) >= lum(hex(bg)) ? 1 : -1;
  for (let i = 1; i <= 100; i++) {
    const nl = L + dir * i * 0.01; if (nl < 0 || nl > 1) break;
    let c = C, rgb = fromOklch([nl, c, H]);
    while (rgb.some((v) => v < -0.001 || v > 1.001) && c > 0) { c -= 0.005; rgb = fromOklch([nl, c, H]); }
    const h = toHex(rgb); if (ratio(h, bg) >= need) return h;
  }
  return null;
}

const report = {};
let fails = 0;
for (const p of Object.values(sets)) for (const [k, v] of Object.entries(p)) {
  try { hex(v); } catch { console.log("not a hex colour: " + k + "=" + v + " (use #RGB or #RRGGBB)"); process.exit(2); }
}
for (const [set, p] of Object.entries(sets)) {
  const rows = [];
  const pair = (fg, bg, need, what) => { if (!p[fg] || !p[bg]) return;
    const r = ratio(p[fg], p[bg]), ok = r >= need; if (!ok) fails++;
    // A label already at white or black cannot move further: move the background instead (darken the accent).
    let f = ok ? null : fix(p[fg], p[bg], need), alt = !ok && !f ? fix(p[bg], p[fg], need) : null;
    rows.push({ pair: fg + " on " + bg, what, ratio: +r.toFixed(2), need, ok, fix: f, fixBackground: alt }); };
  for (const bg of ["bg", "surface"]) for (const fg of ["ink", "muted", "accent"]) pair(fg, bg, 4.5, "text");
  pair("on-accent", "accent", 4.5, "button label");
  pair("accent", "bg", 3, "UI part (icon, focus ring, control)");
  pair("border", "bg", 3, "border of a control");
  report[set] = rows;
}
if (asJson) { console.log(JSON.stringify({ fails, sets: report }, null, 2)); process.exit(fails ? 1 : 0); }
for (const [set, rows] of Object.entries(report)) {
  console.log("[" + set + "]");
  for (const r of rows) console.log("  " + (r.ok ? "PASS" : "FAIL") + "  " + r.pair.padEnd(22) + String(r.ratio).padStart(6) + ":1  need " + r.need + ":1  " + r.what
    + (r.fix ? "  -> nearest passing shade " + r.fix : r.fixBackground ? "  -> keep the label, make " + r.pair.split(" on ")[1] + " " + r.fixBackground : ""));
}
console.log(fails ? fails + " pair(s) fail WCAG AA; use the suggested shade or change the pair." : "All checked pairs pass WCAG AA.");
process.exit(fails ? 1 : 0);
