// figcraft MCP server for Claude Code (.mcp.json). The FigCraft window opens only when an agent first USES a figcraft
// tool: until then a headless figcraft-cli (no window) answers initialize and the */list calls (same 19 tools as
// attached mode). The first other request starts the app in control mode (if nothing listens on the port), attaches a
// second figcraft-cli to it, replays the client's handshake, and from then on every message goes to the app.
// Claude Code has no auto-retry for stdio MCP servers, so this never fails at session start. From glyph task 35.
import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import readline from "node:readline";

const PORT = Number(process.env.FIGCRAFT_PORT ?? 7777);
const exe = process.platform === "win32" ? ".exe" : "";
const DIR = path.dirname(process.env.FIGCRAFT_CLI || `C:/Program Files/ArtCraft/FigCraft/figcraft-cli${exe}`);
const ROOT = path.resolve(import.meta.dirname, "../..");
const WRITE_ROOT = path.join(ROOT, ".memoir/design");
const ROOTS = ["--automation-read-root", ROOT, "--automation-write-root", WRITE_ROOT];
const log = (msg) => process.stderr.write(`figcraft-mcp: ${msg}\n`);

const listening = () =>
  new Promise((ok) => {
    const s = net.connect({ host: "127.0.0.1", port: PORT });
    s.once("connect", () => (s.destroy(), ok(true)));
    s.once("error", () => ok(false));
  });

// a child figcraft-cli whose stdout lines go to our stdout; the first one to fail to start ends the wrapper
const cli = (args) => {
  const c = spawn(path.join(DIR, `figcraft-cli${exe}`), args, { stdio: ["pipe", "pipe", "inherit"] });
  c.on("error", (e) => (log(`cannot start figcraft-cli in ${DIR} (${e.code}); set FIGCRAFT_CLI`), process.exit(1)));
  c.lines = readline.createInterface({ input: c.stdout });
  return c;
};
const pipeOut = (c) => c.lines.on("line", (l) => process.stdout.write(l + "\n"));

async function startApp() {
  if (await listening()) return true;
  const app = spawn(path.join(DIR, `figcraft${exe}`), ["--control", String(PORT), ...ROOTS], {
    detached: true, // the app outlives this wrapper and the MCP session
    stdio: "ignore",
  });
  let failed = false;
  app.on("error", (e) => ((failed = true), log(`cannot start the FigCraft app in ${DIR} (${e.code})`)));
  app.unref();
  for (const until = Date.now() + 30_000; !failed && Date.now() < until; await new Promise((r) => setTimeout(r, 500)))
    if (await listening()) return true;
  if (!failed) log(`the FigCraft app did not open port ${PORT} within 30 s`);
  return false;
}

// attach to the app and replay the handshake; resolves to the attached child, or null (stay headless)
async function attach(handshake) {
  if (!(await startApp())) return null;
  const c = cli(["mcp", "--attach", String(PORT), "--timeout", "120"]);
  const [init, ...rest] = handshake;
  const id = "figcraft-mcp-replay";
  c.stdin.write(JSON.stringify({ ...JSON.parse(init), id }) + "\n");
  const ok = await new Promise((done) => {
    c.once("exit", () => done(false));
    setTimeout(() => done(false), 10_000); // a silent cli must not wedge every later call
    c.lines.on("line", function first(l) {
      let m = {};
      try {
        m = JSON.parse(l);
      } catch {}
      if (m.id !== id) return process.stdout.write(l + "\n");
      c.lines.off("line", first);
      if (m.result) pipeOut(c); // now, not after the await: later lines of this chunk would be lost
      done(Boolean(m.result));
    });
  });
  if (!ok) {
    c.removeAllListeners("exit");
    c.kill();
    return (log("figcraft-cli --attach failed the handshake (exit, error or 10 s silence)"), null);
  }
  for (const l of rest) c.stdin.write(l + "\n");
  return c;
}

fs.mkdirSync(WRITE_ROOT, { recursive: true }); // a missing root makes figcraft exit (os error 2)
const appUp = await listening();
let active = appUp ? cli(["mcp", "--attach", String(PORT), "--timeout", "120"]) : cli(["mcp", ...ROOTS]);
pipeOut(active);
active.on("exit", (code) => process.exit(code ?? 1));
let state = appUp ? "attached" : "headless"; // headless -> switching -> attached
const handshake = []; // initialize request + notifications/initialized, replayed to the attached cli
const queue = [];

const lazyOk = (m) => !m || m === "initialize" || m === "ping" || m.endsWith("/list") || m.startsWith("notifications/");

async function toApp() {
  state = "switching";
  const c = await attach(handshake);
  const old = active;
  if (c) {
    active = c;
    old.removeAllListeners("exit");
    old.stdin.end(); // it finishes in-flight replies, then exits
    c.on("exit", (code) => process.exit(code ?? 1));
  } else log("staying headless: figcraft tools work without a window");
  state = c ? "attached" : "headless-only";
  for (const l of queue.splice(0)) active.stdin.write(l + "\n");
  if (stdinEnded) active.stdin.end();
}

let stdinEnded = false;
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  if (!line.trim()) return;
  let method;
  try {
    method = JSON.parse(line).method;
  } catch {}
  if (state === "headless" && (method === "initialize" || method === "notifications/initialized")) handshake.push(line);
  if (state === "switching") return queue.push(line);
  if (state === "headless" && !lazyOk(method)) return queue.push(line), toApp();
  active.stdin.write(line + "\n");
}).on("close", () => {
  stdinEnded = true;
  if (state !== "switching") active.stdin.end();
});
for (const sig of ["SIGINT", "SIGTERM"]) process.on(sig, () => (active.kill(sig), process.exit(130)));
