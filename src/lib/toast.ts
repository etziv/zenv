import { useSyncExternalStore } from "react";

export interface Toast {
  id: number;
  variant: "error" | "success" | "info";
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

const AUTO_DISMISS_MS: Record<Toast["variant"], number> = { error: 12000, success: 5000, info: 6000 };
const MAX_VISIBLE = 4;

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function dismissToast(id: number): void {
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

/** Shows a notification in the corner, on whatever page is open. */
export function toast(t: Omit<Toast, "id">): number {
  const id = nextId++;
  // The same message again replaces the old one instead of stacking.
  toasts = [...toasts.filter((x) => !(x.title === t.title && x.description === t.description)), { ...t, id }].slice(-MAX_VISIBLE);
  emit();
  setTimeout(() => dismissToast(id), AUTO_DISMISS_MS[t.variant]);
  return id;
}

export function useToasts(): Toast[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => toasts,
  );
}
