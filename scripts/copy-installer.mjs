// Copies the NSIS installer Tauri just built to installer\Zenv-setup.exe, a
// fixed and easy-to-find name (the bundle's own name carries the version and
// sits deep under src-tauri\target). Run by `npm run installer`.
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const { version } = JSON.parse(readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const built = path.join(root, "src-tauri", "target", "release", "bundle", "nsis", `Zenv_${version}_x64-setup.exe`);
if (!existsSync(built)) {
  console.error(`No installer at ${built}; run \`npm run tauri -- build\` first.`);
  process.exit(1);
}
const dir = path.join(root, "installer");
mkdirSync(dir, { recursive: true });
const target = path.join(dir, "Zenv-setup.exe");
copyFileSync(built, target);
const mb = (statSync(target).size / 1024 / 1024).toFixed(2);
console.log(`Installer: ${target} (${mb} MB, Zenv ${version})`);
