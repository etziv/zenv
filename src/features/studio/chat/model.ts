import type { ChatLine } from "@/lib/api/types";

/** What the chat shows, in order. Each change replaces only the item it touches. */
export type Item =
  | { kind: "user"; key: number; text: string; files: string[] }
  | { kind: "text"; key: number; text: string; streaming: boolean }
  | {
      kind: "tool";
      key: number;
      id: string;
      name: string;
      input: Record<string, unknown>;
      result?: { text: string; error: boolean };
      /** Claude Code asked whether it may run this. */
      permission?: Permission;
    }
  | { kind: "result"; key: number; error: boolean; text: string; seconds: number | null }
  | { kind: "notice"; key: number; text: string; error: boolean };

export interface Permission {
  requestId: string;
  state: "pending" | "allowed" | "denied";
  /** What "always" would allow, as Claude Code suggests it (a rule or a mode). */
  always?: string;
  /** The permission mode that "always" switches to. */
  alwaysMode?: string;
}

/** A usage limit window: share used (0..1) and when it resets (epoch seconds). */
export interface Limit {
  utilization: number;
  resetsAt: number;
}

type NewItem = Item extends infer T ? (T extends Item ? Omit<T, "key"> : never) : never;

export interface ChatState {
  items: Item[];
  /** Claude Code is working on a prompt. */
  busy: boolean;
  /** Claude Code's session, to resume it if its program ends. */
  sessionId: string | null;
  model: string | null;
  /** The permission mode Claude Code reports. */
  mode: string | null;
  /** Slash commands Claude Code offers here. */
  commands: string[] | null;
  limits: { fiveHour?: Limit; sevenDay?: Limit };
  /** The text item receiving streamed text. */
  streaming: number | null;
  next: number;
}

export const initialChat: ChatState = {
  items: [],
  busy: false,
  sessionId: null,
  model: null,
  mode: null,
  commands: null,
  limits: {},
  streaming: null,
  next: 1,
};

export type ChatAction =
  | { type: "lines"; lines: ChatLine[] }
  /** A saved session, shown and continued from where it ended. */
  | { type: "history"; sessionId: string; lines: ChatLine[] }
  | { type: "sent"; text: string; files: string[] }
  | { type: "answered"; requestId: string; allow: boolean; always: boolean }
  | { type: "notice"; text: string; error: boolean }
  | { type: "reset" };

type Block = { type?: string; text?: string; id?: string; name?: string; input?: unknown; tool_use_id?: string; content?: unknown; is_error?: boolean };

const obj = (v: unknown): Record<string, unknown> => (v && typeof v === "object" ? (v as Record<string, unknown>) : {});
const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const blocks = (message: unknown): Block[] => {
  const content = obj(message).content;
  return Array.isArray(content) ? (content as Block[]) : [];
};

/** Claude Code's first suggestion, as a button label and the mode it sets. */
function suggestion(raw: unknown): Pick<Permission, "always" | "alwaysMode"> {
  const first = obj(Array.isArray(raw) ? raw[0] : undefined);
  if (first.type === "setMode") {
    const mode = str(first.mode);
    return { always: mode === "acceptEdits" ? "Allow all edits this session" : `Switch to ${mode ?? "that"} mode`, alwaysMode: mode };
  }
  if (first.type === "addRules" && Array.isArray(first.rules)) {
    const rule = obj(first.rules[0]);
    const tool = str(rule.toolName) ?? "this";
    const content = str(rule.ruleContent);
    return { always: `Always allow ${tool}${content ? ` ${content}` : ""}` };
  }
  return {};
}

const limit = (v: unknown): Limit | undefined => {
  const w = obj(v);
  return typeof w.utilization === "number" && typeof w.resetsAt === "number" ? { utilization: w.utilization, resetsAt: w.resetsAt } : undefined;
};

