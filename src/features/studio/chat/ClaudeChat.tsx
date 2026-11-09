import { Channel } from "@tauri-apps/api/core";
import {
  BotIcon,
  ChevronRightIcon,
  FileIcon,
  FilePenIcon,
  FileTextIcon,
  GlobeIcon,
  HistoryIcon,
  ListChecksIcon,
  ListTodoIcon,
  SearchIcon,
  SquarePenIcon,
  TerminalIcon,
  WrenchIcon,
} from "lucide-react";
import type * as React from "react";
import { memo, useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { api } from "@/lib/api/client";
import type { ChatAttachment, ChatCommand, ChatControl, ChatLine, ChatOptions, ChatSession, McpServer } from "@/lib/api/types";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";

import { panelButton } from "../ui";
import { Composer } from "./Composer";
import { Markdown } from "./Markdown";
import { chatReducer, initialChat, type Item } from "./model";
import { ago, FALLBACK_COMMANDS, FALLBACK_MODELS, prettyModel, readCatalog, readOptions, sameOptions, saveCatalog, saveOptions } from "./options";

/** Older items stay out of the page until asked for, so long sessions stay light. */
const SHOWN = 200;

type ToolItem = Extract<Item, { kind: "tool" }>;
type Answer = (requestId: string, allow: boolean, always: boolean) => void;

/** Claude Code as a conversation, like its VS Code panel: `claude` runs over pipes, not in a terminal. */
export function ClaudeChat({
  siteId,
  projectRoot,
  active,
  activeFile,
  onBusy,
  onOpenFile,
  onOpenTerminal,
}: {
  siteId: string;
  projectRoot: string;
  active: boolean;
  /** The file in front in Studio's editor (relative path). */
  activeFile: string | null;
  onBusy: (busy: boolean) => void;
  /** Opens a project file (relative path) in Studio's editor. */
  onOpenFile: (path: string) => void;
  /** A Claude Code terminal tab, on one of its own screens ("/mcp", ...) or plain. */
  onOpenTerminal: (screen?: string) => void;
}) {
  const [state, dispatch] = useReducer(chatReducer, initialChat);
  const [showAll, setShowAll] = useState(false);
  const [options, setOptions] = useState(readOptions);
  const [catalog, setCatalog] = useState(readCatalog);
  const [sessions, setSessions] = useState<ChatSession[] | null>(null);
  const session = useRef<string | null>(null);
  const opening = useRef<Promise<string> | null>(null);
  /** The options the running program was started with. */
  const applied = useRef<ChatOptions | null>(null);
  // Bumped by "New chat", a restart and unmount: lines from an older program are ignored.
  const generation = useRef(0);
  const queue = useRef<ChatLine[]>([]);
  const frame = useRef(0);
  // Answers to control requests (MCP status, ...), by request id; early ones wait in `early`.
  const awaiting = useRef(new Map<string, (r: Record<string, unknown>) => void>());
  const early = useRef(new Map<string, Record<string, unknown>>());
  const latest = useRef({ sessionId: state.sessionId, options, busy: state.busy });
  latest.current = { sessionId: state.sessionId, options, busy: state.busy };

  // Streamed text arrives in many small lines: apply them once per frame.
  const flush = useCallback(() => {
    frame.current = 0;
    const lines = queue.current;
    queue.current = [];
    if (lines.length) dispatch({ type: "lines", lines });
  }, []);

  const ensure = useCallback((): Promise<string> => {
    if (session.current) return Promise.resolve(session.current);
    if (opening.current) return opening.current;
    const gen = generation.current;
    const startedWith = latest.current.options;
    let ended = false;
    const channel = new Channel<ChatLine>();
    channel.onmessage = (line) => {
      if (gen !== generation.current) return;
      if (line.type === "zenv_exit") {
        ended = true;
        session.current = null;
      }
      if (line.type === "control_response") {
        const response = (line.response ?? {}) as Record<string, unknown>;
        const rid = String(response.request_id ?? "");
        const done = awaiting.current.get(rid);
        if (done) {
          awaiting.current.delete(rid);
          done(response);
        } else if (rid.startsWith("zenv-")) early.current.set(rid, response);
        return;
      }
      queue.current.push(line);
      if (!frame.current) frame.current = requestAnimationFrame(flush);
    };
    const started = api.chatOpen(siteId, latest.current.sessionId, startedWith, channel).then(
      (id) => {
        opening.current = null;
        if (gen !== generation.current) {
          void api.chatClose(id).catch(() => {});
          throw new Error("stale");
        }
        if (!ended) {
          session.current = id;
          applied.current = startedWith;
        }
        return id;
      },
      (e: unknown) => {
        opening.current = null;
        throw e;
      },
    );
    opening.current = started;
    return started;
  }, [siteId, flush]);

  const report = useCallback((e: unknown) => {
    const message = e instanceof Error ? e.message : String(e);
    if (message !== "stale") dispatch({ type: "notice", text: message, error: true });
  }, []);

  /** Ends the running program; the next message starts it again on the same session. */
  const end = useCallback(async () => {
    generation.current++;
    const id = session.current;
    session.current = null;
    opening.current = null;
    queue.current = [];
    if (id) await api.chatClose(id).catch(() => {});
  }, []);

  const send = (text: string, attachments: ChatAttachment[]) => {
    if (text === "/clear" && !attachments.length) {
      newChat();
      return;
    }
    dispatch({ type: "sent", text, files: attachments.map((a) => a.name) });
    // A new model, effort or mode needs the program started again (between turns only).
    const restart = session.current && applied.current && !latest.current.busy && !sameOptions(applied.current, latest.current.options);
    (restart ? end() : Promise.resolve())
      .then(ensure)
      .then((id) => api.chatSend(id, text, attachments))
      .catch(report);
  };
  const stop = () => {
    if (session.current) void api.chatInterrupt(session.current).catch(report);
  };
  const answer = useCallback<Answer>(
    (requestId, allow, always) => {
      dispatch({ type: "answered", requestId, allow, always });
      if (session.current) void api.chatAnswer(session.current, requestId, allow, always).catch(report);
    },
    [report],
  );
  const newChat = () => {
    void end();
    setShowAll(false);
    dispatch({ type: "reset" });
  };
  /** Asks the running Claude Code (started if needed: that costs no tokens). */
  const control = async (c: ChatControl): Promise<Record<string, unknown>> => {
    const id = await ensure();
    const rid = await api.chatControl(id, c);
    const answer = await new Promise<Record<string, unknown>>((resolve, reject) => {
      const got = early.current.get(rid);
      if (got) {
        early.current.delete(rid);
        resolve(got);
        return;
      }
      const t = setTimeout(() => {
        awaiting.current.delete(rid);
        reject(new Error("Claude Code did not answer"));
      }, 20_000);
      awaiting.current.set(rid, (r) => {
        clearTimeout(t);
        resolve(r);
      });
    });
    if (answer.subtype !== "success") throw new Error(String(answer.error ?? "Claude Code refused"));
    return (answer.response ?? {}) as Record<string, unknown>;
  };

  const exportChat = () => {
    const md = state.items
      .map((it) => {
        switch (it.kind) {
          case "user":
            return `**You:** ${it.text}${it.files.length ? `\n_(attached: ${it.files.join(", ")})_` : ""}`;
          case "text":
            return it.text;
          case "tool":
            return `> ${it.name}: ${summary(it.name, it.input)}`;
          case "notice":
            return `_${it.text}_`;
          default:
            return null;
        }
      })
      .filter(Boolean)
      .join("\n\n");
    if (!md) {
      toast({ variant: "info", title: "Nothing to export yet" });
      return;
    }
    navigator.clipboard.writeText(md).then(
      () => toast({ variant: "success", title: "Conversation copied", description: "Markdown is on the clipboard." }),
      (e: Error) => toast({ variant: "error", title: "Couldn't copy", description: e.message }),
    );
  };

  /** The project's CLAUDE.md in the editor, created empty when missing. */
  const instructions = () => {
    api
      .projectStat(siteId, ["CLAUDE.md"])
      .then(([t]) => (t == null ? api.projectCreate(siteId, "CLAUDE.md", "file").then(() => undefined) : undefined))
      .then(() => onOpenFile("CLAUDE.md"))
      .catch((e: Error) => toast({ variant: "error", title: "Couldn't open CLAUDE.md", description: e.message }));
  };

  const paletteActions = {
    clear: () => newChat(),
    exportChat,
    terminal: onOpenTerminal,
    instructions,
    docs: () => void api.openClaudeDocs().catch((e: Error) => toast({ variant: "error", title: "Couldn't open the docs", description: e.message })),
    mcp: {
      status: () => control({ kind: "mcpStatus" }).then((r) => (Array.isArray(r.mcpServers) ? (r.mcpServers as McpServer[]) : [])),
      toggle: (server: string, enabled: boolean) => control({ kind: "mcpToggle", server, enabled }),
      reconnect: (server: string) => control({ kind: "mcpReconnect", server }),
    },
  };

  const changeOptions = (o: ChatOptions) => {
    setOptions(o);
    saveOptions(o);
  };

  useEffect(
    () => () => {
      cancelAnimationFrame(frame.current);
      void end();
    },
    [end],
  );

  const busyChanged = useRef(onBusy);
  busyChanged.current = onBusy;
  useEffect(() => busyChanged.current(state.busy), [state.busy]);

  // Claude Code's own list of models (with versions) and commands, asked at most once a day.
  useEffect(() => {
    if (catalog.fresh) return;
    let live = true;
    const t = setTimeout(() => {
      api.chatCatalog(siteId).then(
        (c) => {
          saveCatalog(c);
          if (live) setCatalog({ catalog: c, fresh: true });
        },
        () => {},
      );
    }, 1500);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [catalog.fresh, siteId]);

  const models = catalog.catalog?.models ?? FALLBACK_MODELS;
  const commands: ChatCommand[] = (() => {
    const listed = catalog.catalog?.commands.length ? catalog.catalog.commands : FALLBACK_COMMANDS;
    const extra = (state.commands ?? []).filter((n) => !listed.some((c) => c.name === n)).map((name) => ({ name }));
    return [...listed, ...extra];
  })();

  /** Shows a saved session; the next message continues it. */
  const loadSession = (id: string) => {
    void end();
    setShowAll(false);
    stick.current = true;
    api.chatHistory(siteId, id).then((lines) => dispatch({ type: "history", sessionId: id, lines }), report);
  };

  // "Always allow" that switches the mode changes the running program's mode too.
  useEffect(() => {
    const m = state.mode;
    if (!applied.current || !m || m === "default" || m === applied.current.mode) return;
    if (m === "manual" || m === "acceptEdits" || m === "auto" || m === "plan" || m === "dontAsk" || m === "bypassPermissions") {
      applied.current = { ...applied.current, mode: m };
      setOptions((o) => ({ ...o, mode: m }));
    }
  }, [state.mode]);

  // Follow new output while the view is at the bottom.
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [state.items]);

  const openFile = useCallback(
    (path: string) => {
      const rel = relative(projectRoot, path);
      if (rel) onOpenFile(rel);
    },
    [projectRoot, onOpenFile],
  );

  const hidden = showAll ? 0 : Math.max(0, state.items.length - SHOWN);
  const items = hidden ? state.items.slice(hidden) : state.items;
  const last = state.items.at(-1);
  const waiting = last?.kind === "tool" && last.permission?.state === "pending";
  const working = state.busy && state.streaming === null && !waiting;
  const pending = session.current && applied.current && !sameOptions(applied.current, options);

  return (
    <section className="flex h-full min-h-0 flex-col bg-panel" aria-label="Claude Code chat">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-panel-line pl-3 pr-1 text-[11px] text-panel-muted">
        <span className={cn("size-1.5 rounded-full", state.busy ? "led-on animate-pulse" : session.current ? "led-on" : "led-off")} />
        <span className="min-w-0 truncate font-mono">{prettyModel(state.model) ?? "Claude Code"}</span>
        {pending && <span className="shrink-0 text-amber-400">· new settings apply to your next message</span>}
        <DropdownMenu
          onOpenChange={(open) => {
            if (open) api.chatSessions(siteId).then(setSessions, () => setSessions([]));
          }}
        >
          <DropdownMenuTrigger asChild>
            <button type="button" className={cn(panelButton, "ml-auto size-7")} title="History: this project's Claude Code sessions" aria-label="History">
              <HistoryIcon />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="max-h-96 w-80 overflow-y-auto">
            {sessions === null ? (
              <p className="px-2 py-1.5 text-[12px] text-muted-foreground">Loading…</p>
            ) : sessions.length === 0 ? (
              <p className="px-2 py-1.5 text-[12px] text-muted-foreground">No Claude Code sessions in this project yet.</p>
            ) : (
              sessions.map((h) => (
                <DropdownMenuItem key={h.id} onSelect={() => loadSession(h.id)} className="items-start">
                  <span className="grid min-w-0 flex-1">
                    <span className={cn("truncate", h.id === state.sessionId && "text-led")}>{h.title}</span>
                    <span className="text-[11px] text-muted-foreground">{ago(h.modified)}</span>
                  </span>
                </DropdownMenuItem>
              ))
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        <button type="button" className={cn(panelButton, "size-7")} title="New chat" aria-label="New chat" onClick={newChat}>
          <SquarePenIcon />
        </button>
      </div>

      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
        }}
        className="min-h-0 flex-1 overflow-y-auto px-3 py-3 text-[13px] leading-relaxed text-panel-foreground"
      >
        {state.items.length === 0 ? (
          <div className="grid h-full place-items-center text-center text-panel-muted">
            <div className="grid justify-items-center gap-2">
              <BotIcon className="size-8 opacity-40" />
              <p className="text-sm">Ask Claude Code to build, fix or explain something in this project.</p>
              <p className="font-mono text-[11px]">Enter send · Shift+Enter new line · @ file · / command · paste or drop files</p>
            </div>
          </div>
        ) : (
          <div className="grid gap-3">
            {hidden > 0 && (
              <button type="button" onClick={() => setShowAll(true)} className="justify-self-center text-[11px] text-panel-muted hover:text-panel-foreground">
                Show {hidden} earlier
              </button>
            )}
            {items.map((it) => (
              <ItemView key={it.key} item={it} onAnswer={answer} onOpenFile={openFile} />
            ))}
            {working && (
              <p className="flex items-center gap-2 text-[12px] text-panel-muted">
                <span className="size-1.5 animate-pulse rounded-full bg-led" /> Working…
              </p>
            )}
          </div>
        )}
      </div>

      <Composer
        siteId={siteId}
        active={active}
        busy={state.busy}
        commands={commands}
        models={models}
        catalog={catalog.catalog}
        actions={paletteActions}
        runningModel={state.model}
        activeFile={activeFile}
        options={options}
        onOptions={changeOptions}
        reportedMode={state.mode}
        limits={state.limits}
        onSend={send}
        onStop={stop}
      />
    </section>
  );
}

const ItemView = memo(function ItemView({
  item,
  onAnswer,
  onOpenFile,
}: {
  item: Item;
  onAnswer: Answer;
  onOpenFile: (path: string) => void;
}) {
  switch (item.kind) {
    case "user":
      return (
        <div className="grid max-w-[90%] justify-self-end gap-1.5 rounded-lg bg-panel-raised px-3 py-2 text-white">
          {item.files.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {item.files.map((f, i) => (
                <span key={i} className="flex items-center gap-1 rounded border border-panel-line px-1.5 py-0.5 text-[11px] text-panel-muted">
                  <FileIcon className="size-3" /> {f}
                </span>
              ))}
            </div>
          )}
          {item.text && <p className="whitespace-pre-wrap">{item.text}</p>}
        </div>
      );
    case "text":
      return <Markdown text={item.text} />;
    case "tool":
      return <ToolCard tool={item} onAnswer={onAnswer} onOpenFile={onOpenFile} />;
    case "result":
      return item.error ? (
        <p className="whitespace-pre-wrap rounded-md border border-led-fail/40 px-3 py-2 text-[12px] text-led-fail">{item.text}</p>
      ) : (
        <p className="font-mono text-[10.5px] text-panel-muted">{item.seconds === null ? "done" : `done in ${item.seconds}s`}</p>
      );
    case "notice":
      return <p className={cn("whitespace-pre-wrap font-mono text-[11px]", item.error ? "text-led-fail" : "text-panel-muted")}>{item.text}</p>;
  }
});

const ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  Bash: TerminalIcon,
  PowerShell: TerminalIcon,
  Read: FileTextIcon,
  Write: FilePenIcon,
  Edit: FilePenIcon,
  MultiEdit: FilePenIcon,
  NotebookEdit: FilePenIcon,
  Glob: SearchIcon,
  Grep: SearchIcon,
  WebFetch: GlobeIcon,
  WebSearch: GlobeIcon,
  Task: BotIcon,
  Agent: BotIcon,
  TodoWrite: ListChecksIcon,
  ExitPlanMode: ListTodoIcon,
};

const s = (v: unknown) => (typeof v === "string" ? v : "");

function summary(name: string, input: Record<string, unknown>): string {
  switch (name) {
    case "Bash":
    case "PowerShell":
      return s(input.command);
    case "Read":
    case "Write":
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return s(input.file_path) || s(input.notebook_path);
    case "Glob":
    case "Grep":
      return s(input.pattern);
    case "WebFetch":
      return s(input.url);
    case "WebSearch":
      return s(input.query);
    case "Task":
    case "Agent":
      return s(input.description);
    case "TodoWrite":
      return Array.isArray(input.todos) ? `${input.todos.length} tasks` : "";
    default:
      return Object.values(input).find((v): v is string => typeof v === "string") ?? "";
  }
}

