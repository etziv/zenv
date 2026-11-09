import { FileCodeIcon, LoaderIcon, LockIcon, WandSparklesIcon, XIcon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";

import { api } from "@/lib/api/client";

import { cn } from "@/lib/utils";

import { CodeEditor, type EditorStatus } from "./CodeEditor";
import { fileIcon } from "./fileIcons";
import { FORMAT_EVENT, formatOnSave, setFormatOnSave, subscribeFormatOnSave } from "./format";

export interface OpenFile {
  path: string;
  status?: EditorStatus;
}

function nameOf(path: string) {
  return path.split("/").pop() ?? path;
}

/** Open files as tabs, the editor of the one in front, and a status line. */
export function EditorArea({
  siteId,
  files,
  active,
  onActivate,
  onClose,
  onStatus,
  onDismiss,
}: {
  siteId: string;
  files: OpenFile[];
  active: string | null;
  onActivate: (path: string) => void;
  onClose: (path: string) => void;
  onStatus: (path: string, s: EditorStatus) => void;
  /** Closes the empty editor so the panels beside it take its room. */
  onDismiss?: () => void;
}) {
  const current = files.find((f) => f.path === active);
  const onSave = useSyncExternalStore(subscribeFormatOnSave, formatOnSave);
  const status = current?.status;
  const [installing, setInstalling] = useState<string | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const install = (tool: string, path: string) => {
    setInstalling(tool);
    setInstallError(null);
    api.formatterInstall(siteId, tool).then(
      () => {
        setInstalling(null);
        window.dispatchEvent(new CustomEvent(FORMAT_EVENT, { detail: path }));
      },
      (e: Error) => {
        setInstalling(null);
        setInstallError(e.message);
      },
    );
  };

  return (
    <section className="flex h-full min-h-0 min-w-0 flex-col" aria-label="Editor">
      <div role="tablist" className="flex h-9 shrink-0 items-stretch overflow-x-auto border-b border-panel-line [scrollbar-width:none]">
        {files.map((f) => {
          const isActive = f.path === active;
          const dirty = f.status?.dirty;
          return (
            <div
              key={f.path}
              title={f.path}
              onAuxClick={(e) => e.button === 1 && onClose(f.path)}
              className={cn(
                "group relative flex shrink-0 items-center gap-1.5 border-r border-panel-line pl-3 pr-1 text-[12px] transition-colors",
                isActive ? "bg-panel-raised/70 text-white" : "text-panel-muted hover:text-panel-foreground",
              )}
            >
              <button type="button" role="tab" aria-selected={isActive} onClick={() => onActivate(f.path)} className="flex items-center gap-1.5 focus-visible:outline-none">
                {f.status?.readOnly ? <LockIcon className="size-3 text-panel-muted" /> : <img src={fileIcon(nameOf(f.path))} alt="" draggable={false} className="size-3.5" />}
                {nameOf(f.path)}
              </button>
              <button
                type="button"
                aria-label={`Close ${nameOf(f.path)}`}
                title={dirty ? "Unsaved changes (Ctrl+S saves). Close anyway" : "Close"}
                onClick={() => {
                  if (dirty && !window.confirm(`${nameOf(f.path)} has unsaved changes. Close it anyway?`)) return;
                  onClose(f.path);
                }}
                className="relative grid size-5 place-items-center rounded-sm hover:bg-panel-raised [&_svg]:size-3"
              >
                {dirty && <span className="absolute size-2 rounded-full bg-panel-foreground group-hover:hidden" />}
                <XIcon className={cn(dirty ? "hidden group-hover:block" : isActive ? "opacity-100" : "opacity-0 group-hover:opacity-100")} />
              </button>
              {isActive && <span className="absolute inset-x-0 top-0 h-[2px] bg-led" />}
            </div>
          );
        })}
        {files.length === 0 && onDismiss && (
          <div className="relative flex shrink-0 items-center gap-1.5 border-r border-panel-line bg-panel-raised/70 pl-3 pr-1 text-[12px] text-panel-muted">
            <span>Empty editor</span>
            <button
              type="button"
              aria-label="Close editor"
              title="Close editor (Claude Code takes the full width until a file is opened)"
              onClick={onDismiss}
              className="grid size-5 place-items-center rounded-sm hover:bg-panel-raised hover:text-panel-foreground [&_svg]:size-3"
            >
              <XIcon />
            </button>
            <span className="absolute inset-x-0 top-0 h-[2px] bg-led" />
          </div>
        )}
      </div>

      <div className="relative min-h-0 flex-1">
        {files.map((f) => (
          <div key={f.path} className={cn("absolute inset-0", f.path !== active && "invisible")}>
            <CodeEditor siteId={siteId} path={f.path} active={f.path === active} onStatus={(s) => onStatus(f.path, s)} />
          </div>
        ))}
        {files.length === 0 && (
          <div className="grid h-full place-items-center p-6 text-center">
            <div className="grid justify-items-center gap-3 text-panel-muted">
              <FileCodeIcon className="size-10 opacity-40" />
              <p className="text-sm">Pick a file on the left to edit it.</p>
              <p className="font-mono text-[11px]">
                <Kbd>Ctrl</Kbd> <Kbd>P</Kbd> go to file · <Kbd>Ctrl</Kbd> <Kbd>S</Kbd> save · <Kbd>Ctrl</Kbd> <Kbd>F</Kbd> find
              </p>
            </div>
          </div>
        )}
      </div>

      <div className="flex h-6 shrink-0 items-center gap-4 border-t border-panel-line px-3 font-mono text-[10.5px] text-panel-muted">
        {current ? (
          <>
            <span className="min-w-0 truncate">{current.path}</span>
            {status?.formatMissing && status.formatMissing !== "gofmt" && (
              <button
                type="button"
                disabled={installing !== null}
                onClick={() => install(status.formatMissing ?? "", current.path)}
                title={status.formatMissing === "rustfmt" ? "rustup component add rustfmt" : "The official ruff release from GitHub, into the Zenv home"}
                className="flex shrink-0 items-center gap-1 rounded bg-led/15 px-1.5 text-led hover:bg-led/25 disabled:opacity-60"
              >
                {installing ? <LoaderIcon className="size-3 animate-spin" /> : null}
                {installing ? `Installing ${installing}…` : `Install ${status.formatMissing}`}
              </button>
            )}
            {status?.formatMissing === "gofmt" && <span className="shrink-0">Install Go in Zenv → Languages</span>}
            {installError && (
              <span className="min-w-0 truncate text-led-fail" title={installError}>
                {installError}
              </span>
            )}
            {status?.formatError && !status.formatMissing && (
              <span className="min-w-0 truncate text-led-fail" title={status.formatError}>
                {status.formatError}
              </span>
            )}
            <span className="ml-auto shrink-0">{current.status?.cursor}</span>
            <span className="shrink-0">{current.status?.language ?? "Plain text"}</span>
            {status?.formatter && !status.readOnly && (
              <>
                <button
                  type="button"
                  title={`Format with ${status.formatter} (Shift+Alt+F)`}
                  onClick={() => window.dispatchEvent(new CustomEvent(FORMAT_EVENT, { detail: current.path }))}
                  className="flex shrink-0 items-center gap-1 rounded px-1 hover:bg-panel-raised hover:text-panel-foreground"
                >
                  {status.formatting ? <LoaderIcon className="size-3 animate-spin" /> : <WandSparklesIcon className="size-3" />}
                  {status.formatter}
                </button>
                <label className="flex shrink-0 cursor-pointer items-center gap-1 hover:text-panel-foreground" title="Format the file each time it is saved">
                  <input type="checkbox" checked={onSave} onChange={(e) => setFormatOnSave(e.target.checked)} className="size-3 accent-[#3ee8a0]" />
                  format on save
                </label>
              </>
            )}
            {current.status?.readOnly ? (
              <span className="shrink-0">read-only</span>
            ) : (
              <span className={cn("shrink-0", current.status?.dirty && "text-led")}>{current.status?.dirty ? "unsaved" : "saved"}</span>
            )}
          </>
        ) : (
          <span>No file open</span>
        )}
      </div>
    </section>
  );
}

function Kbd({ children }: { children: string }) {
  return <kbd className="rounded border border-panel-line bg-panel-raised px-1 py-px text-panel-foreground">{children}</kbd>;
}
