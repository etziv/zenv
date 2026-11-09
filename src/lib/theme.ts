import { useSyncExternalStore } from "react";

/** "system" follows Windows' light/dark setting. */
export type Theme = "system" | "light" | "dark";

const KEY = "zenv.theme";
const listeners = new Set<() => void>();

function read(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

let current: Theme = read();

/** Applies the theme to <html>; the stylesheet keys off data-theme. */
export function applyTheme(theme: Theme = current): void {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export function setTheme(theme: Theme): void {
  current = theme;
  try {
    if (theme === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, theme);
  } catch {
    // Storage unavailable: the choice still applies until the app restarts.
  }
  applyTheme(theme);
  for (const l of listeners) l();
}

export function useTheme(): Theme {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}

const NEXT: Record<Theme, Theme> = { system: "light", light: "dark", dark: "system" };

export function nextTheme(theme: Theme): Theme {
  return NEXT[theme];
}
