import { useQuery } from "@tanstack/react-query";
import {
  ChevronRightIcon,
  ExternalLinkIcon,
  FolderOpenIcon,
  GlobeIcon,
  PanelLeftIcon,
  PanelRightIcon,
  SearchIcon,
} from "lucide-react";
import type * as React from "react";
import { useCallback, useEffect, useRef, useState } from "react";

import { EditorIcon, usePreferredEditor } from "@/features/apps/edit-buttons";
import { api } from "@/lib/api/client";
import { useOpenInEditor } from "@/lib/api/queries";
import type { AgentInfo, SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import type { EditorStatus } from "./editor/CodeEditor";
import { EditorArea, type OpenFile } from "./editor/EditorArea";
import { FileTree } from "./editor/FileTree";
import { QuickOpen } from "./editor/QuickOpen";
import { Preview } from "./Preview";
import { TerminalPanel } from "./TerminalPanel";
import { panelButton } from "./ui";

/** The app's Studio: its files and an editor, Claude Code to the right, then the live preview. */
export function StudioWindow({ siteId }: { siteId: string }) {
  const site = useQuery({ queryKey: ["site", siteId], queryFn: () => api.site(siteId) });
  const agents = useQuery({ queryKey: ["studio-agents", siteId], queryFn: () => api.studioAgents(siteId) });

  if (site.error) {
    return <p className="p-6 text-sm text-destructive">{(site.error as Error).message}</p>;
  }
  if (!site.data || !agents.data) {
    return <div className="h-full bg-panel" />;
  }
  return <Studio site={site.data} agents={agents.data} onRefreshAgents={() => void agents.refetch()} />;
}

type Drag = "tree" | "terminal" | "preview" | null;

/** Files width in pixels; terminal and preview widths as a share (%) of the body. */
interface Layout {
  tree: number;
  terminal: number;
  preview: number;
}

const LAYOUT_KEY = "zenv.studio.layout";
const DEFAULT_LAYOUT: Layout = { tree: 260, terminal: 36, preview: 32 };

function readLayout(): Layout {
  try {
    const saved = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}") as Partial<Layout>;
    const num = (v: unknown, fallback: number) => (typeof v === "number" && Number.isFinite(v) ? v : fallback);
    return {
      tree: num(saved.tree, DEFAULT_LAYOUT.tree),
      terminal: num(saved.terminal, DEFAULT_LAYOUT.terminal),
      preview: num(saved.preview, DEFAULT_LAYOUT.preview),
    };
  } catch {
    return DEFAULT_LAYOUT;
  }
}

function saveLayout(layout: Layout) {
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    // Storage unavailable: the widths last until the window closes.
  }
}

function Toggle({ on, onClick, label, children }: { on: boolean; onClick: () => void; label: string; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      title={label}
      aria-label={label}
      className={cn(panelButton, on && "text-led hover:text-led")}
    >
      {children}
    </button>
  );
}

