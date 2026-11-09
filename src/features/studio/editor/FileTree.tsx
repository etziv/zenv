import {
  ChevronRightIcon,
  FilePlusIcon,
  FolderPlusIcon,
  RefreshCwIcon,
} from "lucide-react";
import type * as React from "react";
import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "@/lib/api/client";
import type { ProjectEntry } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { fileIcon, folderIcon } from "./fileIcons";

const iconButton =
  "grid size-6 place-items-center rounded text-panel-muted transition-colors hover:bg-panel-raised hover:text-panel-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led [&_svg]:size-3.5";

/** Where a new file or folder is being named. */
interface Draft {
  parent: string;
  kind: "file" | "folder";
}

function parentOf(path: string) {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

/** The app's project folder, opened one folder at a time. */
export function FileTree({
  siteId,
  rootName,
  activePath,
  onOpen,
}: {
  siteId: string;
  rootName: string;
  activePath: string | null;
  onOpen: (path: string) => void;
}) {
  const [children, setChildren] = useState<Record<string, ProjectEntry[] | undefined>>({});
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([""]));
  const [selectedDir, setSelectedDir] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;

  const load = useCallback(
    (dir: string) =>
      api.projectList(siteId, dir).then(
        (l) => setChildren((c) => ({ ...c, [dir]: l.entries })),
        (e: Error) => setError(e.message),
      ),
    [siteId],
  );

  const refresh = useCallback(() => {
    setError(null);
    for (const dir of expandedRef.current) void load(dir);
  }, [load]);

  useEffect(() => {
    void load("");
    // Files Claude Code adds show up when you come back to the window.
    const onFocus = () => refresh();
    window.addEventListener("focus", onFocus);
    const id = window.setInterval(() => document.visibilityState === "visible" && refresh(), 5000);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.clearInterval(id);
    };
  }, [load, refresh]);

  const toggle = (dir: string) => {
    setSelectedDir(dir);
    setExpanded((s) => {
      const next = new Set(s);
      if (next.has(dir)) next.delete(dir);
      else {
        next.add(dir);
        void load(dir);
      }
      return next;
    });
  };

  const create = (name: string) => {
    const d = draft;
    setDraft(null);
    const clean = name.trim().replace(/\\/g, "/").replace(/^\/+/, "");
    if (!d || !clean) return;
    const path = d.parent ? `${d.parent}/${clean}` : clean;
    api.projectCreate(siteId, path, d.kind).then(
      (entry) => {
        void load(parentOf(entry.path));
        if (d.kind === "file") onOpen(entry.path);
        else setExpanded((s) => new Set(s).add(entry.path));
      },
      (e: Error) => setError(e.message),
    );
  };

  const startDraft = (kind: Draft["kind"]) => {
    const parent = selectedDir;
    setExpanded((s) => new Set(s).add(parent));
    void load(parent);
    setDraft({ parent, kind });
  };

  const renderDir = (dir: string, depth: number): React.ReactNode => {
    const entries = children[dir];
    return (
      <>
        {draft?.parent === dir && <DraftRow depth={depth} kind={draft.kind} onDone={create} />}
        {entries?.map((e) =>
          e.dir ? (
            <li key={e.path}>
              <Row
                depth={depth}
                selected={selectedDir === e.path}
                onClick={() => toggle(e.path)}
                title={e.path}
              >
                <ChevronRightIcon className={cn("size-3 shrink-0 transition-transform", expanded.has(e.path) && "rotate-90")} />
                <TypeIcon src={folderIcon(e.name, expanded.has(e.path))} />
                <span className="truncate">{e.name}</span>
              </Row>
              {expanded.has(e.path) && <ul>{renderDir(e.path, depth + 1)}</ul>}
            </li>
          ) : (
            <li key={e.path}>
              <Row
                depth={depth}
                selected={activePath === e.path}
                onClick={() => {
                  setSelectedDir(parentOf(e.path));
                  onOpen(e.path);
                }}
                title={e.path}
              >
                <span className="w-3 shrink-0" />
                <TypeIcon src={fileIcon(e.name)} />
                <span className="truncate">{e.name}</span>
              </Row>
            </li>
          ),
        )}
        {entries === undefined && (
          <li className="py-0.5 text-[11px] text-panel-muted" style={{ paddingLeft: 12 + depth * 12 }}>
            …
          </li>
        )}
      </>
    );
  };

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-panel-line pl-3 pr-1">
        <button
          type="button"
          onClick={() => setSelectedDir("")}
          className="min-w-0 flex-1 truncate text-left font-mono text-[10px] font-medium uppercase tracking-[0.14em] text-panel-muted"
          title="Project folder"
        >
          {rootName}
        </button>
        <button type="button" className={iconButton} title="New file" aria-label="New file" onClick={() => startDraft("file")}>
          <FilePlusIcon />
        </button>
        <button type="button" className={iconButton} title="New folder" aria-label="New folder" onClick={() => startDraft("folder")}>
          <FolderPlusIcon />
        </button>
        <button type="button" className={iconButton} title="Refresh" aria-label="Refresh" onClick={refresh}>
          <RefreshCwIcon />
        </button>
      </div>
      {error && (
        <button type="button" onClick={() => setError(null)} className="shrink-0 px-3 py-1.5 text-left text-[11px] text-led-fail" title="Dismiss">
          {error}
        </button>
      )}
      <ul className="min-h-0 flex-1 overflow-auto py-1 text-[12.5px] [scrollbar-width:thin]" aria-label="Project files">
        {renderDir("", 0)}
      </ul>
    </div>
  );
}

/** A file type's icon; the WebView loads each one once and caches it. */
function TypeIcon({ src }: { src: string }) {
  return <img src={src} alt="" draggable={false} decoding="async" className="size-4 shrink-0" />;
}

function Row({
  depth,
  selected,
  onClick,
  title,
  children,
}: {
  depth: number;
  selected: boolean;
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      style={{ paddingLeft: 8 + depth * 12 }}
      className={cn(
        "flex h-6 w-full min-w-0 items-center gap-1.5 pr-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-led",
        selected ? "bg-panel-raised text-white" : "text-panel-foreground hover:bg-panel-raised/60",
      )}
    >
      {children}
    </button>
  );
}

function DraftRow({ depth, kind, onDone }: { depth: number; kind: Draft["kind"]; onDone: (name: string) => void }) {
  const [name, setName] = useState("");
  const done = useRef(false);
  const finish = (value: string) => {
    if (done.current) return;
    done.current = true;
    onDone(value);
  };
  return (
    <li className="flex h-6 items-center gap-1.5 pr-2" style={{ paddingLeft: 8 + depth * 12 }}>
      <span className="w-3 shrink-0" />
      <TypeIcon src={kind === "file" ? fileIcon(name) : folderIcon(name, false)} />
      <input
        autoFocus
        value={name}
        placeholder={kind === "file" ? "name.php" : "folder"}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(name);
          if (e.key === "Escape") finish("");
        }}
        onBlur={() => finish(name)}
        className="h-5 min-w-0 flex-1 rounded-sm border border-led/60 bg-panel-raised px-1 font-mono text-[11.5px] text-white outline-none"
      />
    </li>
  );
}
