import { BoxIcon, FileTextIcon, LoaderCircleIcon, PlayIcon, RotateCwIcon, SquareIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useAppProcessAction, useDockerEngineStart } from "@/lib/api/queries";
import type { AppProcessStatus, DockerAppStatus, SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

function describe(s: AppProcessStatus, d: DockerAppStatus): { text: string; tone: "on" | "wait" | "off" | "fail" } {
  const port = s.port ? `:${s.port}` : "its port";
  if (!d.engineRunning) return { text: "Docker Desktop is not running", tone: "off" };
  if (d.state === "missing") return { text: `Container ${d.container} no longer exists`, tone: "fail" };
  switch (s.state) {
    case "running":
      return { text: `Running on ${port}`, tone: "on" };
    case "starting":
      return { text: `Container up, waiting for ${port}`, tone: "wait" };
    case "exited":
      return { text: `Container exited${s.exitCode !== null ? ` (code ${s.exitCode})` : ""}; see the output`, tone: "off" };
    case "stopped":
      return { text: `Container ${d.state}`, tone: "off" };
  }
}

/**
 * The container behind a Docker app: Zenv starts it (with docker compose for
 * compose services) with the web server, on "Start apps" or here.
 */
export function AppContainer({ site, status: s, docker: d, onShowOutput }: { site: SiteView; status: AppProcessStatus; docker: DockerAppStatus; onShowOutput: () => void }) {
  const action = useAppProcessAction();
  const engine = useDockerEngineStart();
  const pending = action.isPending || engine.isPending;
  const run = (a: "start" | "stop" | "restart") => action.mutate({ action: a, siteId: site.id, domain: site.domain });
  const { text, tone } = describe(s, d);
  const up = s.state === "running" || s.state === "starting";

  return (
    <div className="grid gap-2.5">
      <div className="flex min-w-0 items-center gap-2">
        <span
          className={cn(
            "size-1.5 shrink-0 rounded-full",
            tone === "on" && "led-on",
            tone === "wait" && "animate-pulse bg-warning",
            tone === "fail" && "led-fail",
            tone === "off" && "led-off",
          )}
        />
        <span className={cn("min-w-0 flex-1 truncate text-xs", tone === "fail" && "text-destructive")}>{text}</span>
        <Button size="icon" variant="ghost" className="size-7" title="Show the container's output" aria-label="Show output" onClick={onShowOutput}>
          <FileTextIcon />
        </Button>
      </div>

      <dl className="grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1 text-xs">
        <dt className="text-muted-foreground">Container</dt>
        <dd className="flex min-w-0 items-center gap-1.5 font-mono">
          <BoxIcon className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate">{d.container}</span>
        </dd>
        {d.compose && (
          <>
            <dt className="text-muted-foreground">Compose</dt>
            <dd className="truncate font-mono" title={d.compose.dir ?? undefined}>
              {d.compose.project}/{d.compose.service}
            </dd>
          </>
        )}
        {d.image && (
          <>
            <dt className="text-muted-foreground">Image</dt>
            <dd className="truncate font-mono">{d.image}</dd>
          </>
        )}
        <dt className="text-muted-foreground">Port</dt>
        <dd className="font-mono">:{s.port}</dd>
      </dl>
      {d.problem && d.engineRunning && <p className="text-[11px] text-destructive">{d.problem}</p>}
      {s.exposed && (
        <p className="text-[11px] text-warning">
          Docker publishes :{s.port} on every network, so other devices can reach the container directly. Publish it as
          127.0.0.1:{s.port}:&lt;port&gt; to keep it on this computer.
        </p>
      )}

      <div className="flex items-center justify-between gap-3">
        <label htmlFor={`auto-${site.id}`} className="text-sm">
          Start with the web server
        </label>
        <Switch
          id={`auto-${site.id}`}
          checked={s.autostart}
          disabled={pending}
          onCheckedChange={(on) => action.mutate({ action: "save", siteId: site.id, domain: site.domain, command: null, autostart: on })}
        />
      </div>

      <div className="flex gap-1.5">
        {!d.engineRunning ? (
          <Button size="sm" className="flex-1" disabled={pending} onClick={() => engine.mutate()}>
            {engine.isPending ? <LoaderCircleIcon className="animate-spin" /> : <BoxIcon />}
            {engine.isPending ? "Starting Docker Desktop…" : "Start Docker Desktop"}
          </Button>
        ) : up ? (
          <>
            <Button size="sm" variant="outline" className="flex-1" disabled={pending} onClick={() => run("restart")}>
              {pending ? <LoaderCircleIcon className="animate-spin" /> : <RotateCwIcon />} Restart
            </Button>
            <Button size="sm" variant="outline" className="flex-1" disabled={pending} onClick={() => run("stop")}>
              <SquareIcon /> Stop
            </Button>
          </>
        ) : (
          <Button size="sm" className="flex-1" disabled={pending || d.state === "missing" || !site.enabled} onClick={() => run("start")}>
            {pending ? <LoaderCircleIcon className="animate-spin" /> : <PlayIcon />} Start container
          </Button>
        )}
      </div>
      {engine.error && <p className="text-[11px] text-destructive">{engine.error.message}</p>}
    </div>
  );
}
