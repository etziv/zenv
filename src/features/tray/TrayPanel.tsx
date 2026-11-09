import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  AppWindowIcon,
  LoaderCircleIcon,
  PlayIcon,
  PowerIcon,
  SquareIcon,
  XIcon,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef } from "react";

import { Pulse } from "@/components/pulse";
import { Button } from "@/components/ui/button";
import { StatusSwitch } from "@/components/ui/status-switch";
import { api } from "@/lib/api/client";
import {
  keys,
  useAppInfo,
  useServiceAction,
  useServices,
  useStartServicesForApps,
  useStopAllServices,
} from "@/lib/api/queries";
import type { ServiceStatus } from "@/lib/api/types";
import { sortServices } from "@/lib/services";
import { dismissToast, useToasts } from "@/lib/toast";
import { cn } from "@/lib/utils";

function Row({ status }: { status: ServiceStatus }) {
  const action = useServiceAction();
  const { service, installation, health } = status;
  const running = health === "running";
  return (
    <li
      className="flex h-8 items-center gap-2 rounded-md px-2 transition-colors hover:bg-muted/60"
      title={`${service.name}${installation ? ` ${installation.version}` : ""} · :${service.httpPort}${status.behind ? ` via ${status.behind}` : ""}`}
    >
      <Pulse health={health} className="size-2" />
      <span
        className={cn(
          "min-w-0 flex-1 truncate text-[12.5px] font-medium",
          !running && "text-muted-foreground",
        )}
      >
        {service.name}
      </span>
      <span className="w-14 shrink-0 truncate text-right font-mono text-[10.5px] text-muted-foreground">
        {status.behind ? `via ${status.behind}` : `:${service.httpPort}`}
      </span>
      <StatusSwitch
        checked={running}
        pending={action.isPending}
        failed={health === "failed"}
        disabled={action.isPending}
        onCheckedChange={(on) =>
          action.mutate({ action: on ? "start" : "stop", id: service.id })
        }
        aria-label={running ? `Stop ${service.name}` : `Start ${service.name}`}
      />
    </li>
  );
}

function Group({ label, items }: { label: string; items: ServiceStatus[] }) {
  if (items.length === 0) return null;
  return (
    <section className="grid gap-0.5">
      <p className="px-2 pb-0.5 pt-1.5 font-mono text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
        {label} · {items.length}
      </p>
      <ul className="grid gap-0.5">
        {items.map((s) => (
          <Row key={s.service.id} status={s} />
        ))}
      </ul>
    </section>
  );
}

/**
 * The panel that opens from the tray icon: every service with its switch,
 * running ones first, plus Start apps / Stop all. Hidden again when it loses
 * focus or on Escape.
 */
export function TrayPanel() {
  const qc = useQueryClient();
  const info = useAppInfo();
  const services = useServices();
  const start = useStartServicesForApps();
  const stop = useStopAllServices();
  const error = useToasts().findLast((t) => t.variant === "error");

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen(
      "zenv://panel-shown",
      () => void qc.invalidateQueries({ queryKey: keys.services }),
    )
      .then((un) => (cancelled ? un() : (unlisten = un)))
      .catch(() => undefined);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void api.panelHide();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      cancelled = true;
      unlisten?.();
      window.removeEventListener("keydown", onKey);
    };
  }, [qc]);

  const list = sortServices(services.data ?? []);
  // The window fits its content (up to a limit, then the list scrolls).
  const headerRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const fit = () => {
      const h =
        (headerRef.current?.offsetHeight ?? 0) +
        (bodyRef.current?.offsetHeight ?? 0) +
        (footerRef.current?.offsetHeight ?? 0);
      if (h > 0) void api.panelResize(Math.ceil(h)).catch(() => undefined);
    };
    fit();
    const observer = new ResizeObserver(fit);
    for (const el of [headerRef.current, bodyRef.current, footerRef.current])
      if (el) observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const running = list.filter((s) => s.health === "running");
  const others = list.filter((s) => s.health !== "running");

  return (
    <div className="flex h-screen select-none flex-col overflow-hidden bg-background text-foreground">
      <div ref={headerRef} className="shrink-0">
        <header className="flex h-10 items-center gap-2 bg-panel px-3 text-panel-foreground">
          <span className="font-display text-[16px] font-semibold leading-none tracking-tight text-white">
            zenv
          </span>
          {info.data && (
            <span className="mt-1 font-mono text-[10px] text-panel-muted">
              {info.data.version}
            </span>
          )}
          <span className="ml-auto flex items-center gap-1.5 font-mono text-[11px] text-panel-muted">
            <span
              className={cn(
                "size-1.5 rounded-full",
                running.length > 0 ? "led-on" : "led-off",
              )}
            />
            {running.length > 0 ? `${running.length} running` : "all stopped"}
          </span>
        </header>
        <div aria-hidden className="panel-trace h-px" />
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-width:thin]">
        <div ref={bodyRef} className="px-1 py-1">
          {services.isPending ? (
            <p className="flex items-center gap-2 px-2 py-3 text-xs text-muted-foreground">
              <LoaderCircleIcon className="size-4 animate-spin" /> Loading
              services…
            </p>
          ) : list.length === 0 ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              No services yet. Open Zenv to add a web server or database.
            </p>
          ) : (
            <>
              <Group label="Running" items={running} />
              <Group
                label={running.length > 0 ? "Stopped" : "Services"}
                items={others}
              />
            </>
          )}
        </div>
      </div>

      <div ref={footerRef} className="shrink-0">
        {error && (
          <div className="mx-2 mb-2 flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2.5 py-2 text-xs">
            <div className="min-w-0 flex-1">
              <p className="font-medium text-destructive">{error.title}</p>
              {error.description && (
                <p className="line-clamp-3 text-muted-foreground">
                  {error.description}
                </p>
              )}
            </div>
            <button
              type="button"
              aria-label="Dismiss"
              className="text-muted-foreground hover:text-foreground"
              onClick={() => dismissToast(error.id)}
            >
              <XIcon className="size-3.5" />
            </button>
          </div>
        )}

        <footer className="grid gap-1.5 border-t bg-card px-2.5 py-2">
          <div className="grid grid-cols-2 gap-1.5">
            <Button
              size="sm"
              className="h-7 text-xs"
              onClick={() => start.mutate()}
              disabled={start.isPending || list.length === 0}
            >
              {start.isPending ? (
                <LoaderCircleIcon className="animate-spin" />
              ) : (
                <PlayIcon />
              )}
              {start.isPending ? "Starting…" : "Start apps"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              onClick={() => stop.mutate()}
              disabled={stop.isPending || running.length === 0}
            >
              {stop.isPending ? (
                <LoaderCircleIcon className="animate-spin" />
              ) : (
                <SquareIcon />
              )}
              {stop.isPending ? "Stopping…" : "Stop all"}
            </Button>
          </div>
          <div className="flex items-center justify-between">
            <Button
              size="sm"
              variant="ghost"
              className="h-6 px-1.5 text-[11.5px]"
              onClick={() => void api.panelOpenMain()}
            >
              <AppWindowIcon /> Open Zenv
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 px-1.5 text-[11.5px] text-muted-foreground hover:text-destructive"
              onClick={() => void api.quit()}
              title="Stops every service and closes Zenv"
            >
              <PowerIcon /> Quit
            </Button>
          </div>
        </footer>
      </div>
    </div>
  );
}
