import { describe, expect, it } from "vitest";

import type { ChatLine } from "@/lib/api/types";

import { relative } from "./ClaudeChat";
import { chatReducer, initialChat, type ChatState } from "./model";

const run = (lines: ChatLine[], state: ChatState = initialChat) => chatReducer(state, { type: "lines", lines });
const delta = (text: string): ChatLine => ({ type: "stream_event", event: { type: "content_block_delta", delta: { type: "text_delta", text } } });

describe("chat model", () => {
  it("streams text and settles it with the whole message", () => {
    let s = chatReducer(initialChat, { type: "sent", text: "hi", files: [] });
    s = run([{ type: "system", subtype: "init", session_id: "abc", model: "claude-opus-5-5" }], s);
    s = run([{ type: "stream_event", event: { type: "content_block_start", content_block: { type: "text" } } }, delta("Hel"), delta("lo")], s);
    expect(s.items.at(-1)).toMatchObject({ kind: "text", text: "Hello", streaming: true });
    s = run([{ type: "assistant", message: { content: [{ type: "text", text: "Hello!" }] } }], s);
    expect(s.items.at(-1)).toMatchObject({ kind: "text", text: "Hello!", streaming: false });
    expect(s.streaming).toBeNull();
    expect(s.sessionId).toBe("abc");
    expect(s.model).toBe("claude-opus-5-5");
    s = run([{ type: "result", subtype: "success", is_error: false, duration_ms: 3650 }], s);
    expect(s.busy).toBe(false);
    expect(s.items.at(-1)).toMatchObject({ kind: "result", error: false, seconds: 3.7 });
  });

  it("asks permission on the tool's card and fills in its result", () => {
    let s = chatReducer(initialChat, { type: "sent", text: "write", files: [] });
    s = run([{ type: "assistant", message: { content: [{ type: "tool_use", id: "t1", name: "Write", input: { file_path: "a.txt" } }] } }], s);
    s = run([{ type: "control_request", request_id: "r1", request: { subtype: "can_use_tool", tool_name: "Write", tool_use_id: "t1", input: {} } }], s);
    expect(s.items).toHaveLength(2);
    expect(s.items[1]).toMatchObject({ kind: "tool", permission: { requestId: "r1", state: "pending" } });
    s = chatReducer(s, { type: "answered", requestId: "r1", allow: true, always: false });
    expect(s.items[1]).toMatchObject({ permission: { state: "allowed" } });
    s = run([{ type: "user", message: { content: [{ type: "tool_result", tool_use_id: "t1", content: "ok", is_error: false }] } }], s);
    expect(s.items[1]).toMatchObject({ result: { text: "ok", error: false } });
  });

  it("keeps untouched items identical so they don't redraw", () => {
    const s = run([{ type: "assistant", message: { content: [{ type: "text", text: "one" }] } }]);
    const after = run([{ type: "assistant", message: { content: [{ type: "text", text: "two" }] } }], s);
    expect(after.items[0]).toBe(s.items[0]);
  });

  it("reports an unexpected exit with Claude Code's error output", () => {
    const s = run([{ type: "zenv_exit", code: 1, stderr: "not logged in" }], chatReducer(initialChat, { type: "sent", text: "x", files: [] }));
    expect(s.busy).toBe(false);
    expect(s.items.at(-1)).toMatchObject({ kind: "notice", error: true });
    expect((s.items.at(-1) as { text: string }).text).toContain("not logged in");
  });
});

describe("chat options from Claude Code", () => {
  it("keeps commands, mode and usage limits", () => {
    let s = run([{ type: "system", subtype: "init", session_id: "a", model: "m", permissionMode: "default", slash_commands: ["compact", 3, "init"] }]);
    expect(s.commands).toEqual(["compact", "init"]);
    expect(s.mode).toBe("default");
    s = run([{ type: "rate_limit_event", rate_limit_info: { unifiedWindows: { five_hour: { utilization: 0.2, resetsAt: 100 }, seven_day: { utilization: 0.84, resetsAt: 200 } } } }], s);
    expect(s.limits.fiveHour).toEqual({ utilization: 0.2, resetsAt: 100 });
    expect(s.limits.sevenDay?.utilization).toBe(0.84);
  });

  it("offers Claude Code's suggestion and takes its mode when always allowed", () => {
    let s = run([
      {
        type: "control_request",
        request_id: "r1",
        request: { subtype: "can_use_tool", tool_name: "Write", tool_use_id: "t1", input: {}, permission_suggestions: [{ type: "setMode", mode: "acceptEdits", destination: "session" }] },
      },
    ]);
    expect(s.items[0]).toMatchObject({ permission: { always: "Allow all edits this session", alwaysMode: "acceptEdits" } });
    s = chatReducer(s, { type: "answered", requestId: "r1", allow: true, always: true });
    expect(s.mode).toBe("acceptEdits");
    const rule = run([
      {
        type: "control_request",
        request_id: "r2",
        request: { subtype: "can_use_tool", tool_name: "Bash", input: {}, permission_suggestions: [{ type: "addRules", rules: [{ toolName: "Bash", ruleContent: "npm test:*" }] }] },
      },
    ]);
    expect(rule.items[0]).toMatchObject({ permission: { always: "Always allow Bash npm test:*" } });
  });
});

describe("history", () => {
  it("shows a saved session and continues it", () => {
    const s = chatReducer(chatReducer(initialChat, { type: "sent", text: "old chat", files: [] }), {
      type: "history",
      sessionId: "s1",
      lines: [
        { type: "zenv_truncated", count: 4 },
        { type: "zenv_prompt", text: "make a button", images: 1 },
        { type: "assistant", message: { content: [{ type: "text", text: "Done" }] } },
      ],
    });
    expect(s.sessionId).toBe("s1");
    expect(s.busy).toBe(false);
    expect(s.items.map((i) => i.kind)).toEqual(["notice", "user", "text"]);
    expect(s.items[1]).toMatchObject({ text: "make a button", files: ["image 1"] });
  });
});

describe("relative", () => {
  it("maps files inside the project and refuses others", () => {
    expect(relative("C:\\xampp\\htdocs\\app", "C:\\xampp\\htdocs\\app\\src\\a.php")).toBe("src/a.php");
    expect(relative("C:\\xampp\\htdocs\\app", "c:/XAMPP/htdocs/app/b.php")).toBe("b.php");
    expect(relative("C:\\xampp\\htdocs\\app", "D:\\other\\c.php")).toBeNull();
    expect(relative("C:\\xampp\\htdocs\\app", "src/d.php")).toBe("src/d.php");
  });
});
