import { ExternalLinkIcon, RotateCwIcon } from "lucide-react";
import { useState } from "react";

import { api } from "@/lib/api/client";
import type { SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { panelButton } from "./ui";

/** The app as served by Zenv, with its own address bar. */
export function Preview({ site, blockPointer }: { site: SiteView; blockPointer: boolean }) {
  const base = site.url.replace(/\/+$/, "");
  const [path, setPath] = useState("/");
  const [draft, setDraft] = useState("/");
  const [reload, setReload] = useState(0);
  const src = `${base}${path}`;

  return (
    <section className="flex min-w-0 flex-1 flex-col bg-background text-foreground" aria-label="Preview">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-panel-line bg-panel px-2">
        <button
          type="button"
          className={cn(panelButton, "size-7")}
          title="Reload"
          aria-label="Reload preview"
          onClick={() => setReload((n) => n + 1)}
        >
          <RotateCwIcon />
        </button>
        <form
          className="flex h-7 min-w-0 flex-1 items-center rounded-md border border-panel-line bg-panel-raised px-2 font-mono text-[11px]"
          onSubmit={(e) => {
            e.preventDefault();
            const p = draft.trim() || "/";
            setPath(p.startsWith("/") ? p : `/${p}`);
            setReload((n) => n + 1);
          }}
        >
          <span className="shrink-0 text-panel-muted">{base}</span>
          <input
            aria-label="Path"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-panel-foreground outline-none"
          />
        </form>
        <button
          type="button"
          className={cn(panelButton, "size-7")}
          title="Open in browser"
          aria-label="Open in browser"
          disabled={!site.enabled}
          onClick={() => void api.openSiteUrl(site.id)}
        >
          <ExternalLinkIcon />
        </button>
      </div>
      <div className="relative min-h-0 flex-1 bg-white">
        {site.enabled ? (
          <iframe
            key={reload}
            src={src}
            title={`Preview of ${site.domain}`}
            className="h-full w-full border-0"
            style={{ pointerEvents: blockPointer ? "none" : undefined }}
          />
        ) : (
          <div className="dot-grid grid h-full place-items-center p-6 text-sm text-muted-foreground">
            This app isn't being served. Turn on "Serve this app" in Zenv to preview it.
          </div>
        )}
      </div>
    </section>
  );
}
