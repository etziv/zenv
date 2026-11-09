import { useQueryClient } from "@tanstack/react-query";
import { LoaderCircleIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { keys, useCatalog, useInstallJobs } from "@/lib/api/queries";
import type { ComponentId, InstallJob } from "@/lib/api/types";
import { formatBytes } from "@/lib/format";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";

export const isActive = (j: InstallJob) => j.phase !== "done" && j.phase !== "failed";

/** What the job is doing, in words that fit the component. */
export function phaseLabel(job: InstallJob): string {
  switch (job.phase) {
    case "downloading":
      return "Downloading";
    case "verifying":
      return "Verifying the download";
    case "extracting":
      if (job.component === "omniroute") return "npm is installing packages (several minutes)";
      if (job.component === "ninerouter") return "npm is installing packages";
      if (job.component === "rust") return "rustup is installing the toolchain (several minutes)";
      return "Unpacking";
    case "validating":
      return job.component === "mysql" ? "Checking Oracle's signature and the version" : "Checking the installed version";
    case "done":
      return "Installed";
    case "failed":
      return "Failed";
  }
}

function elapsed(since: string, now: number): string {
  const s = Math.max(0, Math.round((now - new Date(since).getTime()) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}

/** A clock that ticks while `on`, for elapsed times. */
function useNow(on: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [on]);
  return now;
}

const percent = (j: InstallJob) => (j.phase === "downloading" && j.total > 0 ? Math.min(100, Math.round((j.downloaded / j.total) * 100)) : null);

/** One install: component and version, phase, a bar and the time it has taken. */
export function InstallProgress({ job, name, className }: { job: InstallJob; name?: string; className?: string }) {
  const active = isActive(job);
  const now = useNow(active);
  const pct = percent(job);
  return (
    <div className={cn("grid gap-1.5 rounded-md border p-3 text-sm", className)}>
      <div className="flex items-center justify-between gap-3">
        <span className="flex min-w-0 items-center gap-2 font-medium">
          {active && <LoaderCircleIcon className="size-3.5 shrink-0 animate-spin text-primary" />}
          <span className="truncate">
            {name ?? job.component} {job.version}
          </span>
        </span>
        <span className={cn("shrink-0 text-xs", job.phase === "failed" ? "text-destructive" : job.phase === "done" ? "text-success" : "text-muted-foreground")}>
          {phaseLabel(job)}
          {pct !== null && ` · ${pct}%`}
          {active && ` · ${elapsed(job.startedAt, now)}`}
        </span>
      </div>
      {active && (
        <div className="h-1.5 overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100}>
          {pct !== null ? (
            <div className="h-full bg-primary transition-[width]" style={{ width: `${Math.max(pct, 2)}%` }} />
          ) : (
            <div className="install-indeterminate h-full w-1/3 bg-primary/70" />
          )}
        </div>
      )}
      {job.phase === "downloading" && (
        <span className="font-mono text-[11px] text-muted-foreground">
          {formatBytes(job.downloaded)}
          {job.total > 0 && ` of ${formatBytes(job.total)}`}
        </span>
      )}
      {job.error && <p className="text-xs text-destructive">{job.error}</p>}
    </div>
  );
}

/** Installs of one component that are running, or ended in the last minutes. */
export function ComponentInstalls({ component, name }: { component: ComponentId; name: string }) {
  const jobs = useInstallJobs();
  const now = useNow(true);
  const recent = (jobs.data ?? []).filter(
    (j) => j.component === component && (isActive(j) || (j.phase === "failed" && now - new Date(j.startedAt).getTime() < 30 * 60_000)),
  );
  if (recent.length === 0) return null;
  return (
    <div className="grid gap-2">
      {recent.map((j) => (
        <InstallProgress key={j.id} job={j} name={name} />
      ))}
    </div>
  );
}

/**
 * App-wide: when an install ends (whether or not its dialog is open), tell
 * the user and refresh the lists that show installed versions.
 */
export function useInstallNotices() {
  const jobs = useInstallJobs();
  const catalog = useCatalog();
  const qc = useQueryClient();
  const seen = useRef<Map<string, InstallJob["phase"]> | null>(null);
  useEffect(() => {
    const list = jobs.data;
    if (!list) return;
    const names = new Map((catalog.data ?? []).map((c) => [c.id, c.name]));
    if (seen.current === null) {
      // Jobs that ended before this window opened are not news.
      seen.current = new Map(list.map((j) => [j.id, j.phase]));
      return;
    }
    for (const j of list) {
      const before = seen.current.get(j.id);
      if (before !== j.phase && !isActive(j)) {
        const name = `${names.get(j.component) ?? j.component} ${j.version}`;
        if (j.phase === "done") {
          toast({ variant: "success", title: `${name} is installed` });
        } else {
          toast({ variant: "error", title: `${name} was not installed`, description: j.error ?? undefined });
        }
        void qc.invalidateQueries({ queryKey: keys.installations });
        void qc.invalidateQueries({ queryKey: keys.available(j.component) });
      }
      seen.current.set(j.id, j.phase);
    }
  }, [jobs.data, catalog.data, qc]);
}

/** Header: what is installing right now; opens the component's page. */
export function InstallIndicator({ onOpen }: { onOpen: (component: ComponentId) => void }) {
  const jobs = useInstallJobs();
  const catalog = useCatalog();
  const active = (jobs.data ?? []).filter(isActive);
  const first = active[0];
  if (!first) return null;
  const name = catalog.data?.find((c) => c.id === first.component)?.name ?? first.component;
  const pct = percent(first);
  const label =
    active.length === 1
      ? `Installing ${name} ${first.version}${pct !== null ? ` · ${pct}%` : ""}`
      : `Installing ${active.length} versions`;
  return (
    <button
      type="button"
      onClick={() => onOpen(first.component)}
      title={active.map((j) => `${catalog.data?.find((c) => c.id === j.component)?.name ?? j.component} ${j.version}: ${phaseLabel(j)}`).join("\n")}
      className="flex h-7 max-w-[18rem] items-center gap-1.5 rounded-sm border border-panel-line bg-panel-raised px-2 font-mono text-[11px] text-panel-foreground transition-colors hover:border-panel-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led"
    >
      <LoaderCircleIcon className="size-3.5 shrink-0 animate-spin text-led" />
      <span className="truncate">{label}</span>
    </button>
  );
}
