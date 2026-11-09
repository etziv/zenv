import type { ChatAttachment, ChatCatalog, ChatCommand, ChatEffort, ChatMode, ChatModel, ChatOptions } from "@/lib/api/types";

/** Until Claude Code lists its models (it names their versions), these aliases. */
export const FALLBACK_MODELS: ChatModel[] = [
  { value: "default", displayName: "Default", description: "Your account's default model" },
  { value: "opus", displayName: "Opus", description: "Latest Opus" },
  { value: "sonnet", displayName: "Sonnet", description: "Latest Sonnet" },
  { value: "haiku", displayName: "Haiku", description: "Latest Haiku" },
  { value: "fable", displayName: "Fable", description: "Latest Fable" },
];

export const EFFORTS: { value: ChatEffort | null; label: string }[] = [
  { value: null, label: "Default" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "xhigh", label: "Extra high" },
  { value: "max", label: "Max" },
];

export const MODES: { value: ChatMode | null; label: string; hint: string }[] = [
  { value: null, label: "Settings", hint: "As in your Claude Code settings" },
  { value: "manual", label: "Ask first", hint: "Asks before edits and commands" },
  { value: "acceptEdits", label: "Accept edits", hint: "Edits files without asking; asks for commands" },
  { value: "auto", label: "Auto", hint: "Decides on its own, asks only for risky steps" },
  { value: "plan", label: "Plan", hint: "Reads and plans; changes nothing until you approve" },
  { value: "dontAsk", label: "Don't ask", hint: "Never asks; refuses what isn't already allowed" },
  { value: "bypassPermissions", label: "Bypass", hint: "Runs everything without asking. Careful" },
];

/** Until Claude Code lists its commands, these. */
export const FALLBACK_COMMANDS: ChatCommand[] = ["compact", "context", "init", "review", "security-review", "usage"].map((name) => ({ name }));

const KEY = "zenv.studio.chat";
const CATALOG_KEY = "zenv.studio.chat.catalog";
/** Claude Code is asked for its models and commands at most once a day. */
const CATALOG_AGE = 24 * 3600 * 1000;

export const validModel = (m: unknown): m is string => typeof m === "string" && /^[A-Za-z0-9._[\]][A-Za-z0-9._[\]-]{0,79}$/.test(m);

export function readOptions(): ChatOptions {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<ChatOptions>;
    const bool = (b: unknown) => (typeof b === "boolean" ? b : null);
    return {
      thinking: bool(v.thinking),
      fast: bool(v.fast),
      switchOnFlag: bool(v.switchOnFlag),
      outputStyle: typeof v.outputStyle === "string" && /^[\p{L}\p{N} ._-]{1,60}$/u.test(v.outputStyle) ? v.outputStyle : null,
      model: validModel(v.model) ? v.model : null,
      effort: EFFORTS.some((e) => e.value === v.effort) ? (v.effort ?? null) : null,
      mode: MODES.some((m) => m.value === v.mode) ? (v.mode ?? null) : null,
    };
  } catch {
    return { ...NO_OPTIONS };
  }
}

export function saveOptions(o: ChatOptions) {
  try {
    localStorage.setItem(KEY, JSON.stringify(o));
  } catch {
    // Storage unavailable: the choice lasts until the window closes.
  }
}

/** The models and commands Claude Code listed last; `fresh` while under a day old. */
export function readCatalog(): { catalog: ChatCatalog | null; fresh: boolean } {
  try {
    const v = JSON.parse(localStorage.getItem(CATALOG_KEY) ?? "null") as { at?: number; catalog?: ChatCatalog } | null;
    const ok = v?.catalog && Array.isArray(v.catalog.models) && v.catalog.models.length > 0 && Array.isArray(v.catalog.commands);
    if (!ok || !v.catalog) return { catalog: null, fresh: false };
    return { catalog: v.catalog, fresh: Date.now() - (v.at ?? 0) < CATALOG_AGE };
  } catch {
    return { catalog: null, fresh: false };
  }
}

export function saveCatalog(catalog: ChatCatalog) {
  try {
    localStorage.setItem(CATALOG_KEY, JSON.stringify({ at: Date.now(), catalog }));
  } catch {
    // Not kept: Claude Code is asked again next time.
  }
}

/** "5m ago", "3h ago", "2d ago", else the date. */
export function ago(ms: number, now = Date.now()): string {
  const mins = Math.max(0, Math.round((now - ms) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  if (mins < 24 * 60) return `${Math.floor(mins / 60)}h ago`;
  if (mins < 7 * 24 * 60) return `${Math.floor(mins / 1440)}d ago`;
  return new Date(ms).toLocaleDateString();
}

export const NO_OPTIONS: ChatOptions = { model: null, effort: null, mode: null, thinking: null, fast: null, outputStyle: null, switchOnFlag: null };

export const sameOptions = (a: ChatOptions, b: ChatOptions) => (Object.keys(NO_OPTIONS) as (keyof ChatOptions)[]).every((k) => a[k] === b[k]);

/** "claude-opus-5-5[1m]" → "Opus 5.5 (1M)". */
export function prettyModel(id: string | null): string | null {
  if (!id) return null;
  const m = /^claude-([a-z]+)-(\d+)(?:-(\d{1,2}))?(?:-\d{8})?(\[1m\])?$/i.exec(id);
  if (!m) return id;
  const family = (m[1] ?? "").charAt(0).toUpperCase() + (m[1] ?? "").slice(1);
  return `${family} ${m[2]}${m[3] ? `.${m[3]}` : ""}${m[4] ? " (1M)" : ""}`;
}

const TEXT_EXT =
  /\.(txt|md|markdown|json|jsonc|ya?ml|toml|ini|env|cfg|conf|log|csv|tsv|xml|svg|html?|css|scss|less|js|mjs|cjs|jsx|ts|tsx|vue|svelte|php|blade|py|go|rs|rb|java|kt|c|h|cpp|hpp|cs|swift|sh|bash|ps1|bat|cmd|sql|graphql|gql|dockerfile|gitignore|lock)$/i;
const IMAGE = /^image\/(png|jpeg|gif|webp)$/;
const MAX_TEXT = 256 * 1024;
export const MAX_FILE = 20 * 1024 * 1024;

const dataUrl = (file: File) =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).replace(/^data:[^,]*,/, ""));
    r.onerror = () => reject(r.error ?? new Error(`cannot read ${file.name}`));
    r.readAsDataURL(file);
  });

/** Reads a pasted or dropped file the way Zenv sends it. */
export async function toAttachment(file: File): Promise<ChatAttachment> {
  const name = file.name || "pasted";
  if (file.size > MAX_FILE) throw new Error(`${name} is larger than 20 MB`);
  if (IMAGE.test(file.type)) return { kind: "image", name, mediaType: file.type, data: await dataUrl(file) };
  const textual = file.type.startsWith("text/") || /json|xml|javascript|yaml|toml/.test(file.type) || TEXT_EXT.test(name);
  if (textual && file.size <= MAX_TEXT) return { kind: "text", name, mediaType: file.type || "text/plain", data: await file.text() };
  return { kind: "file", name, mediaType: file.type || "application/octet-stream", data: await dataUrl(file) };
}

/** "57m", "3h 5m" until `epochSeconds`. */
export function untilText(epochSeconds: number, now = Date.now()): string {
  const mins = Math.max(0, Math.round((epochSeconds * 1000 - now) / 60000));
  return mins < 60 ? `${mins}m` : `${Math.floor(mins / 60)}h ${mins % 60}m`;
}
