import { api } from "@/lib/api/client";

/** Prettier's parser for a file name; null when Prettier can't format it. */
export function prettierParser(name: string): string | null {
  const lower = name.toLowerCase();
  // Prettier's PHP plugin would break Blade's @directives and {{ }}.
  if (lower.endsWith(".blade.php")) return null;
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : "";
  switch (ext) {
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return "babel";
    case "ts":
    case "tsx":
    case "mts":
    case "cts":
      return "typescript";
    case "json":
      return lower === "package.json" ? "json-stringify" : "json";
    case "css":
      return "css";
    case "scss":
      return "scss";
    case "less":
      return "less";
    case "html":
    case "htm":
      return "html";
    case "vue":
      return "vue";
    case "md":
    case "markdown":
      return "markdown";
    case "yml":
    case "yaml":
      return "yaml";
    case "graphql":
    case "gql":
      return "graphql";
    case "php":
      return "php";
    default:
      return null;
  }
}

/** Prettier and its plugins load the first time something is formatted. */
async function loadPrettier() {
  const [prettier, ...plugins] = await Promise.all([
    import("prettier/standalone"),
    import("prettier/plugins/babel"),
    import("prettier/plugins/estree"),
    import("prettier/plugins/typescript"),
    import("prettier/plugins/postcss"),
    import("prettier/plugins/html"),
    import("prettier/plugins/markdown"),
    import("prettier/plugins/yaml"),
    import("prettier/plugins/graphql"),
    import("@prettier/plugin-php/standalone"),
  ]);
  return { prettier, plugins: plugins.map((p) => ("default" in p ? p.default : p)) as never[] };
}

let loading: ReturnType<typeof loadPrettier> | null = null;

/**
 * The project's Prettier options: `.prettierrc` / `.prettierrc.json` (JSON),
 * else the `prettier` key of package.json. JS/YAML configs are not read.
 */
async function projectOptions(siteId: string): Promise<Record<string, unknown>> {
  for (const file of [".prettierrc", ".prettierrc.json"]) {
    try {
      const f = await api.projectRead(siteId, file);
      return JSON.parse(f.content) as Record<string, unknown>;
    } catch {
      // Not there, or not JSON.
    }
  }
  try {
    const pkg = JSON.parse((await api.projectRead(siteId, "package.json")).content) as { prettier?: unknown };
    if (pkg.prettier && typeof pkg.prettier === "object") return pkg.prettier as Record<string, unknown>;
  } catch {
    // No package.json.
  }
  return {};
}

/** The file formatted by Prettier, with the cursor kept near where it was. */
export async function formatWithPrettier(
  siteId: string,
  path: string,
  text: string,
  cursorOffset: number,
): Promise<{ formatted: string; cursorOffset: number }> {
  const name = path.split("/").pop() ?? path;
  const parser = prettierParser(name);
  if (!parser) throw new Error(`Prettier can't format ${name}.`);
  loading ??= loadPrettier();
  const [{ prettier, plugins }, options] = await Promise.all([loading, projectOptions(siteId)]);
  const crlf = text.includes("\r\n");
  const result = await prettier.formatWithCursor(text, {
    endOfLine: crlf ? "crlf" : "lf",
    ...options,
    parser,
    plugins,
    cursorOffset: Math.max(0, Math.min(cursorOffset, text.length)),
    filepath: name,
  });
  return { formatted: result.formatted, cursorOffset: Math.max(0, result.cursorOffset) };
}

/** Go, Rust and Python go to their own formatters, run by Zenv. */
function nativeFormatter(name: string): string | null {
  const ext = name.toLowerCase().split(".").pop();
  if (ext === "go") return "gofmt";
  if (ext === "rs") return "rustfmt";
  if (ext === "py" || ext === "pyi") return "ruff";
  return null;
}

/** What formats this file, for the status line; null when nothing can. */
export function formatterFor(path: string): string | null {
  const name = path.split("/").pop() ?? path;
  return nativeFormatter(name) ?? (prettierParser(name) ? "Prettier" : null);
}

/** Formats with Prettier, or with gofmt / rustfmt / ruff for Go, Rust and Python. */
export async function formatCode(
  siteId: string,
  path: string,
  text: string,
  cursorOffset: number,
): Promise<{ formatted: string; cursorOffset: number }> {
  const name = path.split("/").pop() ?? path;
  if (nativeFormatter(name)) {
    const formatted = await api.projectFormat(siteId, path, text);
    return { formatted, cursorOffset: Math.min(cursorOffset, formatted.length) };
  }
  return formatWithPrettier(siteId, path, text, cursorOffset);
}

// ----- "Format on save", remembered per computer --------------------------------

const KEY = "zenv.studio.formatOnSave";
const listeners = new Set<() => void>();

export function formatOnSave(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

export function setFormatOnSave(on: boolean) {
  try {
    localStorage.setItem(KEY, on ? "1" : "0");
  } catch {
    // Storage unavailable: the choice lasts until the window closes.
  }
  for (const l of listeners) l();
}

export function subscribeFormatOnSave(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Asks the editor showing `path` to format it (the status line's button). */
export const FORMAT_EVENT = "zenv-studio-format";