/** Applies a batch to a copy of the items; untouched items keep their identity. */
function applyLines(state: ChatState, lines: ChatLine[]): ChatState {
  const items = state.items.slice();
  let { busy, sessionId, model, mode, commands, limits, streaming, next } = state;
  const push = (item: NewItem) => {
    items.push({ ...item, key: next++ } as Item);
    return items.length - 1;
  };
  const toolIndex = (id: string) => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it?.kind === "tool" && it.id === id) return i;
    }
    return -1;
  };
  const endStream = (text?: string) => {
    if (streaming === null) return;
    const it = items[streaming];
    if (it?.kind === "text") items[streaming] = { ...it, text: text ?? it.text, streaming: false };
    streaming = null;
  };

  for (const line of lines) {
    switch (line.type) {
      case "system":
        if (line.subtype === "init") {
          sessionId = str(line.session_id) ?? sessionId;
          model = str(line.model) ?? model;
          if (Array.isArray(line.slash_commands)) commands = line.slash_commands.filter((c): c is string => typeof c === "string");
        }
        mode = str(line.permissionMode) ?? mode;
        break;
      case "rate_limit_event": {
        const windows = obj(obj(line.rate_limit_info).unifiedWindows);
        limits = { fiveHour: limit(windows.five_hour) ?? limits.fiveHour, sevenDay: limit(windows.seven_day) ?? limits.sevenDay };
        break;
      }
      case "stream_event": {
        const event = obj(line.event);
        if (event.type === "content_block_start" && obj(event.content_block).type === "text") {
          endStream();
          streaming = push({ kind: "text", text: "", streaming: true });
        } else if (event.type === "content_block_delta" && streaming !== null) {
          const it = items[streaming];
          const delta = str(obj(event.delta).text) ?? "";
          if (it?.kind === "text" && delta) items[streaming] = { ...it, text: it.text + delta };
        }
        break;
      }
      case "assistant":
        for (const b of blocks(line.message)) {
          if (b.type === "text" && b.text) {
            if (streaming !== null) endStream(b.text);
            else push({ kind: "text", text: b.text, streaming: false });
          } else if (b.type === "tool_use" && b.id && toolIndex(b.id) < 0) {
            endStream();
            push({ kind: "tool", id: b.id, name: b.name ?? "Tool", input: obj(b.input) });
          }
        }
        break;
      case "user":
        for (const b of blocks(line.message)) {
          if (b.type !== "tool_result" || !b.tool_use_id) continue;
          const i = toolIndex(b.tool_use_id);
          const it = items[i];
          if (it?.kind === "tool") items[i] = { ...it, result: { text: str(b.content) ?? "", error: b.is_error === true } };
        }
        break;
      case "control_request": {
        const req = obj(line.request);
        const requestId = str(line.request_id);
        if (req.subtype !== "can_use_tool" || !requestId) break;
        const toolId = str(req.tool_use_id) ?? requestId;
        const permission: Permission = { requestId, state: "pending", ...suggestion(req.permission_suggestions) };
        const i = toolIndex(toolId);
        const it = items[i];
        if (it?.kind === "tool") items[i] = { ...it, permission };
        else push({ kind: "tool", id: toolId, name: str(req.tool_name) ?? "Tool", input: obj(req.input), permission });
        break;
      }
      case "control_cancel_request": {
        const i = items.findIndex((it) => it.kind === "tool" && it.permission?.requestId === line.request_id);
        const it = items[i];
        if (it?.kind === "tool") items[i] = { ...it, permission: undefined };
        break;
      }
      case "result": {
        endStream();
        busy = false;
        const error = line.is_error === true || line.subtype !== "success";
        const ms = typeof line.duration_ms === "number" ? line.duration_ms : null;
        const text = error ? (str(line.result) ?? String(line.subtype ?? "error")) : "";
        push({ kind: "result", error, text, seconds: ms === null ? null : Math.round(ms / 100) / 10 });
        // Whatever was still waiting for an answer is moot now.
        for (let i = 0; i < items.length; i++) {
          const it = items[i];
          if (it?.kind === "tool" && it.permission?.state === "pending") items[i] = { ...it, permission: undefined };
        }
        break;
      }
      case "zenv_prompt": {
        const images = typeof line.images === "number" ? line.images : 0;
        push({ kind: "user", text: str(line.text) ?? "", files: Array.from({ length: images }, (_, i) => `image ${i + 1}`) });
        break;
      }
      case "zenv_truncated":
        push({ kind: "notice", error: false, text: `${String(line.count)} earlier messages not shown.` });
        break;
      case "zenv_exit": {
        endStream();
        busy = false;
        const code = typeof line.code === "number" ? line.code : null;
        if (code !== 0) {
          const stderr = str(line.stderr);
          push({ kind: "notice", error: true, text: `Claude Code stopped${code === null ? "" : ` (exit code ${code})`}.${stderr ? `\n${stderr}` : ""}` });
        }
        break;
      }
    }
  }
  return { items, busy, sessionId, model, mode, commands, limits, streaming, next };
}

export function chatReducer(state: ChatState, action: ChatAction): ChatState {
  switch (action.type) {
    case "lines":
      return applyLines(state, action.lines);
    case "history":
      return applyLines({ ...initialChat, sessionId: action.sessionId }, action.lines);
    case "sent":
      return {
        ...state,
        busy: true,
        items: [...state.items, { kind: "user", key: state.next, text: action.text, files: action.files }],
        next: state.next + 1,
      };
    case "answered": {
      let mode = state.mode;
      const items = state.items.map((it) => {
        if (it.kind !== "tool" || it.permission?.requestId !== action.requestId) return it;
        if (action.allow && action.always && it.permission.alwaysMode) mode = it.permission.alwaysMode;
        return { ...it, permission: { requestId: action.requestId, state: action.allow ? ("allowed" as const) : ("denied" as const) } };
      });
      return { ...state, mode, items };
    }
    case "notice":
      return { ...state, busy: false, items: [...state.items, { kind: "notice", key: state.next, text: action.text, error: action.error }], next: state.next + 1 };
    case "reset":
      return initialChat;
  }
}
