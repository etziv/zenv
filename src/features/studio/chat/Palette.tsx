import { ArrowLeftIcon, CheckIcon, RefreshCwIcon, TerminalIcon } from "lucide-react";
import type * as React from "react";
import { useEffect, useRef, useState } from "react";

import type { ChatCatalog, ChatEffort, ChatModel, ChatOptions, McpServer } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import type { Limit } from "./model";
import { EFFORTS, prettyModel, untilText } from "./options";

export interface PaletteActions {
  attach: () => void;
  mention: () => void;
  clear: () => void;
  exportChat: () => void;
  commands: () => void;
  /** A Claude Code terminal, on one of its screens ("/mcp", ...) or plain. */
  terminal: (screen?: string) => void;
  instructions: () => void;
  docs: () => void;
  mcp: {
    status: () => Promise<McpServer[]>;
    toggle: (server: string, enabled: boolean) => Promise<unknown>;
    reconnect: (server: string) => Promise<unknown>;
  };
}

type View = "main" | "models" | "styles" | "mcp" | "account";

interface Row {
  section: string;
  label: string;
  detail?: string;
  right?: React.ReactNode;
  run: () => void;
  /** Stays open after running (toggles, steps). */
  stay?: boolean;
}

const LEVELS: ChatEffort[] = ["low", "medium", "high", "xhigh", "max"];

