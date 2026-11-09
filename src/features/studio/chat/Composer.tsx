import {
  ArrowUpIcon,
  AtSignIcon,
  BanIcon,
  CheckIcon,
  ClockIcon,
  FileIcon,
  FilePenIcon,
  FilePlusIcon,
  HandIcon,
  ListTodoIcon,
  PaperclipIcon,
  PlusIcon,
  Settings2Icon,
  ShieldOffIcon,
  SlashIcon,
  SquareIcon,
  XIcon,
  ZapIcon,
} from "lucide-react";
import type * as React from "react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { api } from "@/lib/api/client";
import type { ChatAttachment, ChatCatalog, ChatCommand, ChatMode, ChatModel, ChatOptions } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import type { Limit } from "./model";
import { fileIcon } from "../editor/fileIcons";
import { Palette, type PaletteActions } from "./Palette";
import { EFFORTS, MODES, prettyModel, toAttachment, untilText } from "./options";

interface Pending {
  id: number;
  file: ChatAttachment;
  /** An object URL for an image's thumbnail. */
  preview?: string;
}

type Menu = { kind: "slash" | "mention"; query: string; start: number };

const MODE_ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  manual: HandIcon,
  default: HandIcon,
  acceptEdits: FilePenIcon,
  auto: ZapIcon,
  plan: ListTodoIcon,
  dontAsk: BanIcon,
  bypassPermissions: ShieldOffIcon,
};

const chip =
  "flex h-7 items-center gap-1.5 rounded-md px-2 text-[11.5px] text-panel-muted transition-colors hover:bg-panel-line/60 hover:text-panel-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led";

