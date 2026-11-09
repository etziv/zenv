import type { Installation, PhpBackend, PhpEngine } from "@/lib/api/types";

/** PHP version a FrankenPHP build embeds, from its version line. */
export function embeddedPhp(versionDetail: string): string | null {
  return /\bPHP (\d+\.\d+\.\d+)/.exec(versionDetail)?.[1] ?? null;
}

export function minorOf(version: string): string | null {
  const m = /^(\d+)\.(\d+)/.exec(version);
  return m ? `${m[1]}.${m[2]}` : null;
}

export type Resolution =
  | { backend: PhpBackend; franken: Installation | null }
  | { backend: null; error: string };

/** Mirrors the backend's choice (php_runtime::select) for a live preview. */
export function resolveBackend(engine: PhpEngine, php: Installation, frankens: Installation[]): Resolution {
  const want = minorOf(php.version);
  const matching = frankens
    .filter((f) => f.enabled && f.executableExists && want !== null && minorOf(embeddedPhp(f.versionDetail) ?? "") === want)
    .sort((a, b) => Number(b.isDefault) - Number(a.isDefault));
  const franken = matching[0] ?? null;
  switch (engine) {
    case "fastcgi":
      return { backend: "fastcgi", franken: null };
    case "auto":
      return franken ? { backend: "frankenphp", franken } : { backend: "fastcgi", franken: null };
    case "frankenphp":
      return franken
        ? { backend: "frankenphp", franken }
        : { backend: null, error: `No FrankenPHP with PHP ${want ?? php.version} is set up and switched on.` };
    default: {
      const unreachable: never = engine;
      return unreachable;
    }
  }
}