/** The "/" menu, as in Claude Code's VS Code panel: every action, filterable. */
export function Palette({
  catalog,
  models,
  runningModel,
  options,
  onOptions,
  limits,
  actions,
  onClose,
}: {
  catalog: ChatCatalog | null;
  models: ChatModel[];
  runningModel: string | null;
  options: ChatOptions;
  onOptions: (o: ChatOptions) => void;
  limits: { fiveHour?: Limit; sevenDay?: Limit };
  actions: PaletteActions;
  onClose: () => void;
}) {
  const [view, setView] = useState<View>("main");
  const [filter, setFilter] = useState("");
  const [pick, setPick] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), [view]);

  const set = (o: Partial<ChatOptions>) => onOptions({ ...options, ...o });
  const done = (f: () => void) => () => {
    f();
    onClose();
  };
  const chosen = models.find((m) => m.value === (options.model ?? "default"));
  const modelName = options.model ? (chosen?.displayName ?? options.model) : (prettyModel(runningModel) ?? prettyModel(chosen?.resolvedModel ?? null) ?? "Default");
  const levels = chosen?.supportedEffortLevels ?? LEVELS;
  const effortName = EFFORTS.find((e) => e.value === options.effort)?.label ?? "Default";
  const nextEffort = () => {
    const i = options.effort ? levels.indexOf(options.effort) : -1;
    set({ effort: i + 1 < levels.length ? (levels[i + 1] ?? null) : null });
  };

  const rows: Row[] = [
    { section: "Context", label: "Attach file…", run: done(actions.attach) },
    { section: "Context", label: "Mention file from this project…", run: done(actions.mention) },
    { section: "Context", label: "Clear conversation", run: done(actions.clear) },
    { section: "Context", label: "Export conversation", detail: "copies it as Markdown", run: done(actions.exportChat) },
    { section: "Model", label: "Switch model…", right: <Muted>{modelName}</Muted>, run: () => go("models") },
    {
      section: "Model",
      label: `Effort (${effortName})`,
      right: <Dots levels={levels} value={options.effort} onPick={(e) => set({ effort: e })} />,
      run: nextEffort,
      stay: true,
    },
    { section: "Model", label: "Thinking", right: <Switch on={options.thinking !== false} />, run: () => set({ thinking: options.thinking === false }), stay: true },
    {
      section: "Model",
      label: "Switch models when a message is flagged",
      right: <Switch on={options.switchOnFlag === true} />,
      run: () => set({ switchOnFlag: options.switchOnFlag !== true }),
      stay: true,
    },
    { section: "Model", label: "Account & usage…", run: () => go("account") },
    {
      section: "Model",
      label: "Fast mode",
      detail: chosen?.supportsFastMode === false ? "not for this model" : undefined,
      right: <Switch on={options.fast === true} />,
      run: () => set({ fast: options.fast !== true }),
      stay: true,
    },
    { section: "Customize", label: "Output styles", right: <Muted>{options.outputStyle ?? catalog?.outputStyle ?? "default"}</Muted>, run: () => go("styles") },
    { section: "Customize", label: "MCP servers", run: () => go("mcp") },
    { section: "Customize", label: "Hooks", right: <Term />, run: done(() => actions.terminal("/hooks")) },
    { section: "Customize", label: "Permissions", right: <Term />, run: done(() => actions.terminal("/permissions")) },
    { section: "Customize", label: "Status", right: <Term />, run: done(() => actions.terminal("/status")) },
    { section: "Customize", label: "Sandbox", right: <Term />, run: done(() => actions.terminal("/sandbox")) },
    { section: "Customize", label: "Slash commands", run: done(actions.commands) },
    { section: "Customize", label: "Memory", right: <Term />, run: done(() => actions.terminal("/memory")) },
    { section: "Customize", label: "Instructions", detail: "CLAUDE.md in the editor", run: done(actions.instructions) },
    { section: "Customize", label: "Agents", right: <Term />, run: done(() => actions.terminal("/agents")) },
    { section: "Customize", label: "Manage plugins", right: <Term />, run: done(() => actions.terminal("/plugin")) },
    { section: "Customize", label: "Open Claude in Terminal", right: <Term />, run: done(() => actions.terminal()) },
    { section: "Settings", label: "Switch account", right: <Term />, run: done(() => actions.terminal("/login")) },
    { section: "Settings", label: "Sign out", right: <Term />, run: done(() => actions.terminal("/logout")) },
    { section: "Settings", label: "General config…", right: <Term />, run: done(() => actions.terminal("/config")) },
    { section: "Support", label: "View help docs", run: done(actions.docs) },
    { section: "Support", label: "Doctor", detail: "checks the installation", right: <Term />, run: done(() => actions.terminal("/doctor")) },
  ];

  function go(v: View) {
    setView(v);
    setFilter("");
    setPick(0);
  }

  const q = filter.trim().toLowerCase();
  const shown = view === "main" ? rows.filter((r) => !q || r.label.toLowerCase().includes(q) || r.section.toLowerCase().includes(q)) : [];

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (view === "main") onClose();
      else go("main");
      return;
    }
    if (view !== "main" || !shown.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setPick((p) => (p + (e.key === "ArrowDown" ? 1 : shown.length - 1)) % shown.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      shown[Math.min(pick, shown.length - 1)]?.run();
    }
  };

  let lastSection = "";
  return (
    <div className="absolute inset-x-0 bottom-full z-40 mb-1 flex max-h-[60vh] flex-col overflow-hidden rounded-md border border-panel-line bg-panel-raised shadow-xl" onKeyDown={onKey}>
      <div className="flex shrink-0 items-center gap-2 border-b border-panel-line px-2">
        {view !== "main" && (
          <button type="button" onClick={() => go("main")} aria-label="Back" className="grid size-6 place-items-center rounded text-panel-muted hover:text-white">
            <ArrowLeftIcon className="size-3.5" />
          </button>
        )}
        <input
          ref={input}
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setPick(0);
          }}
          placeholder={view === "main" ? "Filter actions…" : TITLES[view]}
          readOnly={view !== "main"}
          className="h-9 min-w-0 flex-1 bg-transparent text-[12.5px] text-white placeholder:text-panel-muted focus:outline-none"
        />
      </div>
      <div className="min-h-0 overflow-y-auto p-1 text-[12.5px]">
        {view === "main" &&
          shown.map((r, i) => {
            const header = r.section !== lastSection ? r.section : null;
            lastSection = r.section;
            return (
              <div key={r.label}>
                {header && <p className={cn("px-2 pb-1 pt-2 text-[11px] text-panel-muted", i > 0 && "mt-1 border-t border-panel-line")}>{header}</p>}
                <button
                  type="button"
                  onMouseEnter={() => setPick(i)}
                  onClick={r.run}
                  className={cn("flex w-full items-center gap-3 rounded px-2 py-1.5 text-left", i === pick ? "bg-panel-line text-white" : "text-panel-foreground")}
                >
                  <span className="min-w-0 truncate">
                    {r.label}
                    {r.detail && <span className="ml-2 text-[11px] text-panel-muted">{r.detail}</span>}
                  </span>
                  <span className="ml-auto flex shrink-0 items-center">{r.right}</span>
                </button>
              </div>
            );
          })}
        {view === "main" && !shown.length && <p className="px-2 py-2 text-panel-muted">No action matches.</p>}
        {view === "models" &&
          models.map((m) => {
            const value = m.value === "default" ? null : m.value;
            return (
              <Choice key={m.value} on={options.model === value} onClick={done(() => set({ model: value }))} label={m.displayName} detail={m.description} />
            );
          })}
        {view === "styles" &&
          (catalog?.outputStyles?.length ? catalog.outputStyles : ["default"]).map((s) => (
            <Choice key={s} on={(options.outputStyle ?? catalog?.outputStyle ?? "default") === s} onClick={done(() => set({ outputStyle: s === "default" ? null : s }))} label={s} />
          ))}
        {view === "mcp" && <McpView actions={actions} onClose={onClose} />}
        {view === "account" && <AccountView catalog={catalog} limits={limits} />}
      </div>
    </div>
  );
}