const ToolCard = memo(function ToolCard({
  tool,
  onAnswer,
  onOpenFile,
}: {
  tool: ToolItem;
  onAnswer: Answer;
  onOpenFile: (path: string) => void;
}) {
  const pending = tool.permission?.state === "pending";
  const plan = tool.name === "ExitPlanMode";
  const [open, setOpen] = useState(plan);
  const expanded = open || pending;
  const Icon = ICONS[tool.name] ?? WrenchIcon;
  const text = summary(tool.name, tool.input);
  const file = s(tool.input.file_path);
  const state = tool.result ? (tool.result.error ? "error" : "done") : tool.permission?.state === "denied" ? "error" : "running";

  return (
    <div className={cn("rounded-md border bg-panel-raised/40", pending ? "border-amber-400/50" : "border-panel-line")}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full min-w-0 items-center gap-2 px-2.5 py-1.5 text-left text-[12px]"
        aria-expanded={expanded}
      >
        <ChevronRightIcon className={cn("size-3 shrink-0 text-panel-muted transition-transform", expanded && "rotate-90")} />
        <Icon className="size-3.5 shrink-0 text-panel-muted" />
        <span className="shrink-0 font-medium text-white">{tool.name}</span>
        <span className="min-w-0 truncate font-mono text-[11px] text-panel-muted" title={text}>
          {file ? relativeOrSelf(file) : text}
        </span>
        <span
          className={cn(
            "ml-auto size-1.5 shrink-0 rounded-full",
            state === "done" ? "led-on" : state === "error" ? "bg-led-fail" : pending ? "bg-amber-400" : "animate-pulse bg-panel-muted",
          )}
        />
      </button>
      {expanded && (
        <div className="grid gap-2 border-t border-panel-line px-2.5 py-2">
          <ToolInput tool={tool} />
          {file && (
            <button type="button" onClick={() => onOpenFile(file)} className="justify-self-start font-mono text-[11px] text-led hover:underline">
              Open in editor
            </button>
          )}
          {tool.result && (
            <pre
              className={cn(
                "max-h-60 overflow-auto whitespace-pre-wrap break-words rounded bg-black/40 p-2 font-mono text-[11px]",
                tool.result.error ? "text-led-fail" : "text-panel-muted",
              )}
            >
              {tool.result.text || "(no output)"}
            </pre>
          )}
        </div>
      )}
      {pending && tool.permission && (
        <div className="flex flex-wrap items-center gap-2 border-t border-panel-line px-2.5 py-2 text-[12px]">
          <span className="mr-auto text-panel-foreground">{plan ? "Start with this plan?" : `Allow Claude to use ${tool.name}?`}</span>
          <button
            type="button"
            onClick={() => onAnswer(tool.permission!.requestId, true, false)}
            className="rounded bg-led px-2.5 py-1 font-medium text-black hover:bg-led/90"
          >
            {plan ? "Approve" : "Allow"}
          </button>
          {tool.permission.always && (
            <button
              type="button"
              onClick={() => onAnswer(tool.permission!.requestId, true, true)}
              className="rounded border border-led/50 px-2.5 py-1 text-led hover:bg-led/10"
            >
              {tool.permission.always}
            </button>
          )}
          <button
            type="button"
            onClick={() => onAnswer(tool.permission!.requestId, false, false)}
            className="rounded border border-panel-line px-2.5 py-1 text-panel-foreground hover:bg-panel-raised"
          >
            {plan ? "Keep planning" : "Deny"}
          </button>
        </div>
      )}
      {tool.permission && !pending && !tool.result && (
        <p className="border-t border-panel-line px-2.5 py-1 text-[11px] text-panel-muted">{tool.permission.state === "allowed" ? "Allowed" : "Denied"}</p>
      )}
    </div>
  );
});

