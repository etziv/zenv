import { ExternalLinkIcon, PlusIcon } from "lucide-react";

import { useNavigate } from "@/app/navigation";
import type { RouteId } from "@/app/routes";
import { ErrorPanel } from "@/components/error-panel";
import { Section } from "@/components/page";
import { Pulse } from "@/components/pulse";
import { Button } from "@/components/ui/button";
import { StatusSwitch } from "@/components/ui/status-switch";
import { api } from "@/lib/api/client";
import { useServiceAction } from "@/lib/api/queries";
import { isRouter, type Category, type ServiceStatus } from "@/lib/api/types";
import { sortServices } from "@/lib/services";
import { cn } from "@/lib/utils";

const GROUPS: { kind: Exclude<Category, "language">; label: string; route: RouteId; empty: string }[] = [
  { kind: "web_server", label: "Web servers", route: "web-servers", empty: "Add Apache or Nginx" },
  { kind: "database", label: "Databases", route: "databases", empty: "Add MySQL or PostgreSQL" },
  { kind: "cache", label: "Cache & queue", route: "cache", empty: "Add Redis" },
  { kind: "tool", label: "Tools", route: "tools", empty: "Add 9router or OmniRoute" },
];

/** Other web servers on the same port: they run together, the first one started holds the port. */
function sharesPortWith(status: ServiceStatus, all: ServiceStatus[]): string[] {
  return all
    .filter((o) => o.kind === "web_server" && o.service.id !== status.service.id && o.service.httpPort === status.service.httpPort)
    .map((o) => o.service.name);
}

function ServiceTile({ status, all }: { status: ServiceStatus; all: ServiceStatus[] }) {
  const action = useServiceAction();
  const { service, installation, health } = status;
  const running = health === "running";
  const pending = action.isPending;
  const sharing = status.kind === "web_server" ? sharesPortWith(status, all) : [];

  return (
    <li
      className={cn(
        "relative flex flex-col gap-2.5 overflow-hidden rounded-lg border bg-card p-3.5 shadow-[0_1px_2px_rgb(0_0_0/0.04)] transition-colors",
        running && "border-primary/35",
        health === "failed" && "border-destructive/40",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "absolute inset-y-0 left-0 w-[3px]",
          running ? "bg-primary" : health === "failed" ? "bg-destructive" : "bg-transparent",
        )}
      />
      <div className="flex items-center gap-2.5">
        <Pulse health={health} />
        <span className="min-w-0 flex-1 truncate text-sm font-semibold">{service.name}</span>
        <StatusSwitch
          checked={running}
          pending={pending}
          failed={health === "failed"}
          disabled={pending}
          onCheckedChange={(on) => action.mutate({ action: on ? "start" : "stop", id: service.id })}
          aria-label={running ? `Stop ${service.name}` : `Start ${service.name}`}
          title={!running && sharing.length > 0 ? `Shares :${service.httpPort} with ${sharing.join(", ")}: both run, each serving its own apps` : undefined}
        />
      </div>

      <div className="flex min-h-6 items-center gap-2 font-mono text-[11px] text-muted-foreground">
        <span className="truncate">
          {service.component}
          {installation ? ` ${installation.version}` : ""}
        </span>
        <span className="ml-auto shrink-0 text-foreground">
          :{service.httpPort}
          {status.kind === "web_server" && service.httpsPort && !status.behind ? ` · :${service.httpsPort}` : ""}
          {status.behind && <span className="text-muted-foreground"> via {status.behind}</span>}
        </span>
        {running && isRouter(service.component) && (
          <Button
            size="icon"
            variant="ghost"
            className="size-6"
            onClick={() => void api.routerOpen(service.id, null)}
            title={`Open the ${service.name} dashboard`}
            aria-label={`Open the ${service.name} dashboard`}
          >
            <ExternalLinkIcon className="size-3.5" />
          </Button>
        )}
        {running && status.kind === "web_server" && (
          <Button
            size="icon"
            variant="ghost"
            className="size-6"
            onClick={() => api.openServiceUrl(service.id)}
            title={`Open ${status.url}`}
            aria-label={`Open ${status.url}`}
          >
            <ExternalLinkIcon className="size-3.5" />
          </Button>
        )}
      </div>
      <ErrorPanel error={action.error} />
    </li>
  );
}

/** Every managed service as a switch, grouped by kind. */
export function ServiceBoard({ services }: { services: ServiceStatus[] }) {
  const navigate = useNavigate();
  return (
    <div className="flex flex-col gap-5">
      {GROUPS.map((g) => {
        const members = sortServices(services.filter((s) => s.kind === g.kind));
        return (
          <Section key={g.kind} title={g.label}>
            {members.length === 0 ? (
              <button
                type="button"
                onClick={() => navigate(g.route)}
                className="flex w-fit items-center gap-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <PlusIcon className="size-3.5" /> {g.empty}
              </button>
            ) : (
              <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
                {members.map((s) => (
                  <ServiceTile key={s.service.id} status={s} all={services} />
                ))}
              </ul>
            )}
          </Section>
        );
      })}
    </div>
  );
}