/** The prompt box with VS Code's controls: attach, commands, usage, model and effort, permission mode. */
export function Composer({
  siteId,
  active,
  busy,
  commands,
  models,
  catalog,
  actions,
  runningModel,
  activeFile,
  options,
  onOptions,
  reportedMode,
  limits,
  onSend,
  onStop,
}: {
  siteId: string;
  active: boolean;
  busy: boolean;
  commands: ChatCommand[];
  /** As Claude Code lists them; "default" stands for no `--model`. */
  models: ChatModel[];
  catalog: ChatCatalog | null;
  /** What the "/" menu does beyond the composer itself. */
  actions: Omit<PaletteActions, "attach" | "mention" | "commands">;
  /** The model id Claude Code is running, when known. */
  runningModel: string | null;
  /** The file in front in Studio's editor. */
  activeFile: string | null;
  options: ChatOptions;
  onOptions: (o: ChatOptions) => void;
  /** The permission mode Claude Code reports. */
  reportedMode: string | null;
  limits: { fiveHour?: Limit; sevenDay?: Limit };
  onSend: (text: string, attachments: ChatAttachment[]) => void;
  onStop: () => void;
}) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<Pending[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [found, setFound] = useState<string[]>([]);
  const [pick, setPick] = useState(0);
  const [palette, setPalette] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const nextId = useRef(1);

  useEffect(() => {
    if (active) box.current?.focus();
  }, [active]);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);
  // Object URLs of thumbnails go with the composer.
  const filesRef = useRef(files);
  filesRef.current = files;
  useEffect(() => () => filesRef.current.forEach((f) => f.preview && URL.revokeObjectURL(f.preview)), []);

  // Files are searched only while the @ menu is open, a moment after typing stops.
  useEffect(() => {
    if (menu?.kind !== "mention") return;
    if (!menu.query) {
      setFound(activeFile ? [activeFile] : []);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      api.projectSearch(siteId, menu.query).then(
        (r) => live && setFound(r.filter((e) => !e.dir).slice(0, 8).map((e) => e.path)),
        () => live && setFound([]),
      );
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [menu?.kind, menu?.query, siteId, activeFile]);

  const q = menu?.query.toLowerCase() ?? "";
  const options_: { value: string; hint?: string }[] =
    menu?.kind === "slash"
      ? commands
          .filter((c) => c.name.toLowerCase().includes(q))
          .sort((a, b) => Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)))
          .slice(0, 10)
          .map((c) => ({ value: c.name, hint: c.description }))
      : menu?.kind === "mention"
        ? found.map((f) => ({ value: f }))
        : [];

  const updateMenu = (value: string, caret: number) => {
    const before = value.slice(0, caret);
    const slash = /^\/(\S*)$/.exec(before);
    const mention = /(^|\s)@([^\s@]*)$/.exec(before);
    const next: Menu | null = slash
      ? { kind: "slash", query: slash[1] ?? "", start: 0 }
      : mention
        ? { kind: "mention", query: mention[2] ?? "", start: caret - (mention[2] ?? "").length - 1 }
        : null;
    setMenu(next);
    if (next?.kind !== menu?.kind || next?.query !== menu?.query) setPick(0);
  };

  const insert = (value: string, at: number, removeTo: number) => {
    const caret = at + value.length;
    setText((t) => t.slice(0, at) + value + t.slice(removeTo));
    setMenu(null);
    requestAnimationFrame(() => {
      box.current?.focus();
      box.current?.setSelectionRange(caret, caret);
    });
  };

  const choose = (choice: string) => {
    if (!menu) return;
    const caret = box.current?.selectionStart ?? text.length;
    if (menu.kind === "slash") insert(`/${choice} `, 0, caret);
    else insert(`@${choice} `, menu.start, caret);
  };

  const addFiles = async (list: FileList | File[]) => {
    setError(null);
    const added: Pending[] = [];
    for (const f of Array.from(list)) {
      try {
        const file = await toAttachment(f);
        added.push({ id: nextId.current++, file, preview: file.kind === "image" ? URL.createObjectURL(f) : undefined });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    }
    setFiles((cur) => [...cur, ...added].slice(0, 10));
  };
  const removeFile = (id: number) => {
    const gone = files.find((f) => f.id === id);
    if (gone?.preview) URL.revokeObjectURL(gone.preview);
    setFiles((cur) => cur.filter((f) => f.id !== id));
  };

  const submit = () => {
    const t = text.trim();
    if (!t && !files.length) return;
    onSend(t, files.map((f) => f.file));
    files.forEach((f) => f.preview && URL.revokeObjectURL(f.preview));
    setFiles([]);
    setText("");
    setMenu(null);
    setError(null);
  };

  const startWith = (prefix: "/" | "@") => {
    const el = box.current;
    const caret = el?.selectionStart ?? text.length;
    if (prefix === "/") {
      setText("/");
      updateMenu("/", 1);
      requestAnimationFrame(() => {
        el?.focus();
        el?.setSelectionRange(1, 1);
      });
      return;
    }
    const before = text.slice(0, caret);
    const pad = before && !/\s$/.test(before) ? " @" : "@";
    const value = before + pad + text.slice(caret);
    setText(value);
    updateMenu(value, caret + pad.length);
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(caret + pad.length, caret + pad.length);
    });
  };

  const mode = options.mode ?? reportedMode;
  const modeInfo = MODES.find((m) => m.value === options.mode) ?? MODES[0]!;
  const ModeIcon = (mode && MODE_ICONS[mode]) || Settings2Icon;
  const modeLabel = options.mode ? modeInfo.label : mode === "default" || mode === "manual" ? "Ask first" : (MODES.find((m) => m.value === mode)?.label ?? "Settings");
  const chosen = models.find((m) => m.value === (options.model ?? "default"));
  const fallbackDefault = models.find((m) => m.value === "default");
  // The running model says exactly which version; before that, Claude Code's own name for the choice.
  const model = options.model
    ? (chosen?.displayName ?? options.model)
    : (prettyModel(runningModel) ?? prettyModel(fallbackDefault?.resolvedModel ?? null) ?? "Default");
  const levels = chosen?.supportedEffortLevels;
  // Listed by Claude Code (it names the resolved model) without levels: effort can't be set.
  const fixedEffort = Boolean(chosen?.resolvedModel) && !levels;
  const efforts = EFFORTS.filter((e) => e.value === null || (!fixedEffort && (!levels || levels.includes(e.value))));
  const effort = EFFORTS.find((e) => e.value === options.effort && e.value !== null)?.label;

  return (
    <div
      className="shrink-0 border-t border-panel-line p-2"
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) e.preventDefault();
      }}
      onDrop={(e) => {
        if (!e.dataTransfer.files.length) return;
        e.preventDefault();
        void addFiles(e.dataTransfer.files);
      }}
    >
      <div className="relative rounded-lg border border-panel-line bg-panel-raised focus-within:border-panel-muted">
        {palette && (
          <Palette
            catalog={catalog}
            models={models}
            runningModel={runningModel}
            options={options}
            onOptions={onOptions}
            limits={limits}
            actions={{ ...actions, attach: () => picker.current?.click(), mention: () => startWith("@"), commands: () => startWith("/") }}
            onClose={() => {
              setPalette(false);
              box.current?.focus();
            }}
          />
        )}
        {menu && options_.length > 0 && (
          <ul className="absolute inset-x-0 bottom-full z-30 mb-1 max-h-64 overflow-y-auto rounded-md border border-panel-line bg-panel-raised p-1 shadow-lg" role="listbox">
            {options_.map((o, i) => (
              <li key={o.value}>
                <button
                  type="button"
                  role="option"
                  aria-selected={i === pick}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    choose(o.value);
                  }}
                  onMouseEnter={() => setPick(i)}
                  className={cn("flex w-full items-center gap-2 rounded px-2 py-1 text-left font-mono text-[12px]", i === pick ? "bg-panel-line text-white" : "text-panel-foreground")}
                >
                  {menu.kind === "slash" ? <SlashIcon className="size-3 text-panel-muted" /> : <img src={fileIcon(o.value.split("/").pop() ?? o.value)} alt="" className="size-3.5" />}
                  <span className="shrink-0">{o.value}</span>
                  {o.hint && <span className="min-w-0 truncate font-sans text-[11px] text-panel-muted">{o.hint}</span>}
                </button>
              </li>
            ))}
          </ul>
        )}

        {files.length > 0 && (
          <div className="flex flex-wrap gap-1.5 px-2.5 pt-2">
            {files.map((f) => (
              <span key={f.id} className="flex h-7 max-w-48 items-center gap-1.5 rounded border border-panel-line bg-panel pl-1 pr-0.5 text-[11px] text-panel-foreground">
                {f.preview ? <img src={f.preview} alt="" className="size-5 rounded-sm object-cover" /> : <FileIcon className="size-3.5 text-panel-muted" />}
                <span className="truncate" title={f.file.name}>
                  {f.file.name}
                </span>
                <button type="button" onClick={() => removeFile(f.id)} aria-label={`Remove ${f.file.name}`} className="grid size-5 place-items-center rounded-sm hover:bg-panel-raised">
                  <XIcon className="size-3" />
                </button>
              </span>
            ))}
          </div>
        )}

        <textarea
          ref={box}
          value={text}
          rows={1}
          onChange={(e) => {
            setText(e.target.value);
            updateMenu(e.target.value, e.target.selectionStart);
          }}
          onClick={(e) => updateMenu(text, e.currentTarget.selectionStart)}
          onPaste={(e) => {
            if (!e.clipboardData.files.length) return;
            e.preventDefault();
            void addFiles(e.clipboardData.files);
          }}
          onKeyDown={(e) => {
            if (menu && options_.length) {
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setPick((p) => (p + (e.key === "ArrowDown" ? 1 : options_.length - 1)) % options_.length);
                return;
              }
              if (e.key === "Enter" || e.key === "Tab") {
                e.preventDefault();
                choose((options_[pick] ?? options_[0]!).value);
                return;
              }
            }
            if (e.key === "Escape" && menu) {
              setMenu(null);
              return;
            }
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={busy ? "Claude is working… (your message waits its turn)" : "Ask Claude Code… (@ file, / command, paste files)"}
          className="block max-h-[220px] min-h-10 w-full resize-none bg-transparent px-3 pb-1 pt-2.5 text-[13px] leading-5 text-white placeholder:text-panel-muted focus:outline-none"
        />

        {error && <p className="px-3 pb-1 text-[11px] text-led-fail">{error}</p>}

        <div className="flex items-center gap-0.5 px-1.5 pb-1.5">
          <input
            ref={picker}
            type="file"
            multiple
            hidden
            onChange={(e) => {
              if (e.target.files) void addFiles(e.target.files);
              e.target.value = "";
            }}
          />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button type="button" className={cn(chip, "px-1.5")} title="Attach" aria-label="Attach">
                <PlusIcon className="size-4" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" side="top">
              <DropdownMenuItem onSelect={() => picker.current?.click()}>
                <PaperclipIcon /> Upload files or images…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => startWith("@")}>
                <AtSignIcon /> Mention a project file
              </DropdownMenuItem>
              {activeFile && (
                <DropdownMenuItem onSelect={() => insert(`@${activeFile} `, text.length, text.length)}>
                  <FilePlusIcon /> Add {activeFile.split("/").pop()}
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
          <button
            type="button"
            className={cn(chip, "px-1.5", palette && "bg-panel-line text-white")}
            title="Actions: model, MCP servers, account, settings…"
            aria-label="Actions"
            aria-expanded={palette}
            onClick={() => setPalette((v) => !v)}
          >
            <SlashIcon className="size-3.5" />
          </button>
          <Usage limits={limits} />

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button type="button" className={cn(chip, "min-w-0 bg-panel-line/50")} title="Model and effort">
                <span className="truncate text-panel-foreground">{model}</span>
                {effort && <span className="shrink-0">{effort}</span>}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" side="top" className="max-h-[70vh] w-72 overflow-y-auto">
              <p className="px-2 py-1 text-[11px] text-muted-foreground">Model</p>
              {models.map((m) => {
                const value = m.value === "default" ? null : m.value;
                return (
                  <DropdownMenuItem key={m.value} onSelect={() => onOptions({ ...options, model: value })} className="items-start">
                    <CheckIcon className={cn("mt-0.5", options.model !== value && "opacity-0")} />
                    <span className="grid min-w-0">
                      <span>{m.displayName}</span>
                      {m.description && <span className="text-[11px] text-muted-foreground">{m.description}</span>}
                    </span>
                  </DropdownMenuItem>
                );
              })}
              <DropdownMenuSeparator />
              <p className="px-2 py-1 text-[11px] text-muted-foreground">Effort</p>
              {fixedEffort && <p className="px-2 pb-1 text-[11px] text-muted-foreground">This model has no effort setting.</p>}
              {efforts.map((e) => (
                <DropdownMenuItem key={e.label} onSelect={() => onOptions({ ...options, effort: e.value })}>
                  <CheckIcon className={cn(options.effort !== e.value && "opacity-0")} />
                  {e.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>

          <span className="flex-1" />

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button type="button" className={cn(chip, mode === "bypassPermissions" && "text-led-fail")} title={`Permission mode: ${modeInfo.hint}`}>
                <ModeIcon className="size-3.5" />
                <span className="hidden sm:inline">{modeLabel}</span>
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" side="top" className="min-w-64">
              {MODES.map((m) => {
                const Icon = (m.value && MODE_ICONS[m.value]) || Settings2Icon;
                return (
                  <DropdownMenuItem key={m.label} onSelect={() => onOptions({ ...options, mode: m.value as ChatMode | null })} className="items-start">
                    <Icon className="mt-0.5" />
                    <span className="grid">
                      <span className={cn(options.mode === m.value && "text-led")}>{m.label}</span>
                      <span className="text-[11px] text-muted-foreground">{m.hint}</span>
                    </span>
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuContent>
          </DropdownMenu>

          {busy && !text.trim() && !files.length ? (
            <button type="button" onClick={onStop} title="Stop" aria-label="Stop" className="ml-1 grid size-7 shrink-0 place-items-center rounded-md bg-panel-line text-white hover:bg-panel-muted/40">
              <SquareIcon className="size-3 fill-current" />
            </button>
          ) : (
            <button
              type="button"
              onClick={submit}
              disabled={!text.trim() && !files.length}
              title="Send (Enter)"
              aria-label="Send"
              className="ml-1 grid size-7 shrink-0 place-items-center rounded-md bg-led text-black hover:bg-led/90 disabled:opacity-40"
            >
              <ArrowUpIcon className="size-4" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Usage of the 5-hour limit and when it resets; the weekly one on hover. Ticks once a minute. */
function Usage({ limits }: { limits: { fiveHour?: Limit; sevenDay?: Limit } }) {
  const [, tick] = useState(0);
  const five = limits.fiveHour;
  useEffect(() => {
    if (!five) return;
    const t = setInterval(() => tick((n) => n + 1), 60_000);
    return () => clearInterval(t);
  }, [five]);
  if (!five) return null;
  const pct = (w: Limit) => `${Math.round(w.utilization * 100)}%`;
  const at = (w: Limit) => new Date(w.resetsAt * 1000).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" });
  const week = limits.sevenDay;
  const title = `5-hour limit: ${pct(five)} used, resets ${at(five)}${week ? `\nWeekly limit: ${pct(week)} used, resets ${at(week)}` : ""}`;
  const high = five.utilization >= 0.75 || (week?.utilization ?? 0) >= 0.9;
  return (
    <span className={cn(chip, "cursor-default hover:bg-transparent", high && "text-amber-400 hover:text-amber-400")} title={title}>
      <ClockIcon className="size-3.5" />
      {untilText(five.resetsAt)}
    </span>
  );
}
