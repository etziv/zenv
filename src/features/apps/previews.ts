import type { QueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";

import { api } from "@/lib/api/client";
import { thumbnailKey } from "@/lib/api/queries";

/**
 * App screenshots are taken one at a time (each is a headless browser run),
 * in the order they were asked for. Automatic captures that failed are not
 * retried this session; "Refresh preview" always tries again.
 */
let chain: Promise<void> = Promise.resolve();
const pending = new Set<string>();
const failed = new Map<string, string>();
const listeners = new Set<() => void>();
let version = 0;

function changed() {
  version += 1;
  for (const l of listeners) l();
}

export function capturePreview(qc: QueryClient, id: string, { force = false } = {}): void {
  if (pending.has(id) || (!force && failed.has(id))) return;
  pending.add(id);
  failed.delete(id);
  changed();
  chain = chain.then(async () => {
    try {
      qc.setQueryData(thumbnailKey(id), await api.captureSiteThumbnail(id));
    } catch (e) {
      failed.set(id, e instanceof Error ? e.message : String((e as { message?: string })?.message ?? e));
    } finally {
      pending.delete(id);
      changed();
    }
  });
}

export function previewState(id: string): { capturing: boolean; error: string | null; triedAutomatically: boolean } {
  return { capturing: pending.has(id), error: failed.get(id) ?? null, triedAutomatically: failed.has(id) };
}

/** Re-renders when any capture starts or finishes. */
export function usePreviewQueue(): number {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => version,
  );
}
