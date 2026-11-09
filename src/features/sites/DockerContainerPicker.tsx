import { BoxIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useDockerContainers, useDockerEngineStart } from "@/lib/api/queries";
import type { DockerContainer, PublishedPort } from "@/lib/api/types";
import { cn } from "@/lib/utils";

export interface PickedContainer {
  container: DockerContainer;
  port: PublishedPort;
}

const portKey = (p: PublishedPort) => `${p.hostPort}:${p.containerPort}`;

/**
 * Step 1 for Docker apps: the containers on this computer (running first);
 * one with a published port, and which port when it has several.
 */
export function DockerContainerPicker({ picked, onPick }: { picked: PickedContainer | null; onPick: (p: PickedContainer) => void }) {
  const list = useDockerContainers(true);
  const engine = useDockerEngineStart();
  const notRunning = list.error?.message.includes("Docker Desktop is not running");

  if (list.isLoading) {
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <LoaderCircleIcon className="size-4 animate-spin" /> Asking Docker for its containers…
      </p>
    );
  }
  if (list.error) {
    return (
      <div className="grid gap-2 rounded-md border border-warning/40 bg-warning/5 p-3 text-sm">
        <p>{list.error.message}</p>
        {notRunning && (
          <div className="flex items-center gap-2">
            <Button type="button" size="sm" disabled={engine.isPending} onClick={() => engine.mutate()}>
              {engine.isPending ? <LoaderCircleIcon className="animate-spin" /> : <BoxIcon />}
              {engine.isPending ? "Starting Docker Desktop…" : "Start Docker Desktop"}
            </Button>
            {engine.error && <span className="text-xs text-destructive">{engine.error.message}</span>}
          </div>
        )}
      </div>
    );
  }
  const containers = list.data ?? [];
  if (containers.length === 0) {
    return <p className="text-sm text-muted-foreground">Docker has no containers yet. Create one (docker run or docker compose up), then reload.</p>;
  }
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>Zenv proxies a .test domain to the port the container publishes on this computer.</span>
        <Button type="button" size="icon" variant="ghost" className="size-7" title="Reload" disabled={list.isFetching} onClick={() => void list.refetch()}>
          {list.isFetching ? <LoaderCircleIcon className="animate-spin" /> : <RefreshCwIcon />}
        </Button>
      </div>
      <ul className="grid max-h-80 gap-1.5 overflow-y-auto pr-1" aria-label="Docker containers">
        {containers.map((c) => {
          const usable = c.ports.length > 0;
          const selected = picked?.container.name === c.name;
          return (
            <li key={c.name}>
              <div
                className={cn(
                  "grid gap-1.5 rounded-md border p-2.5 text-sm",
                  selected ? "border-primary bg-primary/5" : usable ? "hover:bg-accent" : "opacity-60",
                )}
              >
                <button
                  type="button"
                  disabled={!usable}
                  onClick={() => c.ports[0] && onPick({ container: c, port: c.ports[0] })}
                  className="flex min-w-0 items-center gap-2 text-left disabled:cursor-not-allowed"
                >
                  <span className={cn("size-1.5 shrink-0 rounded-full", c.state === "running" ? "led-on" : "led-off")} />
                  <span className="min-w-0 flex-1 truncate font-medium">{c.name}</span>
                  {c.compose && (
                    <span className="shrink-0 rounded-sm border px-1 font-mono text-[10px] text-muted-foreground" title={c.compose.dir ?? undefined}>
                      {c.compose.project}/{c.compose.service}
                    </span>
                  )}
                </button>
                <p className="truncate pl-3.5 font-mono text-[11px] text-muted-foreground">
                  {c.image} · {c.status}
                </p>
                {usable ? (
                  <div className="flex flex-wrap gap-1 pl-3.5">
                    {c.ports.map((p) => (
                      <button
                        key={portKey(p)}
                        type="button"
                        onClick={() => onPick({ container: c, port: p })}
                        className={cn(
                          "rounded-sm border px-1.5 py-0.5 font-mono text-[11px]",
                          selected && picked && portKey(picked.port) === portKey(p) ? "border-primary bg-primary/10" : "hover:bg-accent",
                        )}
                        title={`This computer's port ${p.hostPort} → the container's ${p.containerPort}`}
                      >
                        :{p.hostPort} → {p.containerPort}
                      </button>
                    ))}
                  </div>
                ) : (
                  <p className="pl-3.5 text-[11px] text-muted-foreground">
                    Publishes no port (add <span className="font-mono">-p 8080:80</span> or <span className="font-mono">ports:</span> in compose), so Windows can't reach it.
                  </p>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
