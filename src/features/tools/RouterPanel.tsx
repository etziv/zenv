import { CheckIcon, CopyIcon, LayoutDashboardIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useCodexAction, useCodexStatus, useRouterAction, useRouterStatus } from "@/lib/api/queries";
import type { RouterId, ServiceStatus } from "@/lib/api/types";
import { toast } from "@/lib/toast";

import { CodexModelPicker } from "./CodexModelPicker";

const NAME: Record<RouterId, string> = { omniroute: "OmniRoute", ninerouter: "9router" };

/** Shortcut groups per router, in the order the dashboard pages are usually needed. */
const GROUPS: Record<RouterId, { label: string; pages: string[] }[]> = {
  omniroute: [
    { label: "Providers", pages: ["providers", "combos"] },
    { label: "Access", pages: ["api-manager", "api-endpoints"] },
    { label: "Spend", pages: ["usage", "quota", "costs"] },
    { label: "Watch", pages: ["logs", "health"] },
  ],
  ninerouter: [
    { label: "Providers", pages: ["providers", "combos"] },
    { label: "Access", pages: ["endpoint", "cli-tools"] },
    { label: "Spend", pages: ["usage", "quota"] },
    { label: "Setup", pages: ["settings"] },
  ],
};

/**
 * An AI router inside Zenv: its dashboard and pages in a Zenv window, the API
 * base URL, updates (automatic unless switched off) and Codex.
 */
