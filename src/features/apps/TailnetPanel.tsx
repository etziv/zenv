import { CheckIcon, CopyIcon, ExternalLinkIcon, LoaderCircleIcon, NetworkIcon, XIcon } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useTailnetAction, useTailnetApps, useTailnetStatus } from "@/lib/api/queries";
import type { SiteView, TailnetStatus } from "@/lib/api/types";
import { toast } from "@/lib/toast";

/** "vault-warden" from "Vault_Warden.test": a DNS label for a Service name. */
export function serviceNameFor(domain: string): string {
  const label = (domain.split(".")[0] ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 63);
  return label || "app";
}

export const validService = (s: string) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(s);

/** What this computer still needs before it can host a Tailscale Service. */
export function serviceProblem(st: TailnetStatus | undefined): string | null {
  if (!st) return null;
  if (st.problem) return st.problem;
  if (!st.tags.length) return "Tag this computer in Tailscale first (for example tag:server): Services need a tagged host.";
  if (!st.https) return "Turn on HTTPS certificates in the Tailscale admin console (DNS page) first.";
  return null;
}

/**
 * Opens an app to the devices signed in to the same Tailscale account: at
 * http://<this computer>:<port>, or as a Tailscale Service with its own name
 * (https://<name>.<tailnet>.ts.net). Nothing outside the tailnet can reach it.
 */
export function TailnetPanel({ site, serving }: { site: SiteView; serving: boolean }) {
  const status = useTailnetStatus();
  const apps = useTailnetApps();
  const action = useTailnetAction();
  const [copied, setCopied] = useState(false);
  const [ownName, setOwnName] = useState(false);
  const [service, setService] = useState(() => serviceNameFor(site.domain));
  const open = apps.data?.find((a) => a.siteId === site.id);
  const pending = action.isPending && action.variables?.siteId === site.id;
  const st = status.data;
  const tailnet = st?.dnsName?.split(".").slice(1).join(".");

  const copy = (url: string) => {
    void navigator.clipboard?.writeText(url).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };

  if (open) {
    const url = open.url ?? open.fullUrl ?? `port ${open.port}`;
    return (
      <div className="grid gap-2">
        <div className="flex min-w-0 items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-2.5 py-2">
          <span className="led-on size-1.5 shrink-0 rounded-full" />
          <span className="min-w-0 flex-1 truncate font-mono text-xs" title={open.fullUrl ?? url}>
            {url.replace(/^https?:\/\//, "")}
          </span>
          <Button size="icon" variant="ghost" className="size-7" aria-label="Copy tailnet link" title="Copy link" onClick={() => copy(open.url ?? open.fullUrl ?? "")}>
            {copied ? <CheckIcon className="text-success" /> : <CopyIcon />}
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="size-7"
            aria-label="Open tailnet link"
            title="Open link"
            onClick={() => api.tailnetOpenUrl(site.id).catch((e: Error) => toast({ variant: "error", title: "Couldn't open the link", description: e.message }))}
          >
            <ExternalLinkIcon />
          </Button>
        </div>
        {open.service ? (
          <p className="text-[11px] leading-relaxed text-muted-foreground">
            Tailscale Service <span className="font-mono text-foreground">svc:{open.service}</span>. If the link doesn't load yet: in the Tailscale admin console →
            Services, define <span className="font-mono text-foreground">svc:{open.service}</span> with port <span className="font-mono">tcp:443</span> and approve this
            computer as its host.
          </p>
        ) : (
          open.ipUrl && (
            <p className="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
              or
              <button
                type="button"
                className="truncate font-mono text-foreground hover:text-primary"
                title="Copy the tailnet IP address"
                onClick={() => copy(open.ipUrl ?? "")}
              >
                {open.ipUrl.replace("http://", "")}
              </button>
            </p>
          )
        )}
        <div className="flex items-center justify-between gap-2">
          <p className="text-[11px] text-muted-foreground">
            Devices on your Tailscale account{serving ? "" : " (while its web server runs)"}.
          </p>
          <Button size="sm" variant="outline" disabled={pending} onClick={() => action.mutate({ action: "close", siteId: site.id, domain: site.domain })}>
            {pending ? <LoaderCircleIcon className="animate-spin" /> : <XIcon />} Close
          </Button>
        </div>
      </div>
    );
  }

  const base = st?.problem ?? (!site.enabled ? "Switch the app on first." : null);
  const svcProblem = ownName ? serviceProblem(st) : null;
  const nameProblem = ownName && !validService(service) ? "Use lower-case letters, digits and hyphens." : null;
  const problem = base ?? svcProblem ?? nameProblem;
  return (
    <div className="grid gap-2.5">
      <label className="flex items-center justify-between gap-3 text-xs">
        <span>
          Own name <span className="text-muted-foreground">(Tailscale Service, HTTPS)</span>
        </span>
        <Switch checked={ownName} onCheckedChange={setOwnName} aria-label="Own name as a Tailscale Service" />
      </label>
      {ownName && (
        <div className="flex min-w-0 items-center gap-1.5">
          <Input
            value={service}
            onChange={(e) => setService(e.target.value.toLowerCase())}
            aria-label="Service name"
            className="h-8 min-w-0 flex-1 font-mono text-xs"
            spellCheck={false}
          />
          <span className="shrink-0 truncate font-mono text-[11px] text-muted-foreground">.{tailnet ?? "<tailnet>.ts.net"}</span>
        </div>
      )}
      <div className="flex items-center justify-between gap-3">
        <p className="min-w-0 text-[11px] text-muted-foreground">
          {problem ??
            (ownName
              ? `https://${service}.${tailnet ?? "<tailnet>.ts.net"} on your other devices.`
              : `Open it on your other devices with Tailscale, at ${st?.host ?? "this computer"}:<port>.`)}
        </p>
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          disabled={Boolean(problem) || pending || status.isPending}
          onClick={() => action.mutate({ action: "open", siteId: site.id, domain: site.domain, service: ownName ? service : null })}
        >
          {pending ? <LoaderCircleIcon className="animate-spin" /> : <NetworkIcon />} {pending ? "Opening…" : "Open on tailnet"}
        </Button>
      </div>
    </div>
  );
}
