import { useNavigate } from "@/app/navigation";
import { ComponentLogo } from "@/components/component-logo";
import { cn } from "@/lib/utils";
import { ErrorPanel } from "@/components/error-panel";
import { Loading, Page, PageHeader, Section } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { LoaderCircleIcon, PlayIcon, SquareIcon } from "lucide-react";

import {
  useCatalog,
  useDiagnostics,
  useInstallations,
  useServices,
  useServicesForApps,
  useStartServicesForApps,
  useStopAllServices,
} from "@/lib/api/queries";

import { ServiceBoard } from "./ServiceBoard";

/** Starts every service the enabled apps use (web server, databases, caches). */
function StartAppsButton() {
  const needed = useServicesForApps();
  const start = useStartServicesForApps();
  const list = needed.data ?? [];
  const stopped = list.filter((n) => !n.running);
  /** Distinct apps behind a service ("shop.test (DB_PORT=3307)" → "shop.test"). */
  const apps = (usedBy: string[]) =>
    new Set(usedBy.map((u) => u.split(" ")[0])).size;

  return (
    <div className="group relative">
      <Button
        onClick={() => start.mutate()}
        disabled={start.isPending || list.length === 0}
      >
        {start.isPending ? (
          <LoaderCircleIcon className="animate-spin" />
        ) : (
          <PlayIcon />
        )}
        {start.isPending
          ? "Starting…"
          : stopped.length > 0
            ? `Start apps (${stopped.length})`
            : "Apps running"}
      </Button>
      {list.length > 0 && (
        <div
          role="tooltip"
          className="pointer-events-none invisible absolute top-full right-0 z-40 mt-2 w-64 rounded-lg border bg-popover p-2 text-popover-foreground opacity-0 shadow-lg transition-opacity group-focus-within:visible group-focus-within:opacity-100 group-hover:visible group-hover:opacity-100"
        >
          <p className="px-1.5 pb-1.5 text-[11px] text-muted-foreground">
            Used by your apps
          </p>
          <ul className="grid gap-0.5">
            {list.map((n) => (
              <li
                key={n.serviceId}
                className="flex items-center gap-2 rounded-md px-1.5 py-1 text-xs"
              >
                <span
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    n.running ? "led-on" : "border border-muted-foreground/60",
                  )}
                />
                <span className="min-w-0 flex-1 truncate font-medium">
                  {n.name}
                </span>
                <span className="shrink-0 font-mono text-[10.5px] text-muted-foreground">
                  {apps(n.usedBy)} app{apps(n.usedBy) === 1 ? "" : "s"}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** Stops every running service; shown only while something runs. */
function StopAllButton({ running }: { running: number }) {
  const stop = useStopAllServices();
  if (running === 0 && !stop.isPending) return null;
  return (
    <Button
      variant="outline"
      onClick={() => stop.mutate()}
      disabled={stop.isPending}
      title="Stop every running service"
    >
      {stop.isPending ? (
        <LoaderCircleIcon className="animate-spin" />
      ) : (
        <SquareIcon />
      )}
      {stop.isPending ? "Stopping…" : `Stop all (${running})`}
    </Button>
  );
}

export function DashboardPage() {
  const navigate = useNavigate();
  const services = useServices();
  const installations = useInstallations();
  const catalog = useCatalog();
  const diagnostics = useDiagnostics();

  const error = services.error ?? installations.error ?? catalog.error;
  if (error) {
    return (
      <Page>
        <ErrorPanel error={error} />
      </Page>
    );
  }
  if (!services.data || !installations.data || !catalog.data) {
    return (
      <Page>
        <Loading />
      </Page>
    );
  }
  const installed = installations.data;
  const components = catalog.data;
  const problems = (diagnostics.data ?? []).filter((c) => c.status !== "ok");
  const languages = components.filter((c) => c.category === "language");
  const running = services.data.filter((s) => s.health === "running").length;

  return (
    <Page>
      <PageHeader
        title="Dashboard"
        actions={
          <div className="flex items-center gap-2">
            <StopAllButton running={running} />
            <StartAppsButton />
          </div>
        }
        meta={
          services.data.length === 0
            ? "No services yet"
            : `${running} of ${services.data.length} services running`
        }
      />

      <ServiceBoard services={services.data} />

      <Section title="Languages">
        <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5">
          {languages.map((lang) => {
            const versions = installed
              .filter((i) => i.component === lang.id)
              .sort((x, y) =>
                y.version.localeCompare(x.version, undefined, {
                  numeric: true,
                }),
              );
            return (
              <button
                key={lang.id}
                type="button"
                onClick={() => navigate("languages")}
                className="flex flex-col gap-2 rounded-lg border bg-card p-3 text-left transition-colors hover:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span className="flex items-center gap-2.5">
                  <ComponentLogo
                    component={lang.id}
                    installedCount={versions.length}
                    className="size-8 text-[11px]"
                  />
                  <span className="font-display text-base font-semibold leading-none">
                    {lang.name}
                  </span>
                </span>
                {versions.length === 0 ? (
                  <span className="text-xs text-muted-foreground">
                    Not installed · add one
                  </span>
                ) : (
                  <span className="flex flex-wrap gap-1">
                    {versions.map((v) => (
                      <span
                        key={v.id}
                        title={`${v.rootPath}${v.isDefault ? " (default)" : ""}`}
                        className={
                          v.isDefault
                            ? "rounded-sm border border-primary/40 bg-primary/10 px-1.5 py-px font-mono text-[11px] text-primary"
                            : "rounded-sm border px-1.5 py-px font-mono text-[11px] text-muted-foreground"
                        }
                      >
                        {v.version}
                      </span>
                    ))}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </Section>

      <Section title="Health">
        {diagnostics.isPending ? (
          <Loading label="Running checks…" />
        ) : diagnostics.error ? (
          <ErrorPanel error={diagnostics.error} />
        ) : (
          <Card className="flex items-center gap-3 p-4">
            <p className="text-sm">
              {problems.length === 0
                ? `All ${diagnostics.data.length} checks passed.`
                : `${problems.length} of ${diagnostics.data.length} checks need attention.`}
            </p>
            <Button
              size="sm"
              variant="outline"
              className="ml-auto"
              onClick={() => navigate("logs")}
            >
              View diagnostics
            </Button>
          </Card>
        )}
      </Section>
    </Page>
  );
}
