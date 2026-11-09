import { useEffect, useMemo, useRef } from "react";

import type { LogTail } from "@/lib/api/types";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";

export type LineLevel = "error" | "warning" | "plain";

const ERROR = /\b(fatal|error|err|crit|critical|emerg|alert|panic|exception|uncaught)\b/i;
const WARNING = /\b(warn|warning|deprecated|notice)\b/i;
/** Access logs: `"GET / HTTP/1.1" 500 …`. */
const STATUS = /" ([1-5])\d\d /;

/** How a log line should be highlighted. */
export function lineLevel(line: string): LineLevel {
  const status = STATUS.exec(line)?.[1];
  if (status === "5") return "error";
  if (status === "4") return "warning";
  if (status) return "plain";
  if (ERROR.test(line)) return "error";
  if (WARNING.test(line)) return "warning";
  return "plain";
}

/** Fixed-height log tail that stays pinned to the bottom while following. */
export function LogView({
  tail,
  follow,
  filter = "",
  className,
  emptyHint = "No log written yet. It appears after the first start.",
}: {
  tail: LogTail;
  follow: boolean;
  filter?: string;
  className?: string;
  emptyHint?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const needle = filter.trim().toLowerCase();
  const lines = useMemo(
    () => (needle ? tail.lines.filter((l) => l.toLowerCase().includes(needle)) : tail.lines),
    [tail.lines, needle],
  );
  useEffect(() => {
    if (follow && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines, follow]);

  if (!tail.exists) {
    return <p className="py-3 text-xs text-muted-foreground">{emptyHint}</p>;
  }
  return (
    <div className="flex min-h-0 flex-col">
      <div
        ref={ref}
        role="log"
        aria-live="off"
        className={cn(
          "h-72 overflow-auto rounded-md border bg-muted/40 py-1.5 font-mono text-[11.5px] leading-relaxed",
          className,
        )}
      >
        {lines.length === 0 ? (
          <p className="px-2.5 text-muted-foreground">{needle ? "No lines match the filter." : "(empty)"}</p>
        ) : (
          lines.map((line, i) => {
            const level = lineLevel(line);
            return (
              <div
                // Lines have no identity of their own; position is stable between refreshes.
                // eslint-disable-next-line react/no-array-index-key
                key={i}
                className={cn(
                  "border-l-2 px-2.5 whitespace-pre-wrap break-all",
                  level === "error" && "border-destructive bg-destructive/[0.06] text-destructive",
                  level === "warning" && "border-warning bg-warning/[0.06]",
                  level === "plain" && "border-transparent",
                )}
              >
                {line || " "}
              </div>
            );
          })
        )}
      </div>
      <p className="mt-1 truncate font-mono text-[10.5px] text-muted-foreground" title={tail.path}>
        {tail.path} · {formatBytes(tail.sizeBytes)}
        {tail.truncated && ` · last ${tail.lines.length} lines`}
        {needle && ` · ${lines.length} matching`}
      </p>
    </div>
  );
}