const TITLES: Record<View, string> = { main: "", models: "Switch model", styles: "Output style", mcp: "MCP servers", account: "Account & usage" };

function Muted({ children }: { children: React.ReactNode }) {
  return <span className="max-w-40 truncate text-[11.5px] text-panel-muted">{children}</span>;
}

function Term() {
  return <TerminalIcon className="size-3.5 text-panel-muted" aria-label="opens in a terminal" />;
}

function Switch({ on }: { on: boolean }) {
  return (
    <span className={cn("relative h-4 w-7 rounded-full transition-colors", on ? "bg-led/80" : "bg-panel-line")}>
      <span className={cn("absolute top-0.5 size-3 rounded-full bg-white transition-all", on ? "left-3.5" : "left-0.5")} />
    </span>
  );
}

function Dots({ levels, value, onPick }: { levels: ChatEffort[]; value: ChatEffort | null; onPick: (e: ChatEffort | null) => void }) {
  return (
    <span className="flex items-center gap-1 rounded-full bg-panel-line/70 px-1.5 py-1" onClick={(e) => e.stopPropagation()}>
      {levels.map((l) => (
        <span
          key={l}
          role="button"
          tabIndex={-1}
          title={EFFORTS.find((e) => e.value === l)?.label}
          onClick={() => onPick(value === l ? null : l)}
          className={cn("rounded-full transition-all", value === l ? "size-2.5 bg-white" : "size-1.5 bg-panel-muted hover:bg-panel-foreground")}
        />
      ))}
    </span>
  );
}

function Choice({ on, onClick, label, detail }: { on: boolean; onClick: () => void; label: string; detail?: string }) {
  return (
    <button type="button" onClick={onClick} className="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left text-panel-foreground hover:bg-panel-line hover:text-white">
      <CheckIcon className={cn("mt-0.5 size-3.5 shrink-0 text-led", !on && "opacity-0")} />
      <span className="grid min-w-0">
        <span>{label}</span>
        {detail && <span className="text-[11px] text-panel-muted">{detail}</span>}
      </span>
    </button>
  );
}

