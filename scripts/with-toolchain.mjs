import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const DEFAULT_MINGW = "D:\\DevCache\\toolchains\\winlibs-gcc16-msvcrt\\mingw64\\bin";

const extra = [];
const mingw = process.env.ZENV_MINGW_BIN ?? DEFAULT_MINGW;
if (existsSync(path.join(mingw, "gcc.exe"))) extra.push(mingw);
else if (process.env.ZENV_MINGW_BIN) {
  console.error(`ZENV_MINGW_BIN does not contain gcc.exe: ${mingw}`);
  process.exit(1);
}
const cargoBin = path.join(homedir(), ".cargo", "bin");
if (existsSync(cargoBin)) extra.push(cargoBin);

const [cmd, ...args] = process.argv.slice(2);
if (!cmd) {
  console.error("usage: node scripts/with-toolchain.mjs <command> [args...]");
  process.exit(2);
}

const env = { ...process.env, PATH: [...extra, process.env.PATH ?? process.env.Path].join(path.delimiter) };
delete env.Path;
const child = spawn(cmd, args, { stdio: "inherit", env, shell: process.platform === "win32" });
child.on("exit", (code, signal) => process.exit(signal ? 1 : (code ?? 1)));