export function RouterPanel({ component, status }: { component: RouterId; status: ServiceStatus }) {
  const name = NAME[component];
  const router = useRouterStatus(component);
  const action = useRouterAction(component, name);
  const [copied, setCopied] = useState(false);
  const running = status.health === "running";
  const listen = status.service.listenAddress;
  const host = listen === "0.0.0.0" ? "127.0.0.1" : listen === "::" ? "[::1]" : listen.includes(":") ? `[${listen}]` : listen;
  const baseUrl = `http://${host}:${status.service.httpPort}/v1`;
  const labels = new Map((router.data?.pages ?? []).map((p) => [p.id, p.label]));
  const open = (page: string | null) =>
    api.routerOpen(status.service.id, page).catch((e: Error) => toast({ variant: "error", title: `Couldn't open ${name}`, description: e.message }));
  const copy = () => {
    void navigator.clipboard?.writeText(baseUrl).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };
  const o = router.data;
  const newer = o?.latest && o.installed && o.latest !== o.installed;

  return (
    <div className="grid gap-3 border-t px-3.5 py-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={!running} title={running ? undefined : `Start ${name} first`} onClick={() => open(null)}>
          <LayoutDashboardIcon /> Open dashboard
        </Button>
        <Button size="sm" variant="outline" onClick={copy} title="For Claude Code, Cursor, Codex and other tools">
          {copied ? <CheckIcon className="text-success" /> : <CopyIcon />} <span className="font-mono text-[11px]">{baseUrl}</span>
        </Button>
      </div>

      <div className="grid gap-1.5 sm:grid-cols-2">
        {GROUPS[component].map((g) => (
          <div key={g.label} className="flex min-w-0 flex-wrap items-center gap-1">
            <span className="w-16 shrink-0 font-mono text-[10px] uppercase tracking-[0.12em] text-muted-foreground">{g.label}</span>
            {g.pages.map((p) => (
              <Button
                key={p}
                size="sm"
                variant="ghost"
                className="h-7 px-2 text-xs"
                disabled={!running}
                title={running ? `Open ${labels.get(p) ?? p} in ${name}` : `Start ${name} first`}
                onClick={() => open(p)}
              >
                {labels.get(p) ?? p}
              </Button>
            ))}
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs">
        <span className="text-muted-foreground">
          {o?.updating
            ? "Checking for an update…"
            : newer
              ? `Version ${o.latest} is available${o.autoUpdate && o.autoFrom ? ` · installs automatically from ${new Date(o.autoFrom).toLocaleDateString()}` : ""}`
              : o?.lastChecked
                ? `Up to date · checked ${new Date(o.lastChecked).toLocaleString()}`
                : "Not checked yet"}
        </span>
        <Button size="sm" variant="outline" className="h-7 px-2 text-xs" disabled={o?.updating || action.isPending} onClick={() => action.mutate({ action: "update", now: !!newer })}>
          {o?.updating || action.isPending ? <LoaderCircleIcon className="animate-spin" /> : <RefreshCwIcon />}
          {newer ? "Install now" : "Check for updates"}
        </Button>
        <label className="ml-auto flex items-center gap-2">
          Auto-update
          <Switch checked={o?.autoUpdate ?? true} disabled={!o || action.isPending} onCheckedChange={(on) => action.mutate({ action: "auto", on })} aria-label={`Update ${name} automatically`} />
        </label>
      </div>
      {o?.lastError && <p className="text-xs text-destructive">{o.lastError}</p>}

      <CodexRow component={component} name={name} serviceId={status.service.id} baseUrl={baseUrl} running={running} />
    </div>
  );
}

/** Codex CLI: add the router as a provider/profile, and make it the default. */
function CodexRow({
  component,
  name,
  serviceId,
  baseUrl,
  running,
}: {
  component: RouterId;
  name: string;
  serviceId: string;
  baseUrl: string;
  running: boolean;
}) {
  const codex = useCodexStatus(component);
  const action = useCodexAction(component, name);
  const [showText, setShowText] = useState(false);
  const [copiedText, setCopiedText] = useState(false);
  const snippet = useQuery({ queryKey: ["codexSnippet", serviceId, baseUrl], queryFn: () => api.codexSnippet(serviceId), enabled: showText });
  const c = codex.data;
  if (!c) return null;
  const stale = c.configured && c.baseUrl !== baseUrl;
  const otherDefault = c.defaultProfile ?? c.topLevelProvider;
  return (
    <div className="grid gap-1.5 border-t pt-3 text-xs">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-muted-foreground">Codex</span>
        <span className="min-w-0 flex-1 text-muted-foreground">
          {!c.configured
            ? `Not set up${otherDefault ? ` · Codex now uses ${otherDefault}` : ""}`
            : c.isDefault
              ? `Uses ${name} by default`
              : `Profile ready: codex --profile ${component}${otherDefault ? ` · default is ${otherDefault}` : ""}`}
        </span>
        {(!c.configured || stale) && (
          <Button size="sm" variant="outline" className="h-7 px-2 text-xs" disabled={action.isPending} onClick={() => action.mutate({ action: "setup", serviceId, makeDefault: false })}>
            {stale ? "Update address" : "Add to Codex"}
          </Button>
        )}
        {c.configured && (
          <label className="flex items-center gap-2">
            Default for Codex
            <Switch checked={c.isDefault} disabled={action.isPending} onCheckedChange={(on) => action.mutate({ action: "default", on })} aria-label={`Codex uses ${name} by default`} />
          </label>
        )}
      </div>
      {c.configured && <CodexModelPicker key={`${c.model}-${c.isDefault}`} component={component} name={name} serviceId={serviceId} running={running} codex={c} />}
      <div className="flex items-center gap-2">
        <p className="min-w-0 flex-1 truncate font-mono text-[10.5px] text-muted-foreground" title={c.configPath}>
          {c.configPath}
        </p>
        <Button size="sm" variant="ghost" className="h-6 px-1.5 text-[11px]" onClick={() => setShowText((v) => !v)}>
          {showText ? "Hide config text" : "Show config text"}
        </Button>
      </div>
      {showText && (
        <div className="relative">
          <pre className="max-h-64 overflow-auto rounded-md border bg-muted/40 p-2.5 pr-20 font-mono text-[11px] leading-relaxed whitespace-pre">
            {snippet.data ?? "Loading…"}
          </pre>
          <Button
            size="sm"
            variant="outline"
            className="absolute top-2 right-2 h-7 px-2 text-xs"
            disabled={!snippet.data}
            onClick={() =>
              void navigator.clipboard?.writeText(snippet.data ?? "").then(() => {
                setCopiedText(true);
                window.setTimeout(() => setCopiedText(false), 1500);
              })
            }
          >
            {copiedText ? <CheckIcon className="text-success" /> : <CopyIcon />} Copy
          </Button>
          <p className="mt-1 text-[11px] text-muted-foreground">
            Paste it at the end of config.toml yourself, or let “Add to Codex” do it (with a backup).
          </p>
        </div>
      )}
    </div>
  );
}
