import { PlusIcon, XIcon } from "lucide-react";
import type * as React from "react";
import { useCallback, useRef, useState } from "react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { AgentId, AgentInfo } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { ClaudeChat } from "./chat/ClaudeChat";
import { TerminalView } from "./Terminal";
import { panelButton } from "./ui";

/** A Claude Code chat, or a program in a terminal. */
type Tab = { key: number; name: string; running: boolean } & ({ kind: "chat" } | { kind: "terminal"; agent: AgentId; screen?: string });

/** Tabs: the Claude Code chat first, then whatever the user adds. */
export function TerminalPanel({
  siteId,
  projectRoot,
  activeFile,
  agents,
  onRefreshAgents,
  onOpenFile,
  actions,
}: {
  siteId: string;
  projectRoot: string;
  /** The file in front in the editor, for the chat's "add file". */
  activeFile: string | null;
  agents: AgentInfo[];
  onRefreshAgents: () => void;
  onOpenFile: (path: string) => void;
  /** Buttons at the right of the tab bar (hide panel, ...). */
  actions?: React.ReactNode;
}) {
  const next = useRef(1);
  const claude = agents.find((a) => a.id === "claudeCode");
  const [tabs, setTabs] = useState<Tab[]>(() => {
    if (claude?.program) return [{ key: 0, kind: "chat", name: "Claude", running: false }];
    const shell = agents.find((a) => a.id === "shell");
    return shell ? [{ key: 0, kind: "terminal", agent: shell.id, name: shell.name, running: false }] : [];
  });
  const [active, setActive] = useState(0);

  const add = (tab: { kind: "chat" } | { kind: "terminal"; agent: AgentId; screen?: string }, base: string) => {
    const key = next.current++;
    const same = tabs.filter((t) => t.name === base || t.name.startsWith(`${base} `)).length;
    setTabs((t) => [...t, { ...tab, key, name: same ? `${base} ${same + 1}` : base, running: false } as Tab]);
    setActive(key);
  };
  const close = (key: number) => {
    const rest = tabs.filter((x) => x.key !== key);
    setTabs(rest);
    const last = rest.at(-1);
    if (key === active && last) setActive(last.key);
  };
  const setRunning = useCallback(
    (key: number, running: boolean) =>
      setTabs((t) => (t.some((x) => x.key === key && x.running !== running) ? t.map((x) => (x.key === key ? { ...x, running } : x)) : t)),
    [],
  );

  return (
    <section className="flex h-full min-h-0 flex-col" aria-label="Claude Code and terminals">
      <div className="flex h-9 shrink-0 items-stretch border-b border-panel-line pl-1 pr-1">
        <div role="tablist" className="flex min-w-0 items-stretch overflow-x-auto [scrollbar-width:none]">
          {tabs.map((t) => (
            <div
              key={t.key}
              className={cn(
                "group relative flex shrink-0 items-center gap-2 pl-3 pr-1 text-[12px] transition-colors",
                t.key === active ? "text-white" : "text-panel-muted hover:text-panel-foreground",
              )}
            >
              <button
                type="button"
                role="tab"
                aria-selected={t.key === active}
                onClick={() => setActive(t.key)}
                className="flex items-center gap-2 focus-visible:outline-none"
              >
                <span className={cn("size-1.5 rounded-full", t.running ? "led-on" : "led-off")} />
                {t.name}
              </button>
              <button
                type="button"
                aria-label={`Close ${t.name}`}
                title="Close (ends the program)"
                onClick={() => close(t.key)}
                className="grid size-5 place-items-center rounded-sm opacity-0 transition-opacity hover:bg-panel-raised group-hover:opacity-100 focus-visible:opacity-100 [&_svg]:size-3"
              >
                <XIcon />
              </button>
              <span
                className={cn(
                  "absolute inset-x-2 bottom-0 h-[2px] rounded-t-full bg-led transition-opacity",
                  t.key === active ? "opacity-100" : "opacity-0",
                )}
              />
            </div>
          ))}
        </div>
        <DropdownMenu onOpenChange={(open) => open && onRefreshAgents()}>
          <DropdownMenuTrigger asChild>
            <button type="button" className={cn(panelButton, "my-0.5 ml-1")} aria-label="New tab" title="New chat or terminal">
              <PlusIcon />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem
              disabled={!claude?.program}
              onSelect={() => add({ kind: "chat" }, "Claude")}
              title={claude?.program ?? `Not installed. ${claude?.installHint ?? ""}`}
            >
              Claude Code chat
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <p className="px-2 py-1 text-[11px] text-muted-foreground">Terminal</p>
            {agents.map((a) => (
              <DropdownMenuItem
                key={a.id}
                disabled={!a.program}
                onSelect={() => add({ kind: "terminal", agent: a.id }, a.name)}
                title={a.program ?? `Not installed. ${a.installHint}`}
              >
                {a.name}
                {!a.program && <span className="ml-auto pl-4 text-[11px] text-muted-foreground">not installed</span>}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <div className="ml-auto flex items-center">{actions}</div>
      </div>

      <div className="relative min-h-0 flex-1">
        {tabs.map((t) => (
          <div key={t.key} className={cn("absolute inset-0", t.key !== active && "invisible")}>
            {t.kind === "chat" ? (
              <ClaudeChat
                siteId={siteId}
                projectRoot={projectRoot}
                activeFile={activeFile}
                active={t.key === active}
                onBusy={(busy) => setRunning(t.key, busy)}
                onOpenFile={onOpenFile}
                onOpenTerminal={(screen) => add({ kind: "terminal", agent: "claudeCode", screen }, screen ? `Claude ${screen}` : "Claude Code")}
              />
            ) : (
              <TerminalView
                siteId={siteId}
                agent={t.agent}
                screen={t.screen}
                name={t.name}
                active={t.key === active}
                onExit={(running) => setRunning(t.key, running)}
              />
            )}
          </div>
        ))}
        {tabs.length === 0 && (
          <div className="grid h-full place-items-center p-6 text-center text-sm text-panel-muted">
            <div className="grid gap-2">
              <p>Nothing open.</p>
              {!claude?.program && (
                <p className="font-mono text-[11px]">
                  Claude Code isn't installed: <span className="text-panel-foreground">{claude?.installHint}</span>
                </p>
              )}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
