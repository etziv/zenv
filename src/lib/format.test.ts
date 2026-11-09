import { describe, expect, it } from "vitest";

import { formatBytes, formatRelative, processSummary } from "./format";

describe("formatBytes", () => {
  it("formats across units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(10 * 1024 * 1024)).toBe("10 MB");
    expect(formatBytes(-1)).toBe("—");
  });
});

describe("formatRelative", () => {
  const now = new Date("2026-10-16T12:00:00Z");
  it("uses short relative units", () => {
    expect(formatRelative("2026-10-16T11:59:58Z", now)).toBe("just now");
    expect(formatRelative("2026-10-16T11:59:30Z", now)).toBe("30s ago");
    expect(formatRelative("2026-10-16T11:45:00Z", now)).toBe("15m ago");
    expect(formatRelative("2026-10-16T09:00:00Z", now)).toBe("3h ago");
  });
  it("returns unparsable input unchanged", () => {
    expect(formatRelative("garbage", now)).toBe("garbage");
  });
});

describe("processSummary", () => {
  it("describes each process state", () => {
    expect(processSummary({ state: "running", pid: 10, pids: [10, 11], startedAt: "" })).toBe("PID 10 (+1 child process)");
    expect(processSummary({ state: "running", pid: 10, pids: [10, 11, 12], startedAt: "" })).toBe("PID 10 (+2 child processes)");
    expect(processSummary({ state: "exited", exit: { code: 1, at: "" } })).toBe("exited with code 1");
    expect(processSummary({ state: "notStarted" })).toBe("not started");
  });
});
