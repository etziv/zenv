import { ErrorPanel } from "@/components/error-panel";
import { Loading } from "@/components/page";
import { Pulse } from "@/components/pulse";
import { Card } from "@/components/ui/card";
import { api } from "@/lib/api/client";
import { useLogSources, useRuntimeLogGroups, useServices } from "@/lib/api/queries";
import type { Category, RuntimeLogGroup, ServiceStatus } from "@/lib/api/types";

import { ListGroup, ListItem, LogPanel } from "./LogPanel";

export type ServiceLogTarget = { type: "app" } | { type: "service"; id: string } | { type: "runtime"; id: string };

const GROUPS: { kind: Category; label: string }[] = [
  { kind: "web_server", label: "Web servers" },
  { kind: "database", label: "Databases" },
  { kind: "cache", label: "Cache & queue" },
  { kind: "tool", label: "Tools" },
];

function ServicePanel({ status }: { status: ServiceStatus }) {
  const sources = useLogSources(status.service.id);
  if (sources.isPending) return <Loading />;
  if (sources.error) return <ErrorPanel error={sources.error} />;
  const id = status.service.id;
  return (
    <LogPanel
      title={status.service.name}
      subtitle={`${status.installation ? `${status.service.component} ${status.installation.version} · ` : ""}${status.service.listenAddress}:${status.service.httpPort}`}
      sources={sources.data}
      queryKey={["service", id]}
      read={(log) => api.serviceLog(id, log)}
      reveal={(log) => ({ kind: "serviceLog", id, log })}
      emptyHint="This service has not written any logs yet."
    />
  );
}

function RuntimePanel({ group }: { group: RuntimeLogGroup }) {
  return (
    <LogPanel
      title={group.label}
      subtitle="Shared by every site on this version; each project's own PHP errors are under Projects."
      sources={group.sources}
      queryKey={["runtime", group.installationId]}
      read={(log) => api.runtimeLog(group.installationId, log)}
      reveal={(log) => ({ kind: "runtimeLog", id: group.installationId, log })}
    />
  );
}

function AppPanel() {
  return (
    <LogPanel
      title="Zenv"
      subtitle="What Zenv itself did: starts, stops, configuration changes, errors."
      sources={[{ id: "app", label: "Zenv log", exists: true }]}
      queryKey={["app"]}
      read={() => api.appLog()}
      reveal={() => ({ kind: "appLog" })}
    />
  );
}

export function ServiceLogs({ target, onSelect }: { target: ServiceLogTarget; onSelect: (t: ServiceLogTarget) => void }) {
  const services = useServices();
  const runtimes = useRuntimeLogGroups();
  const list = services.data ?? [];
  const groups = runtimes.data ?? [];
  const selectedService = target.type === "service" ? list.find((s) => s.service.id === target.id) : undefined;
  const selectedRuntime = target.type === "runtime" ? groups.find((g) => g.installationId === target.id) : undefined;

  return (
    <div className="grid grid-cols-[15rem_1fr] items-start gap-4">
      <Card className="max-h-[calc(100vh-12rem)] overflow-y-auto py-1">
        <ListGroup label="Zenv">
          <ListItem active={target.type === "app"} onClick={() => onSelect({ type: "app" })}>
            Zenv log
          </ListItem>
        </ListGroup>
        {GROUPS.map(({ kind, label }) => {
          const members = list.filter((s) => s.kind === kind);
          if (members.length === 0) return null;
          return (
            <ListGroup key={kind} label={label}>
              {members.map((s) => (
                <ListItem
                  key={s.service.id}
                  active={target.type === "service" && target.id === s.service.id}
                  onClick={() => onSelect({ type: "service", id: s.service.id })}
                >
                  <Pulse health={s.health} />
                  <span className="min-w-0 flex-1 truncate">{s.service.name}</span>
                  <span className="font-mono text-[10.5px] text-muted-foreground">:{s.service.httpPort}</span>
                </ListItem>
              ))}
            </ListGroup>
          );
        })}
        {groups.length > 0 && (
          <ListGroup label="PHP runtimes">
            {groups.map((g) => (
              <ListItem
                key={g.installationId}
                active={target.type === "runtime" && target.id === g.installationId}
                onClick={() => onSelect({ type: "runtime", id: g.installationId })}
              >
                <span className="min-w-0 flex-1 truncate">{g.label}</span>
              </ListItem>
            ))}
          </ListGroup>
        )}
        {services.error && <ErrorPanel error={services.error} />}
      </Card>

      {selectedService ? (
        <ServicePanel key={selectedService.service.id} status={selectedService} />
      ) : selectedRuntime ? (
        <RuntimePanel key={selectedRuntime.installationId} group={selectedRuntime} />
      ) : (
        <AppPanel />
      )}
    </div>
  );
}
