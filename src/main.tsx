import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, Suspense, lazy } from "react";
import { createRoot } from "react-dom/client";

import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import { App } from "./App";
import { TrayPanel } from "./features/tray/TrayPanel";
import { applyTheme } from "./lib/theme";
import "./index.css";

applyTheme();

const queryClient = new QueryClient({
  defaultOptions: {
    // Backend calls are local IPC; retrying a failed validation or start
    // would only repeat the same error.
    queries: { retry: false, refetchOnWindowFocus: true },
    mutations: { retry: false },
  },
});

/** The tray icon's panel is a second window of the same app. */
function windowLabel(): string {
  try {
    return getCurrentWebviewWindow().label;
  } catch {
    return "main";
  }
}

// The editor (CodeMirror, Prettier) loads only in Studio windows.
const StudioWindow = lazy(() => import("./features/studio/StudioWindow").then((m) => ({ default: m.StudioWindow })));

/** An app's Studio window is labelled `studio-<site id>`. */
function Screen() {
  const label = windowLabel();
  if (label === "tray") return <TrayPanel />;
  if (label.startsWith("studio-")) {
    return (
      <Suspense fallback={<div className="h-full bg-panel" />}>
        <StudioWindow siteId={label.slice("studio-".length)} />
      </Suspense>
    );
  }
  return <App />;
}

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Screen />
    </QueryClientProvider>
  </StrictMode>,
);