function ToolInput({ tool }: { tool: ToolItem }) {
  const { name, input } = tool;
  const pre = "max-h-60 overflow-auto whitespace-pre-wrap break-words rounded bg-black/40 p-2 font-mono text-[11px]";
  if (name === "Bash" || name === "PowerShell") {
    return <pre className={cn(pre, "text-panel-foreground")}>{s(input.command)}</pre>;
  }
  if (name === "Edit") {
    return (
      <pre className={pre}>
        {lines(s(input.old_string)).map((l, i) => (
          <span key={`o${i}`} className="block bg-led-fail/10 text-led-fail">
            - {l}
          </span>
        ))}
        {lines(s(input.new_string)).map((l, i) => (
          <span key={`n${i}`} className="block bg-led/10 text-led">
            + {l}
          </span>
        ))}
      </pre>
    );
  }
  if (name === "Write") {
    return <pre className={cn(pre, "text-panel-foreground")}>{clip(s(input.content))}</pre>;
  }
  if (name === "ExitPlanMode" && typeof input.plan === "string") {
    return <Markdown text={input.plan} />;
  }
  if (name === "TodoWrite" && Array.isArray(input.todos)) {
    return (
      <ul className="grid gap-0.5 text-[12px]">
        {(input.todos as { content?: string; status?: string }[]).map((t, i) => (
          <li key={i} className={cn(t.status === "completed" && "text-panel-muted line-through", t.status === "in_progress" && "text-led")}>
            {t.content}
          </li>
        ))}
      </ul>
    );
  }
  return <pre className={cn(pre, "text-panel-muted")}>{clip(JSON.stringify(input, null, 2))}</pre>;
}

const lines = (t: string) => t.split("\n").slice(0, 80);
const clip = (t: string) => (t.length > 4000 ? `${t.slice(0, 4000)}\n…` : t);

/** A project file's path relative to the project root, or null outside it. */
export function relative(root: string, path: string): string | null {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  const r = norm(root);
  const f = norm(path);
  if (f.toLowerCase().startsWith(`${r.toLowerCase()}/`)) return f.slice(r.length + 1);
  if (!/^([a-zA-Z]:)?\//.test(f)) return f;
  return null;
}

function relativeOrSelf(path: string) {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts.length > 3 ? `…/${parts.slice(-3).join("/")}` : path;
}