function McpView({ actions, onClose }: { actions: PaletteActions; onClose: () => void }) {
  const [servers, setServers] = useState<McpServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = () => actions.mcp.status().then(setServers, (e: Error) => setError(e.message));
  useEffect(() => {
    void load();
  }, []);
  const act = (name: string, p: Promise<unknown>) => {
    setBusy(name);
    p.then(load, (e: Error) => setError(e.message)).finally(() => setBusy(null));
  };
  return (
    <div className="grid gap-1 p-1">
      {servers === null && !error && <p className="px-1 py-1 text-panel-muted">Asking Claude Code…</p>}
      {error && <p className="px-1 py-1 text-led-fail">{error}</p>}
      {servers?.length === 0 && <p className="px-1 py-1 text-panel-muted">No MCP servers are set up.</p>}
      {servers?.map((s) => {
        const enabled = s.status !== "disabled";
        return (
          <div key={s.name} className="flex items-center gap-2 rounded px-1 py-1.5">
            <span
              className={cn(
                "size-1.5 shrink-0 rounded-full",
                s.status === "connected" ? "led-on" : s.status === "failed" ? "bg-led-fail" : s.status === "disabled" ? "led-off" : "animate-pulse bg-amber-400",
              )}
            />
            <span className="grid min-w-0 flex-1">
              <span className="truncate text-panel-foreground">{s.name}</span>
              <span className="truncate text-[11px] text-panel-muted">
                {s.status}
                {s.tools ? ` · ${s.tools} tools` : ""}
                {s.scope ? ` · ${s.scope}` : ""}
                {s.error ? ` · ${s.error}` : ""}
              </span>
            </span>
            {(s.status === "failed" || s.status === "needs-auth") && (
              <button type="button" title="Reconnect" disabled={busy === s.name} onClick={() => act(s.name, actions.mcp.reconnect(s.name))} className="grid size-6 place-items-center rounded text-panel-muted hover:text-white">
                <RefreshCwIcon className={cn("size-3.5", busy === s.name && "animate-spin")} />
              </button>
            )}
            <button type="button" title={enabled ? "Turn off for this chat" : "Turn on"} disabled={busy === s.name} onClick={() => act(s.name, actions.mcp.toggle(s.name, !enabled))}>
              <Switch on={enabled} />
            </button>
          </div>
        );
      })}
      <button
        type="button"
        onClick={() => {
          actions.terminal("/mcp");
          onClose();
        }}
        className="mt-1 flex items-center gap-2 rounded border-t border-panel-line px-1 pt-2 text-left text-[12px] text-led hover:underline"
      >
        <TerminalIcon className="size-3.5" /> Add or configure servers (/mcp in a terminal)
      </button>
    </div>
  );
}

function AccountView({ catalog, limits }: { catalog: ChatCatalog | null; limits: { fiveHour?: Limit; sevenDay?: Limit } }) {
  const a = catalog?.account;
  const bar = (label: string, w?: Limit) =>
    w && (
      <div className="grid gap-1">
        <div className="flex text-[12px]">
          <span className="text-panel-foreground">{label}</span>
          <span className="ml-auto text-panel-muted">
            {Math.round(w.utilization * 100)}% · resets in {untilText(w.resetsAt)}
          </span>
        </div>
        <span className="h-1.5 overflow-hidden rounded-full bg-panel-line">
          <span className={cn("block h-full", w.utilization >= 0.9 ? "bg-led-fail" : w.utilization >= 0.75 ? "bg-amber-400" : "bg-led")} style={{ width: `${Math.min(100, w.utilization * 100)}%` }} />
        </span>
      </div>
    );
  return (
    <div className="grid gap-3 p-2">
      <div className="grid gap-0.5 text-[12px]">
        <span className="text-white">{a?.email ?? "Account unknown until Claude Code answers"}</span>
        {(a?.subscriptionType || a?.organization) && (
          <span className="text-panel-muted">{[a.subscriptionType && `Claude ${a.subscriptionType}`, a.organization].filter(Boolean).join(" · ")}</span>
        )}
      </div>
      {bar("5-hour limit", limits.fiveHour)}
      {bar("Weekly limit", limits.sevenDay)}
      {!limits.fiveHour && <p className="text-[11px] text-panel-muted">Usage shows after the first message of this chat.</p>}
      {catalog?.version && <p className="text-[11px] text-panel-muted">Claude Code {catalog.version}</p>}
    </div>
  );
}
