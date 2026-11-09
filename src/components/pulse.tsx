import type { Health } from "@/lib/api/types";
import { cn } from "@/lib/utils";

const LABEL: Record<Health, string> = {
  running: "Running",
  stopped: "Stopped",
  failed: "Exited unexpectedly",
};

/** Status pulse: animates only while the process is actually running. */
export function Pulse({ health, className }: { health: Health; className?: string }) {
  return (
    <span className={cn("relative inline-flex size-2.5 shrink-0", className)} role="img" aria-label={LABEL[health]}>
      {health === "running" && <span className="status-pulse absolute inset-0 rounded-full bg-primary" />}
      <span
        className={cn(
          "relative inline-flex size-2.5 rounded-full",
          health === "running" && "bg-primary",
          health === "stopped" && "border border-muted-foreground/50 bg-transparent",
          health === "failed" && "bg-destructive",
        )}
      />
    </span>
  );
}

export function HealthLabel({ health }: { health: Health }) {
  return (
    <span className="inline-flex items-center gap-2 text-xs font-medium">
      <Pulse health={health} />
      <span className={cn(health === "failed" && "text-destructive")}>{LABEL[health]}</span>
    </span>
  );
}
