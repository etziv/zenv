import { ExternalLinkIcon, SearchIcon } from "lucide-react";
import type * as React from "react";
import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { LogView } from "@/components/log-view";
import { Loading } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api/client";
import { useLogTail } from "@/lib/api/queries";
import type { LogTail, RevealTarget } from "@/lib/api/types";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";

export interface PanelSource {
  id: string;
  label: string;
  exists: boolean;
  sizeBytes?: number;
}

/** The right-hand log viewer: source switcher, filter, follow, open file. */
export function LogPanel({
  title,
  subtitle,
  banner,
  sources,
  queryKey,
  read,
  reveal,
  emptyHint,
}: {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  banner?: React.ReactNode;
  sources: PanelSource[];
  queryKey: readonly unknown[];
  read: (sourceId: string) => Promise<LogTail>;
  reveal: (sourceId: string) => RevealTarget;
  emptyHint?: string;
}) {
  const firstWritten = sources.find((s) => s.exists && (s.sizeBytes ?? 1) > 0) ?? sources[0];
  const [picked, setPicked] = useState<string | null>(null);
  const active = sources.find((s) => s.id === picked) ?? firstWritten;
  const [filter, setFilter] = useState("");
  const [follow, setFollow] = useState(true);
  const [openError, setOpenError] = useState<unknown>(null);
  const tail = useLogTail([...queryKey, active?.id], () => read(active?.id ?? ""), Boolean(active), follow);

  return (
    <Card className="flex min-w-0 flex-col gap-3 p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="truncate font-display text-lg font-semibold leading-tight">{title}</h2>
          {subtitle && (
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground" title={typeof subtitle === "string" ? subtitle : undefined}>
              {subtitle}
            </p>
          )}
        </div>
        {active && (
          <Button
            size="sm"
            variant="outline"
            disabled={!active.exists}
            onClick={() => {
              setOpenError(null);
              api.reveal(reveal(active.id)).catch(setOpenError);
            }}
          >
            <ExternalLinkIcon /> Open file
          </Button>
        )}
      </div>

      {banner}

      {sources.length === 0 ? (
        <p className="text-sm text-muted-foreground">{emptyHint ?? "Nothing has been logged here yet."}</p>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <div role="tablist" aria-label="Log" className="flex flex-wrap gap-1 rounded-md border bg-muted/40 p-0.5">
              {sources.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  role="tab"
                  aria-selected={s.id === active?.id}
                  onClick={() => setPicked(s.id)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-sm px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                    s.id === active?.id ? "bg-card text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  {s.label}
                  {s.exists && s.sizeBytes !== undefined && (
                    <span className="font-mono text-[10px] text-muted-foreground">{formatBytes(s.sizeBytes)}</span>
                  )}
                </button>
              ))}
            </div>
            <div className="relative ml-auto">
              <SearchIcon className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter lines"
                aria-label="Filter lines"
                className="h-7 w-48 pl-7 text-xs"
              />
            </div>
            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} className="accent-primary" />
              Follow
            </label>
          </div>

          {tail.isPending ? (
            <Loading />
          ) : tail.error ? (
            <ErrorPanel error={tail.error} />
          ) : (
            <LogView
              tail={tail.data}
              follow={follow}
              filter={filter}
              className="h-[calc(100vh-25rem)] min-h-64"
              emptyHint="Nothing written to this log yet."
            />
          )}
        </>
      )}
      <ErrorPanel error={openError} />
    </Card>
  );
}

/** One selectable row in the left-hand list. */
export function ListItem({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "true" : undefined}
      className={cn(
        "flex w-full items-center gap-2 border-l-2 px-3 py-1.5 text-left text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring",
        active ? "border-primary bg-accent font-medium" : "border-transparent hover:bg-accent/60",
      )}
    >
      {children}
    </button>
  );
}

export function ListGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="py-1.5">
      <p className="px-3 pb-1 font-mono text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">{label}</p>
      {children}
    </div>
  );
}
