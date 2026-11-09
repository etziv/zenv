// Verifies that src-tauri/windows/installer.nsi is Tauri's upstream NSIS
// template for the installed @tauri-apps/cli, with only Zenv's install-folder
// change applied. Run after upgrading the CLI: npm run check:nsis
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const version = require("@tauri-apps/cli/package.json").version;
const url = `https://raw.githubusercontent.com/tauri-apps/tauri/tauri-cli-v${version}/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi`;

const res = await fetch(url);
if (!res.ok) {
  console.error(`cannot download the upstream template for CLI ${version}: ${res.status}`);
  process.exit(2);
}
const upstream = await res.text();
const ours = readFileSync(new URL("../src-tauri/windows/installer.nsi", import.meta.url), "utf8");

const ORIGINAL = '      StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"';
const ZENV = [
  "      ; Zenv: install into <system drive>\\zenv (C:\\zenv), which is also its",
  "      ; home for installed versions, data and configuration.",
  "      StrCpy $0 $WINDIR 2",
  '      StrCpy $INSTDIR "$0\\zenv"',
].join("\n");

if (!upstream.includes(ORIGINAL)) {
  console.error(`the upstream template (CLI ${version}) no longer contains the install-folder line; update the Zenv change`);
  process.exit(1);
}
const expected = upstream.replace(ORIGINAL, ZENV);
const body = ours.slice(ours.indexOf(expected.slice(0, 40)));
if (body.replace(/\r\n/g, "\n") !== expected.replace(/\r\n/g, "\n")) {
  console.error(`src-tauri/windows/installer.nsi differs from upstream CLI ${version} beyond the Zenv change; re-copy it`);
  process.exit(1);
}
console.log(`installer.nsi matches the upstream template of @tauri-apps/cli ${version} plus the Zenv install-folder change`);
