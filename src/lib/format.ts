import type { Category, ProcessState, WindowsSupport } from "./api/types";

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"] as const;
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

/** "3m ago" style relative time; falls back to the raw string if unparsable. */
export function formatRelative(iso: string, now: Date = new Date()): string {
  const then = new Date(iso);
  const seconds = Math.round((now.getTime() - then.getTime()) / 1000);
  if (Number.isNaN(seconds)) return iso;
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return then.toLocaleString();
}

export function categoryLabel(category: Category): string {
  switch (category) {
    case "web_server":
      return "Web server";
    case "language":
      return "Language / runtime";
    case "database":
      return "Database";
    case "cache":
      return "Cache & queue";
    case "tool":
      return "Tool";
    default: {
      const unreachable: never = category;
      return unreachable;
    }
  }
}

export function windowsSupportLabel(support: WindowsSupport): string {
  switch (support) {
    case "native":
      return "Native Windows build";
    case "native_with_caveats":
      return "Native, with caveats";
    case "no_official_build":
      return "No official Windows build";
    default: {
      const unreachable: never = support;
      return unreachable;
    }
  }
}

export function processSummary(process: ProcessState): string {
  switch (process.state) {
    case "running":
      return process.pids.length > 1
        ? `PID ${process.pid} (+${process.pids.length - 1} child ${process.pids.length === 2 ? "process" : "processes"})`
        : `PID ${process.pid}`;
    case "exited":
      return process.exit.code === null ? "exited" : `exited with code ${process.exit.code}`;
    case "notStarted":
      return "not started";
    default: {
      const unreachable: never = process;
      return String(unreachable);
    }
  }
}
