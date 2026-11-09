import { SearchIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";

import { EmptyState, Loading } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ErrorPanel } from "@/components/error-panel";
import { api } from "@/lib/api/client";
import { useServices, useSiteLogSources, useSites } from "@/lib/api/queries";
import type { ServiceStatus, SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { ListItem, LogPanel } from "./LogPanel";

/** The web server a site is served by right now (its own, or whichever is running). */
function servingServer(site: SiteView, services: ServiceStatus[]): ServiceStatus | undefined {
  if (site.serviceId) return services.find((s) => s.service.id === site.serviceId);
  const web = services.filter((s) => s.kind === "web_server");
  return web.find((s) => s.health === "running") ?? web.find((s) => s.health === "failed");
}

function ServerBanner({ site, onShowService }: { site: SiteView; onShowService: (id: string) => void }) {
  const services = useServices();
  if (!services.data) return null;
  const server = servingServer(site, services.data);
  if (server?.health === "failed") {
    return (
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm">
        <TriangleAlertIcon className="size-4 text-destructive" />
        <span className="flex-1">{server.service.name} stopped unexpectedly, so this site is not being served.</span>
        <Button size="sm" variant="outline" onClick={() => onShowService(server.service.id)}>
          View service log
        </Button>
      </div>
    );
  }
  if (!server || server.health !== "running") {
    return (
      <p className="rounded-md border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
        No web server is serving this site right now, so nothing new is being logged.
      </p>
    );
  }
  return null;
}

function SiteLogPanel({ site, onShowService }: { site: SiteView; onShowService: (id: string) => void }) {
  const sources = useSiteLogSources(site.id);
  if (sources.isPending) return <Loading />;
  if (sources.error) return <ErrorPanel error={sources.error} />;
  return (
    <LogPanel
      key={site.id}
      title={site.domain}
      subtitle={site.projectRoot}
      banner={<ServerBanner site={site} onShowService={onShowService} />}
      sources={sources.data}
      queryKey={["site", site.id]}
      read={(kind) => api.siteLog(site.id, kind)}
      reveal={(kind) => ({ kind: "siteLog", id: site.id, log: kind })}
    />
  );
}

export function ProjectLogs({ onShowService }: { onShowService: (serviceId: string) => void }) {
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const sites = useSites({ search, offset: 0, limit: 200 });
  const items = sites.data?.items ?? [];
  const site = items.find((s) => s.id === selected) ?? items[0];

  if (sites.isPending) return <Loading />;
  if (sites.error) return <ErrorPanel error={sites.error} />;
  if (sites.data.total === 0 && !search) {
    return (
      <EmptyState title="No sites yet">
        Each site gets its own access, error and PHP log here, plus the app's log (such as Laravel's) when it has one.
      </EmptyState>
    );
  }

  return (
    <div className="grid grid-cols-[15rem_1fr] items-start gap-4">
      <Card className="overflow-hidden">
        <div className="relative border-b p-2">
          <SearchIcon className="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Find a site"
            aria-label="Find a site"
            className="h-7 pl-7 text-xs"
          />
        </div>
        <div className="max-h-[calc(100vh-15rem)] overflow-y-auto py-1">
          {items.length === 0 && <p className="px-3 py-2 text-xs text-muted-foreground">No site matches.</p>}
          {items.map((s) => (
            <ListItem key={s.id} active={s.id === site?.id} onClick={() => setSelected(s.id)}>
              <span className={cn("size-1.5 shrink-0 rounded-full", s.enabled ? "bg-primary" : "bg-muted-foreground/40")} />
              <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{s.domain}</span>
            </ListItem>
          ))}
          {sites.data.total > items.length && (
            <p className="px-3 py-1.5 text-[11px] text-muted-foreground">Showing {items.length} of {sites.data.total}; search to narrow.</p>
          )}
        </div>
      </Card>
      {site ? <SiteLogPanel key={site.id} site={site} onShowService={onShowService} /> : <div />}
    </div>
  );
}
