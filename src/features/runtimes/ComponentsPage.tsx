import { useMutation } from "@tanstack/react-query";
import { DownloadIcon, PlusIcon, ScanSearchIcon } from "lucide-react";
import { useState } from "react";

import { ComponentLogo } from "@/components/component-logo";
import { ErrorPanel } from "@/components/error-panel";
import { Loading, Mono, Page, PageHeader } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

import { api, type ApiError } from "@/lib/api/client";
import {
  useCatalog,
  useInstallations,
  useRuntimes,
  useServices,
} from "@/lib/api/queries";
import type {
  Category,
  ComponentId,
  DiscoveredInstallation,
} from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { ComponentCard } from "./ComponentCard";
import { ServerVersionList } from "./ServerVersionList";
import { DiscoveryPanel } from "./DiscoveryPanel";
import { InstallDialog } from "./InstallDialog";
import { ComponentInstalls } from "./install-progress";
import { RegisterDialog } from "./RegisterDialog";

function Label({ children }: { children: React.ReactNode }) {
  return (
    <p className="font-mono text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
      {children}
    </p>
  );
}

/**
 * One layout for every kind of component: the components of a category on
 * the left; on the right the chosen one's servers (for services) and its
 * installed versions.
 */
export function ComponentsPage({
  category,
  title,
}: {
  category: Category;
  title: string;
}) {
  const catalog = useCatalog();
  const installations = useInstallations();
  const runtimes = useRuntimes();
  const services = useServices();
  const [picked, setPicked] = useState<ComponentId | null>(null);
  const [installing, setInstalling] = useState(false);
  const [registering, setRegistering] = useState(false);
  const scan = useMutation<DiscoveredInstallation[], ApiError>({
    mutationFn: () => api.discover(),
  });

  if (catalog.isPending || installations.isPending) {
    return (
      <Page>
        <Loading />
      </Page>
    );
  }
  if (catalog.error || installations.error) {
    return (
      <Page>
        <ErrorPanel error={catalog.error ?? installations.error} />
      </Page>
    );
  }

  const components = catalog.data.filter((c) => c.category === category);
  const versionsOf = (id: ComponentId) =>
    installations.data.filter((i) => i.component === id);
  const runningOf = (id: ComponentId) =>
    (services.data ?? []).filter(
      (s) => s.service.component === id && s.health === "running",
    ).length;
  const current =
    components.find((c) => c.id === picked) ??
    components.find((c) => versionsOf(c.id).length > 0) ??
    components[0];
  if (!current) {
    return (
      <Page>
        <PageHeader title={title} />
      </Page>
    );
  }
  const versions = versionsOf(current.id);
  const isService = current.integration === "service_managed";

  return (
    <div className="mx-auto flex h-full w-full max-w-7xl flex-col gap-5 px-8 pt-7 pb-6">
      <PageHeader
        title={title}
        actions={
          <Button
            variant="outline"
            onClick={() => scan.mutate()}
            disabled={scan.isPending}
          >
            <ScanSearchIcon />{" "}
            {scan.isPending ? "Scanning…" : "Scan this computer"}
          </Button>
        }
      />
      <ErrorPanel error={scan.error} />
      {scan.data && (
        <DiscoveryPanel
          results={scan.data}
          components={components}
          onClose={() => scan.reset()}
        />
      )}

      <div className="grid min-h-[26rem] flex-1 grid-cols-[17rem_minmax(0,1fr)] gap-5">
        <Card className="flex min-h-0 flex-col overflow-y-auto p-2">
          <nav aria-label={title} className="grid gap-1">
            {components.map((c) => {
              const list = versionsOf(c.id);
              const running = runningOf(c.id);
              const active = c.id === current.id;
              return (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => setPicked(c.id)}
                  aria-current={active ? "true" : undefined}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-lg border px-3 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    active
                      ? "border-primary/50 bg-primary/[0.07]"
                      : "border-transparent hover:bg-accent",
                  )}
                >
                  <ComponentLogo
                    component={c.id}
                    installedCount={list.length}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span
                        className={cn(
                          "truncate text-[15px] leading-tight",
                          active ? "font-semibold" : "font-medium",
                        )}
                      >
                        {c.name}
                      </span>
                      {running > 0 && (
                        <span
                          className="led-on size-1.5 shrink-0 rounded-full"
                          title={`${running} running`}
                        />
                      )}
                    </span>
                    {list.length > 0 ? (
                      <span className="mt-1 flex flex-wrap gap-1">
                        {list.map((v) => (
                          <span
                            key={v.id}
                            className={cn(
                              "rounded-sm border px-1.5 font-mono text-[11px] leading-4",
                              v.isDefault
                                ? "border-primary/40 text-primary"
                                : "text-muted-foreground",
                            )}
                          >
                            {v.version}
                          </span>
                        ))}
                      </span>
                    ) : (
                      <span className="mt-0.5 block text-xs text-muted-foreground">
                        Not installed
                      </span>
                    )}
                  </span>
                </button>
              );
            })}
          </nav>
        </Card>

        <Card className="flex min-h-0 flex-col gap-5 overflow-y-auto p-5">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex min-w-0 items-center gap-3">
              <ComponentLogo
                component={current.id}
                installedCount={versions.length}
                className="size-12"
              />
              <div className="min-w-0">
                <h2 className="flex items-center gap-2 font-display text-2xl font-semibold leading-tight">
                  {current.name}
                  {current.windowsSupport === "no_official_build" && (
                    <Badge variant="warning">unofficial Windows build</Badge>
                  )}
                </h2>
                <p className="mt-1 text-xs text-muted-foreground">
                  {current.distribution}
                </p>
              </div>
            </div>
            <div className="flex gap-2">
              {current.installable && (
                <Button onClick={() => setInstalling(true)}>
                  <DownloadIcon /> Install version
                </Button>
              )}
              <Button variant="outline" onClick={() => setRegistering(true)}>
                <PlusIcon /> Add from folder
              </Button>
            </div>
          </div>

          <ComponentInstalls component={current.id} name={current.name} />

          <div className="grid gap-2">
            <Label>
              {isService && category !== "language"
                ? "Versions & servers"
                : "Installed versions"}{" "}
              · {versions.length}
            </Label>
            {versions.length > 0 ? (
              isService && category !== "language" ? (
                <ServerVersionList
                  key={current.id}
                  category={category}
                  installations={versions}
                />
              ) : (
                <ComponentCard
                  info={current}
                  installations={versions}
                  runtimes={runtimes.data ?? []}
                  showHeader={false}
                />
              )
            ) : (
              <div className="grid justify-items-start gap-1.5 rounded-lg border border-dashed p-5">
                <p className="text-sm font-medium">
                  {current.name} is not installed yet
                </p>
                <p className="text-sm text-muted-foreground">
                  {current.installable
                    ? "Install a version from its official source, or add one that is already on this computer."
                    : "Add an installation that is already on this computer."}
                </p>
              </div>
            )}
          </div>

          <div className="mt-auto grid gap-1 border-t pt-3 text-xs text-muted-foreground">
            {current.notes.map((n) => (
              <p key={n}>{n}</p>
            ))}
            {category === "language" && versions.length > 0 && (
              <p>
                Each app picks its own version; the <Mono>default</Mono> one is
                used where none is chosen.
              </p>
            )}
          </div>
        </Card>
      </div>

      {installing && (
        <InstallDialog
          components={[current]}
          onOpenChange={(o) => !o && setInstalling(false)}
        />
      )}
      {registering && (
        <RegisterDialog
          open
          onOpenChange={(o) => !o && setRegistering(false)}
          components={components}
          initialComponent={current.id}
        />
      )}
    </div>
  );
}
