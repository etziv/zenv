#!/usr/bin/env node
// fonts: fetch the faces a design picked (Google Fonts, static TTF per weight) into a project folder, so FigCraft
// draws them (figlive "fonts", text.loadFont) and the app can self-host them. Not an install: files only.
//   node fonts.mjs "Hanken Grotesk:400,600,700" "Fraunces:400,700" [--out .brain/design/proposal/fonts]
// Prints the saved paths as JSON for the plan's "fonts". Unknown family or weight: exit 1 with its name.
// shortcut: upright weights only; add ital when a direction's type needs italics.
import fs from "node:fs";
import path from "node:path";

const a = process.argv.slice(2), oi = a.indexOf("--out");
const out = oi >= 0 ? a[oi + 1] : ".brain/design/proposal/fonts";
const specs = a.filter((x, i) => !x.startsWith("--") && (oi < 0 || i !== oi + 1));
if (!specs.length) { console.log('usage: node fonts.mjs "<Family>:<w>,<w>" ... [--out dir]'); process.exit(1); }
fs.mkdirSync(out, { recursive: true });
const saved = [], fails = [];
for (const spec of specs) {
  const [family, w = "400"] = spec.split(":"), weights = w.split(",").map((x) => x.trim()).sort((x, y) => x - y);
  // Node's default user agent gets whole TTF files (a browser one gets woff2 subsets FigCraft cannot load)
  const r = await fetch("https://fonts.googleapis.com/css2?family=" + encodeURIComponent(family.trim()).replace(/%20/g, "+") + ":wght@" + weights.join(";"));
  if (!r.ok) { fails.push(family + " " + weights.join(",") + " (Google Fonts " + r.status + ": wrong name or a weight it does not have)"); continue; }
  const css = await r.text(), got = new Set();
  for (const m of css.matchAll(/font-weight:\s*(\d+);[\s\S]*?src:\s*url\((https:\/\/fonts\.gstatic\.com\/[^)]+\.ttf)\)/g)) {
    got.add(m[1]);
    const file = path.join(out, family.replace(/[^A-Za-z0-9]/g, "") + "-" + m[1] + ".ttf");
    const f = await fetch(m[2], { redirect: "error" }); if (!f.ok) { fails.push(file + " (" + f.status + ")"); continue; }
    fs.writeFileSync(file, Buffer.from(await f.arrayBuffer())); saved.push(file.replace(/\\/g, "/"));
  }
  // Google drops a weight the family lacks without an error
  for (const x of weights.filter((x) => /^\d+$/.test(x) && !got.has(x))) fails.push(family + " " + x + " (Google Fonts has no such weight)");
}
for (const f of fails) console.log("FAIL " + f);
console.log(JSON.stringify(saved));
console.log("licence: check each family at fonts.google.com/specimen/<Family>/license (OFL or Apache) before it ships");
process.exit(fails.length || !saved.length ? 1 : 0);
