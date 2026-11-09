import { useEffect, useState } from "react";

import { api } from "@/lib/api/client";
import type { ProjectEntry } from "@/lib/api/types";
import { cn } from "@/lib/utils";
import { fileIcon } from "./fileIcons";

/** Ctrl+P: find a file by name. */
export function QuickOpen({ siteId, onOpen, onClose }: { siteId: string; onOpen: (path: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<ProjectEntry[]>([]);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    let stale = false;
    const id = window.setTimeout(() => {
      api.projectSearch(siteId, q).then(
        (r) => {
          if (stale) return;
          setHits(r);
          setIndex(0);
        },
        () => {},
      );
    }, 120);
    return () => {
      stale = true;
      window.clearTimeout(id);
    };
  }, [query, siteId]);

  const pick = (h: ProjectEntry | undefined) => {
    if (!h) return;
    onOpen(h.path);
    onClose();
  };

  return (
    <div className="absolute inset-0 z-20 flex justify-center bg-black/40 pt-16" onMouseDown={onClose}>
      <div
        className="h-fit w-[min(560px,90%)] overflow-hidden rounded-lg border border-panel-line bg-panel-raised shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          placeholder="Go to file… (name or part of its path)"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setIndex((i) => Math.min(i + 1, hits.length - 1));
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setIndex((i) => Math.max(i - 1, 0));
            }
            if (e.key === "Enter") pick(hits[index]);
          }}
          className="h-10 w-full border-b border-panel-line bg-transparent px-3 font-mono text-[13px] text-white outline-none placeholder:text-panel-muted"
        />
        <ul className="max-h-80 overflow-auto py-1">
          {hits.map((h, i) => (
            <li key={h.path}>
              <button
                type="button"
                onMouseEnter={() => setIndex(i)}
                onClick={() => pick(h)}
                className={cn(
                  "flex w-full min-w-0 items-center gap-2 px-3 py-1 text-left text-[12.5px]",
                  i === index ? "bg-led/15 text-white" : "text-panel-foreground",
                )}
              >
                <img src={fileIcon(h.name)} alt="" draggable={false} className="size-4 shrink-0" />
                <span className="shrink-0">{h.name}</span>
                <span className="min-w-0 truncate font-mono text-[11px] text-panel-muted">{h.path}</span>
              </button>
            </li>
          ))}
          {query.trim() && hits.length === 0 && <li className="px-3 py-2 text-[12px] text-panel-muted">No matching file.</li>}
        </ul>
      </div>
    </div>
  );
}