function Studio({ site, agents, onRefreshAgents }: { site: SiteView; agents: AgentInfo[]; onRefreshAgents: () => void }) {
  const [files, setFiles] = useState<OpenFile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [showTree, setShowTree] = useState(true);
  const [showTerminal, setShowTerminal] = useState(true);
  const [showPreview, setShowPreview] = useState(false);
  // Closing the empty editor gives its room to Claude Code until a file is opened.
  const [editorOpen, setEditorOpen] = useState(true);
  const [quickOpen, setQuickOpen] = useState(false);
  const [initial] = useState(readLayout);
  const [treeWidth, setTreeWidth] = useState(initial.tree);
  const [terminalShare, setTerminalShare] = useState(initial.terminal);
  const [previewShare, setPreviewShare] = useState(initial.preview);
  const [drag, setDrag] = useState<Drag>(null);
  const body = useRef<HTMLDivElement>(null);
  const terminal = useRef<HTMLDivElement>(null);
  const { preferred: editor } = usePreferredEditor();
  const openInEditor = useOpenInEditor();
  const rootName = site.projectRoot.split(/[\\/]/).filter(Boolean).pop() ?? site.projectRoot;

  const open = useCallback((path: string) => {
    setFiles((f) => (f.some((x) => x.path === path) ? f : [...f, { path }]));
    setActive(path);
    setEditorOpen(true);
  }, []);
  // Never leave the body empty: with the terminal hidden the editor shows anyway.
  const showEditor = editorOpen || !showTerminal;
  const close = (path: string) => {
    const rest = files.filter((f) => f.path !== path);
    setFiles(rest);
    if (active === path) setActive(rest.at(-1)?.path ?? null);
  };
  const setStatus = useCallback(
    (path: string, status: EditorStatus) => setFiles((f) => f.map((x) => (x.path === path ? { ...x, status } : x))),
    [],
  );

  // Ctrl+P go to file, Ctrl+B files, Ctrl+` terminal (as in VS Code).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "p") setQuickOpen(true);
      else if (k === "b") setShowTree((v) => !v);
      else if (e.key === "`") setShowTerminal((v) => !v);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  useEffect(() => {
    if (!drag) return;
    const move = (e: PointerEvent) => {
      if (drag === "tree") {
        const box = body.current?.getBoundingClientRect();
        if (box) setTreeWidth(Math.min(520, Math.max(160, e.clientX - box.left)));
      } else if (drag === "terminal") {
        // Measured from the terminal's own right edge, which the preview may push in.
        const box = body.current?.getBoundingClientRect();
        const right = terminal.current?.getBoundingClientRect().right;
        if (box && right !== undefined) {
          const max = showPreview ? 80 - previewShare : 70;
          setTerminalShare(Math.min(max, Math.max(18, ((right - e.clientX) / box.width) * 100)));
        }
      } else {
        const box = body.current?.getBoundingClientRect();
        const max = showTerminal && showEditor ? 80 - terminalShare : 70;
        if (box) setPreviewShare(Math.min(max, Math.max(18, ((box.right - e.clientX) / box.width) * 100)));
      }
    };
    const up = () => setDrag(null);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
  }, [drag, showPreview, showTerminal, showEditor, previewShare, terminalShare]);

  useEffect(() => {
    if (!drag) saveLayout({ tree: treeWidth, terminal: terminalShare, preview: previewShare });
  }, [drag, treeWidth, terminalShare, previewShare]);

  const reset = (which: Exclude<Drag, null>) => {
    if (which === "tree") setTreeWidth(DEFAULT_LAYOUT.tree);
    else if (which === "terminal") setTerminalShare(DEFAULT_LAYOUT.terminal);
    else setPreviewShare(DEFAULT_LAYOUT.preview);
  };

  // A 1px line with a wider invisible grip around it; double-click restores the default width.
  const splitter = (which: Exclude<Drag, null>) => (
    <div className={cn("group relative w-px shrink-0 bg-panel-line", drag === which && "bg-led")}>
      <div
        role="separator"
        aria-orientation="vertical"
        title="Drag to resize · double-click to reset"
        onPointerDown={(e) => {
          e.preventDefault();
          setDrag(which);
        }}
        onDoubleClick={() => reset(which)}
        className="absolute inset-y-0 -left-[5px] -right-[5px] z-20 flex cursor-col-resize justify-center"
      >
        <span className={cn("w-[3px] bg-led/70 opacity-0 transition-opacity group-hover:opacity-100", drag === which && "opacity-100")} />
      </div>
    </div>
  );

  return (
    <div
      className={cn(
        "relative flex h-full flex-col bg-panel text-panel-foreground",
        drag && "select-none cursor-col-resize",
      )}
    >
      <header className="flex h-11 shrink-0 items-center gap-3 pl-4 pr-2">
        <span className="font-display text-[17px] font-semibold leading-none tracking-tight text-white">zenv</span>
        <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-led">studio</span>
        <span aria-hidden className="h-4 w-px bg-panel-line" />
        <div className="min-w-0">
          <p className="truncate text-[13px] font-medium leading-tight text-white">{site.domain}</p>
          <p className="truncate font-mono text-[10px] leading-tight text-panel-muted" title={site.projectRoot}>
            {site.projectRoot}
          </p>
        </div>
        <button
          type="button"
          onClick={() => setQuickOpen(true)}
          className="mx-auto hidden h-7 w-[min(360px,30vw)] items-center gap-2 rounded-md border border-panel-line bg-panel-raised px-2.5 text-[12px] text-panel-muted transition-colors hover:border-panel-muted md:flex"
        >
          <SearchIcon className="size-3.5" /> Go to file
          <span className="ml-auto font-mono text-[10px]">Ctrl P</span>
        </button>
        <div className="ml-auto flex shrink-0 items-center gap-1 md:ml-0">
          <Toggle on={showTree} onClick={() => setShowTree((v) => !v)} label="Files (Ctrl+B)">
            <PanelLeftIcon />
          </Toggle>
          <Toggle on={showTerminal} onClick={() => setShowTerminal((v) => !v)} label="Terminal: Claude Code (Ctrl+`)">
            <PanelRightIcon />
          </Toggle>
          <Toggle on={showPreview} onClick={() => setShowPreview((v) => !v)} label="Preview">
            <GlobeIcon />
          </Toggle>
          <span aria-hidden className="mx-1 h-4 w-px bg-panel-line" />
          {editor && (
            <button
              type="button"
              className={cn(panelButton, "w-auto gap-2 px-2.5 text-[12px]")}
              title={`Open ${site.projectRoot} in ${editor.name}`}
              onClick={() => openInEditor.mutate({ siteId: site.id, editor: editor.id })}
            >
              <EditorIcon editor={editor} /> {editor.name}
            </button>
          )}
          <button
            type="button"
            className={panelButton}
            title="Open project folder"
            aria-label="Open project folder"
            onClick={() => void api.reveal({ kind: "siteProjectRoot", id: site.id })}
          >
            <FolderOpenIcon />
          </button>
          <button
            type="button"
            className={panelButton}
            title="Open in browser"
            aria-label="Open in browser"
            disabled={!site.enabled}
            onClick={() => void api.openSiteUrl(site.id)}
          >
            <ExternalLinkIcon />
          </button>
        </div>
      </header>
      <div aria-hidden className="panel-trace h-px shrink-0" />

      <div ref={body} className="flex min-h-0 flex-1">
        {showTree && (
          <>
            <aside className="shrink-0" style={{ width: treeWidth }}>
              <FileTree siteId={site.id} rootName={rootName} activePath={active} onOpen={open} />
            </aside>
            {splitter("tree")}
          </>
        )}

        {showEditor && (
          <div className="min-w-0 flex-1">
            <EditorArea
              siteId={site.id}
              files={files}
              active={active}
              onActivate={setActive}
              onClose={close}
              onStatus={setStatus}
              onDismiss={showTerminal ? () => setEditorOpen(false) : undefined}
            />
          </div>
        )}

        {/* Hidden, not unmounted: Claude Code keeps running. */}
        {showTerminal && showEditor && splitter("terminal")}
        <div
          ref={terminal}
          className={cn("min-w-0", showEditor ? "shrink-0" : "flex-1", !showTerminal && "hidden")}
          style={showEditor ? { width: `${terminalShare}%` } : undefined}
        >
          <TerminalPanel
            siteId={site.id}
            projectRoot={site.projectRoot}
            activeFile={active}
            agents={agents}
            onRefreshAgents={onRefreshAgents}
            onOpenFile={open}
            actions={
              <button
                type="button"
                className={cn(panelButton, "size-7")}
                title="Hide terminal (Ctrl+`)"
                aria-label="Hide terminal"
                onClick={() => setShowTerminal(false)}
              >
                <ChevronRightIcon />
              </button>
            }
          />
        </div>

        {showPreview && (
          <>
            {splitter("preview")}
            <div className="flex min-w-0 shrink-0" style={{ width: `${previewShare}%` }}>
              <Preview site={site} blockPointer={drag !== null} />
            </div>
          </>
        )}
      </div>

      {/* While dragging, nothing underneath (terminal, preview frame) may take the pointer. */}
      {drag && <div aria-hidden className="fixed inset-0 z-50 cursor-col-resize" />}

      {quickOpen && <QuickOpen siteId={site.id} onOpen={open} onClose={() => setQuickOpen(false)} />}
    </div>
  );
}
