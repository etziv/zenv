import "@xterm/xterm/css/xterm.css";

import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XTerm } from "@xterm/xterm";
import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

import { api } from "@/lib/api/client";
import type { AgentId, TerminalMessage } from "@/lib/api/types";

/** Black like the header panel, in both themes; the LED green as accent. */
const THEME = {
  background: "#0a0b0b",
  foreground: "#d9dddc",
  cursor: "#3ee8a0",
  cursorAccent: "#0a0b0b",
  selectionBackground: "#3ee8a04d",
  black: "#161818",
  brightBlack: "#5c6361",
  green: "#3ee8a0",
  brightGreen: "#7af2c0",
  red: "#ff5a4a",
  brightRed: "#ff8a7e",
};

function toBytes(m: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (m instanceof Uint8Array) return m;
  if (m instanceof ArrayBuffer) return new Uint8Array(m);
  return Uint8Array.from(m);
}

/** WebView2 may not grant the async clipboard to the app's page. */
function copyText(text: string) {
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(text).catch(() => legacyCopy(text));
  } else {
    legacyCopy(text);
  }
}

function legacyCopy(text: string) {
  const area = document.createElement("textarea");
  area.value = text;
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  document.execCommand("copy");
  area.remove();
}

/**
 * One terminal running `agent` in the app's folder. It stays mounted while
 * its tab is hidden, so the agent keeps running; `active` refits it.
 * After the program exits, Enter starts it again.
 */
export function TerminalView({
  siteId,
  agent,
  screen,
  name,
  active,
  onExit,
}: {
  siteId: string;
  agent: AgentId;
  /** One of Claude Code's own screens to open ("/mcp", ...), the first time only. */
  screen?: string;
  name: string;
  active: boolean;
  onExit: (running: boolean) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const exitRef = useRef(onExit);
  exitRef.current = onExit;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new XTerm({
      fontFamily: '"JetBrains Mono Variable", "Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      allowProposedApi: true,
      scrollback: 10000,
      theme: THEME,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    let session: string | null = null;
    let starting = false;
    // Typed (or answered by xterm, like the console's cursor-position
    // query) before the terminal's id arrived; sent once it does.
    let pending = "";
    let disposed = false;

    // Started again after it exits: a plain session, not the screen once more.
    let firstScreen: string | null = screen ?? null;
    const start = () => {
      if (starting || disposed) return;
      starting = true;
      const channel = new Channel<TerminalMessage>();
      channel.onmessage = (m) => {
        if (disposed) return;
        if (m && typeof m === "object" && "exit" in m && !Array.isArray(m)) {
          session = null;
          exitRef.current(false);
          const code = m.exit === null ? "" : ` (exit code ${m.exit})`;
          term.write(`\r\n\x1b[90m[${name} ended${code}. Press Enter to start it again.]\x1b[0m\r\n`);
          return;
        }
        term.write(toBytes(m as ArrayBuffer | Uint8Array | number[]));
      };
      api
        .terminalOpen(siteId, agent, firstScreen, term.cols, term.rows, channel)
        .then((id) => {
          if (disposed) {
            void api.terminalClose(id).catch(() => {});
            return;
          }
          session = id;
          firstScreen = null;
          exitRef.current(true);
          if (pending) void api.terminalWrite(id, pending).catch(() => {});
          pending = "";
        })
        .catch((e: Error) => {
          pending = "";
          term.write(`\x1b[31m${e.message}\x1b[0m\r\n\x1b[90mPress Enter to try again.\x1b[0m\r\n`);
          exitRef.current(false);
        })
        .finally(() => {
          starting = false;
        });
    };

    // Ctrl+C copies when text is selected (else it interrupts); Ctrl+V and
    // Ctrl+Shift+V paste through the browser. Shift+Enter is a new line in
    // the agent's prompt (sent as Alt+Enter).
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      if (e.ctrlKey && (e.key === "c" || e.key === "C") && term.hasSelection()) {
        copyText(term.getSelection());
        term.clearSelection();
        return false;
      }
      if (e.ctrlKey && (e.key === "v" || e.key === "V")) return false;
      if (e.shiftKey && e.key === "Enter" && session) {
        void api.terminalWrite(session, "\x1b\r").catch(() => {});
        return false;
      }
      return true;
    });

    const input = term.onData((data) => {
      if (session) {
        void api.terminalWrite(session, data).catch(() => {});
      } else if (starting) {
        pending += data;
      } else if (data.includes("\r")) {
        term.reset();
        start();
      }
    });
    const resized = term.onResize(({ cols, rows }) => {
      if (session) void api.terminalResize(session, cols, rows).catch(() => {});
    });

    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (el.offsetWidth > 0 && el.offsetHeight > 0) fit.fit();
      });
    });
    observer.observe(el);

    start();

    return () => {
      disposed = true;
      observer.disconnect();
      cancelAnimationFrame(frame);
      input.dispose();
      resized.dispose();
      if (session) void api.terminalClose(session).catch(() => {});
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [siteId, agent, name]);

  useEffect(() => {
    if (!active) return;
    const id = requestAnimationFrame(() => {
      fitRef.current?.fit();
      termRef.current?.focus();
    });
    return () => cancelAnimationFrame(id);
  }, [active]);

  return <div ref={host} className="h-full w-full px-2 pt-1.5" />;
}
