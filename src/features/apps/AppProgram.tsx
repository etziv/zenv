import { FileTextIcon, LoaderCircleIcon, PlayIcon, RotateCwIcon, SquareIcon, WandSparklesIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useAppProcess, useAppProcessAction } from "@/lib/api/queries";
import type { AppProcessStatus, SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { AppContainer } from "./AppContainer";

function describe(s: AppProcessStatus): { text: string; tone: "on" | "wait" | "off" | "fail" } {
  const port = s.port ? `:${s.port}` : "its port";
  switch (s.state) {
    case "running":
      return { text: `Running on ${port}`, tone: "on" };
    case "starting":
      return { text: `Starting… waiting for ${port}`, tone: "wait" };
    case "exited":
      return s.listening
        ? { text: `Answering on ${port} (started outside Zenv)`, tone: "on" }
        : { text: `Exited${s.exitCode !== null ? ` (code ${s.exitCode})` : ""}; see the output`, tone: "fail" };
    case "stopped":
      return s.listening
        ? { text: `Answering on ${port} (started outside Zenv)`, tone: "on" }
        : { text: `Not running; nothing answers on ${port}`, tone: "off" };
  }
}

/**
 * The program behind a proxy app (cargo run, npm run dev, ...): Zenv starts
 * it with the app's web server and on "Start apps".
 */
export function AppProgram({
  site,
  onShowOutput,
  onChangePort,
}: {
  site: SiteView;
  onShowOutput: () => void;
  /** Saves a new port for the app (resolves once saved). */
  onChangePort: (port: number) => Promise<unknown>;
}) {
  const status = useAppProcess(site.id);
  const action = useAppProcessAction();
  const [draft, setDraft] = useState<string | null>(null);
  const [moving, setMoving] = useState(false);
  const s = status.data;
  // A new app selected: forget the unsaved edit.
  useEffect(() => setDraft(null), [site.id]);

  if (!s) {
    return <p className="text-xs text-muted-foreground">Checking the program…</p>;
  }
  if (s.docker) {
    return <AppContainer site={site} status={s} docker={s.docker} onShowOutput={onShowOutput} />;
  }
  const { text, tone } = describe(s);
  const ownProcess = s.state === "running" || s.state === "starting";
  const value = draft ?? (s.detected ? "" : (s.command ?? ""));
  const dirty = draft !== null && draft.trim() !== (s.detected ? "" : (s.command ?? ""));
  const pending = action.isPending;
  const run = (a: "start" | "stop" | "restart") => action.mutate({ action: a, siteId: site.id, domain: site.domain });
  /** The best free port for this kind of project; the program restarts on it. */
  const bestPort = async () => {
    setMoving(true);
    try {
      const port = await api.appPortSuggest(site.projectType, site.id);
      if (port !== site.upstreamPort) {
        const wasRunning = s?.state === "running" || s?.state === "starting";
        await onChangePort(port);
        if (wasRunning) run("restart");
      }
    } finally {
      setMoving(false);
    }
  };
  const save = () => {
    action.mutate(
      { action: "save", siteId: site.id, domain: site.domain, command: value.trim() || null, autostart: s.autostart },
      { onSuccess: () => setDraft(null) },
    );
  };

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
        <Button size="icon" variant="ghost" className="size-7" title="Show output" aria-label="Show output" onClick={onShowOutput}>
          <FileTextIcon />
        </Button>
      </div>

      <div className="grid gap-1">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-muted-foreground">Port</span>
          <Button size="sm" variant="ghost" className="h-6 px-1.5 text-[11px]" disabled={moving || pending} onClick={() => void bestPort()}>
            {moving ? <LoaderCircleIcon className="animate-spin" /> : <WandSparklesIcon />} Best free port
          </Button>
        </div>
        <p className="font-mono text-xs">
          :{s.port}
          {s.portVars.length > 0 && <span className="text-muted-foreground"> → {s.portVars.join(", ")}</span>}
        </p>
        <p className="text-[11px] text-muted-foreground">Zenv's port, on this computer only; everything else comes from the project's .env.</p>
        {s.exposed && (
          <p className="text-[11px] text-warning">
            The program listens on every network, so other devices can reach :{s.port} directly. Make it listen on 127.0.0.1 (Zenv
            passes HOST=127.0.0.1).
          </p>
        )}
      </div>

      <form
        className="grid gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          if (dirty) save();
        }}
      >
        <label htmlFor={`cmd-${site.id}`} className="text-[11px] text-muted-foreground">
          Start command
        </label>
        <div className="flex gap-1.5">
          <Input
            id={`cmd-${site.id}`}
            value={value}
            placeholder={s.detected && s.command ? `${s.command} (detected)` : "e.g. cargo run, npm run dev"}
            onChange={(e) => setDraft(e.target.value)}
            className="h-8 min-w-0 flex-1 font-mono text-xs"
            spellCheck={false}
          />
          {dirty && (
            <Button type="submit" size="sm" className="h-8" disabled={pending}>
              Save
            </Button>
          )}
        </div>
        <p className="text-[11px] text-muted-foreground">Runs in the project folder with your default language versions.</p>
        {s.detected && s.command && (
          <p className="text-[11px] text-warning">
            Detected from the project and not run yet. Zenv runs it once you press Start (or save it); check that you trust this
            project first{s.runs ? <>: it runs <code className="font-mono">{s.runs}</code></> : "."}
          </p>
        )}
      </form>

      <div className="flex items-center justify-between gap-3">
        <label htmlFor={`auto-${site.id}`} className="text-sm">
          Start with the web server
        </label>
        <Switch
          id={`auto-${site.id}`}
          checked={s.autostart}
          disabled={pending}
          onCheckedChange={(on) =>
            // Switching it on approves a detected command.
            action.mutate({ action: "save", siteId: site.id, domain: site.domain, command: s.detected && !on ? null : s.command, autostart: on })
          }
        />
      </div>

      <div className="flex gap-1.5">
        {ownProcess ? (
          <>
            <Button size="sm" variant="outline" className="flex-1" disabled={pending} onClick={() => run("restart")}>
              {pending ? <LoaderCircleIcon className="animate-spin" /> : <RotateCwIcon />} Restart
            </Button>
            <Button size="sm" variant="outline" className="flex-1" disabled={pending} onClick={() => run("stop")}>
              <SquareIcon /> Stop
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            className="flex-1"
            disabled={pending || !s.command || s.listening || !site.enabled}
            title={s.listening ? "Something already answers on this port" : s.runs ? `Runs: ${s.runs}` : undefined}
            onClick={() => run("start")}
          >
            {pending ? <LoaderCircleIcon className="animate-spin" /> : <PlayIcon />} Start {s.command ? <code className="font-mono text-[11px] opacity-80">{s.command}</code> : null}
          </Button>
        )}
      </div>
    </div>
  );
}
