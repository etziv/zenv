import { CircleAlertIcon } from "lucide-react";

import { ApiError } from "@/lib/api/client";
import { cn } from "@/lib/utils";

function DetailBlock({ title, lines }: { title: string; lines: string[] }) {
  if (lines.length === 0) return null;
  return (
    <div className="mt-2">
      <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{title}</p>
      <pre className="mt-1 max-h-56 overflow-auto rounded bg-background/70 p-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap">
        {lines.join("\n")}
      </pre>
    </div>
  );
}

/** Renders a backend error with whatever diagnostics it carries. */
export function ErrorPanel({ error, className }: { error: unknown; className?: string }) {
  if (!error) return null;
  const err = error instanceof ApiError ? error : new ApiError("ipc", error instanceof Error ? error.message : String(error));
  const d = err.details;

  return (
    <div role="alert" className={cn("rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm", className)}>
      <div className="flex gap-2">
        <CircleAlertIcon className="mt-0.5 size-4 shrink-0 text-destructive" />
        <div className="min-w-0 flex-1">
          <p className="font-medium break-words">{err.message}</p>
          {d && "owners" in d && d.owners.length > 0 && (
            <ul className="mt-1 text-xs text-muted-foreground">
              {d.owners.map((o) => (
                <li key={o.pid} className="font-mono">
                  {o.processName ?? "unknown process"} · PID {o.pid} · listening on {o.address}
                </li>
              ))}
            </ul>
          )}
          {d && "output" in d && <DetailBlock title="Checker output" lines={d.output.split(/\r?\n/)} />}
          {d && "logTail" in d && <DetailBlock title="Recent log lines" lines={d.logTail} />}
        </div>
      </div>
    </div>
  );
}
