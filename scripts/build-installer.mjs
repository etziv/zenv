import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const extraDefines = process.argv.slice(2);

function run(cmd, args) {
  const r = spawnSync(cmd, args, { stdio: "inherit", cwd: root });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

function findMakensis() {
  const candidates = [
    process.env.ZENV_MAKENSIS,
    path.join(process.env.LOCALAPPDATA ?? "", "tauri", "NSIS", "makensis.exe"),
    path.join(process.env["ProgramFiles(x86)"] ?? "", "NSIS", "makensis.exe"),
    path.join(process.env.ProgramFiles ?? "", "NSIS", "makensis.exe"),
  ].filter(Boolean);
  const found = candidates.find((c) => existsSync(c));
  if (!found) {
    console.error(`makensis.exe not found (looked in ${candidates.join(", ")}); install NSIS 3 or set ZENV_MAKENSIS`);
    process.exit(1);
  }
  return found;
}

const manifest = readFileSync(path.join(root, "src-native", "Cargo.toml"), "utf8");
const version = manifest.match(/^version\s*=\s*"([^"]+)"/m)[1];

run(process.execPath, [
  "scripts/with-toolchain.mjs",
  "cargo", "build", "--release", "-j", process.env.ZENV_JOBS ?? "2", "--manifest-path", "src-tauri/Cargo.toml", "-p", "zenv-native", "--bin", "zenv",
]);

const binary = path.join(root, "src-tauri", "target", "release", "zenv.exe");
const outDir = path.join(root, "installer");
mkdirSync(outDir, { recursive: true });
const outFile = path.join(outDir, "Zenv-setup.exe");
const sizeKb = Math.ceil(statSync(binary).size / 1024);

run(findMakensis(), [
  "/V2",
  `/DVERSION=${version}`,
  `/DBINARY=${binary}`,
  `/DOUTFILE=${outFile}`,
  `/DICON=${path.join(root, "src-tauri", "icons", "icon.ico")}`,
  `/DSIZE_KB=${sizeKb}`,
  ...extraDefines,
  path.join(outDir, "zenv.nsi"),
]);

const mb = (statSync(outFile).size / 1024 / 1024).toFixed(2);
console.log(`Installer: ${outFile} (${mb} MB, Zenv ${version})`);
