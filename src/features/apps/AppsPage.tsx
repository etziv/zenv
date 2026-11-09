import {
  ChevronLeftIcon,
  ChevronRightIcon,
  PlusIcon,
  SearchIcon,
  TriangleAlertIcon,
} from "lucide-react";
import { useEffect, useState } from "react";

import { useNavigate } from "@/app/navigation";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { ErrorPanel } from "@/components/error-panel";
import { EmptyState, Loading, Mono, Page, PageHeader } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  useHostsStatus,
  useInstallations,
  useServices,
  useSiteAction,
  useSites,
} from "@/lib/api/queries";
import type { ServiceStatus, SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { HostsDialog } from "../sites/HostsDialog";
import { SiteDetailsDialog } from "../sites/SiteDetailsDialog";
import { SiteFormDialog } from "../sites/SiteFormDialog";
import { AppCard } from "./AppCard";
import { AppPanel, type PanelAction } from "./AppPanel";

const PAGE_SIZE = 48;

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

type Overlay =
  | { kind: "form"; site: SiteView | null }
  | { kind: "details"; site: SiteView; tab?: "process" }
  | { kind: "delete"; site: SiteView }
  | { kind: "hosts" }
  | null;

/** Is a web server that serves this app running right now? */
function isServing(site: SiteView, webServers: ServiceStatus[]): boolean {
  return site.serviceId
    ? webServers.some(
        (s) => s.service.id === site.serviceId && s.health === "running",
      )
    : webServers.some((s) => s.health === "running");
}

export function AppsPage() {
  const navigate = useNavigate();
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const debounced = useDebounced(search.trim(), 250);
  const sites = useSites({
    search: debounced,
    offset: page * PAGE_SIZE,
    limit: PAGE_SIZE,
  });
  const services = useServices();
  const installations = useInstallations();
  const hosts = useHostsStatus();
  const remove = useSiteAction();
  const [overlay, setOverlay] = useState<Overlay>(null);

  useEffect(() => setPage(0), [debounced]);

  const webServers = (services.data ?? []).filter(
    (s) => s.kind === "web_server",
  );
  const phpVersions = (installations.data ?? []).filter(
    (i) => i.component === "php" && i.executableExists,
  );
  const frankens = (installations.data ?? []).filter(
    (i) => i.component === "frankenphp",
  );
  const pendingHosts = hosts.data?.plan.changed ? hosts.data.plan : null;
  const items = sites.data?.items ?? [];
  const total = sites.data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const selected = items.find((s) => s.id === selectedId) ?? items[0] ?? null;

  const act = (site: SiteView) => (a: PanelAction) => {
    if (a === "edit") setOverlay({ kind: "form", site });
    else if (a === "details") setOverlay({ kind: "details", site });
    else if (a === "output")
      setOverlay({ kind: "details", site, tab: "process" });
    else setOverlay({ kind: "delete", site });
  };

  return (
    <Page>
      <PageHeader
        title="Apps"
        meta={
          sites.data
            ? `${total} app${total === 1 ? "" : "s"}${debounced ? " found" : ""}`
            : undefined
        }
        actions={
          <>
            <div className="relative w-64">
              <SearchIcon className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search apps"
                className="pl-8"
                aria-label="Search apps"
              />
            </div>
            <Button
              onClick={() => setOverlay({ kind: "form", site: null })}
              disabled={webServers.length === 0}
            >
              <PlusIcon /> New app
            </Button>
          </>
        }
      />

      {pendingHosts &&
        (pendingHosts.add.length > 0 || pendingHosts.remove.length > 0) && (
          <Card className="flex flex-wrap items-center gap-3 border-warning/40 bg-warning/5 p-3">
            <TriangleAlertIcon className="size-4 text-warning" />
            <p className="flex-1 text-sm">
              {pendingHosts.add.length > 0
                ? `${pendingHosts.add.length} domain${pendingHosts.add.length === 1 ? "" : "s"} still need${pendingHosts.add.length === 1 ? "s" : ""} a hosts-file entry before ${pendingHosts.add.length === 1 ? "it works" : "they work"} in your browser.`
                : `The hosts file still lists ${pendingHosts.remove.length} domain${pendingHosts.remove.length === 1 ? "" : "s"} of removed or switched-off apps.`}
            </p>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setOverlay({ kind: "hosts" })}
            >
              Review & update hosts file
            </Button>
          </Card>
        )}

      {sites.isPending ? (
        <Loading />
      ) : sites.error ? (
        <ErrorPanel error={sites.error} />
      ) : total === 0 && !debounced ? (
        webServers.length === 0 ? (
          <EmptyState
            title="Add a web server first"
            action={
              <Button variant="outline" onClick={() => navigate("web-servers")}>
                Go to Web Servers
              </Button>
            }
          >
            Apps are served by a web server Zenv manages. Add Apache or Nginx,
            then come back to add your projects.
          </EmptyState>
        ) : (
          <EmptyState
            title="No apps yet"
            action={
              <Button onClick={() => setOverlay({ kind: "form", site: null })}>
                <PlusIcon /> New app
              </Button>
            }
          >
            Pick a project folder. Zenv suggests a domain such as{" "}
            <Mono>my-app.test</Mono> and how to serve it.
          </EmptyState>
        )
      ) : total === 0 ? (
        <p className="text-sm text-muted-foreground">
          No app matches “{debounced}”.
        </p>
      ) : (
        <div className="grid grid-cols-[minmax(0,1fr)_22rem] items-start gap-5">
          <div className="grid gap-3">
            <ul
              className={cn(
                "grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-3",
                sites.isPlaceholderData && "opacity-70",
              )}
            >
              {items.map((s) => (
                <AppCard
                  key={s.id}
                  site={s}
                  selected={s.id === selected?.id}
                  serving={isServing(s, webServers)}
                  removing={
                    remove.isPending &&
                    remove.variables?.action === "delete" &&
                    remove.variables.id === s.id
                  }
                  onSelect={() => setSelectedId(s.id)}
                  onAction={act(s)}
                />
              ))}
            </ul>
            {pages > 1 && (
              <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
                <span>
                  {page * PAGE_SIZE + 1}–
                  {Math.min(total, (page + 1) * PAGE_SIZE)} of {total}
                </span>
                <Button
                  size="icon"
                  variant="ghost"
                  aria-label="Previous page"
                  disabled={page === 0}
                  onClick={() => setPage((p) => p - 1)}
                >
                  <ChevronLeftIcon />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  aria-label="Next page"
                  disabled={page + 1 >= pages}
                  onClick={() => setPage((p) => p + 1)}
                >
                  <ChevronRightIcon />
                </Button>
              </div>
            )}
          </div>
          <div className="sticky top-4 min-w-0">
            {selected && (
              <AppPanel
                key={selected.id}
                site={selected}
                webServers={webServers}
                phpVersions={phpVersions}
                frankens={frankens}
                onAction={act(selected)}
              />
            )}
          </div>
        </div>
      )}

      {overlay?.kind === "form" && (
        <SiteFormDialog
          services={webServers}
          phpVersions={phpVersions}
          frankens={frankens}
          editing={overlay.site}
          onOpenChange={(o) => !o && setOverlay(null)}
        />
      )}
      {overlay?.kind === "details" && (
        <SiteDetailsDialog
          site={overlay.site}
          initialTab={overlay.tab}
          onOpenChange={(o) => !o && setOverlay(null)}
        />
      )}
      {overlay?.kind === "hosts" && hosts.data && (
        <HostsDialog
          open
          status={hosts.data}
          onOpenChange={(o) => !o && setOverlay(null)}
        />
      )}
      {overlay?.kind === "delete" && (
        <ConfirmDialog
          open
          onOpenChange={(o) => !o && setOverlay(null)}
          title={`Remove ${overlay.site.domain}?`}
          confirmLabel="Remove app"
          pendingLabel="Removing…"
          destructive
          pending={remove.isPending}
          onConfirm={() =>
            remove.mutate(
              { action: "delete", id: overlay.site.id },
              { onSettled: () => setOverlay(null) },
            )
          }
        >
          <p>
            Zenv stops serving <Mono>{overlay.site.domain}</Mono> and forgets
            this app. The project folder and its files stay exactly as they are:
          </p>
          <Mono>{overlay.site.projectRoot}</Mono>
        </ConfirmDialog>
      )}
      {remove.error && <ErrorPanel error={remove.error} />}
    </Page>
  );
}
