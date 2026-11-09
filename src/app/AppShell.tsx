import { MonitorIcon, MoonIcon, SettingsIcon, SunIcon } from "lucide-react";
import type * as React from "react";
import { useEffect, useRef } from "react";

import { Toaster } from "@/components/toaster";
import { toast } from "@/lib/toast";

import { useAppInfo, useServices } from "@/lib/api/queries";
import type { Category, ComponentId, ServiceStatus } from "@/lib/api/types";
import { nextTheme, setTheme, type Theme, useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

const THEME_LABEL: Record<Theme, string> = { system: "Theme: follow Windows", light: "Theme: light", dark: "Theme: dark" };

function ThemeButton() {
  const theme = useTheme();
  const Icon = theme === "light" ? SunIcon : theme === "dark" ? MoonIcon : MonitorIcon;
  return (
    <button
      type="button"
      onClick={() => setTheme(nextTheme(theme))}
      aria-label={`${THEME_LABEL[theme]} (click to change)`}
      title={`${THEME_LABEL[theme]} (click to change)`}
      className="grid size-8 place-items-center rounded-md text-panel-muted transition-colors hover:bg-panel-raised hover:text-panel-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led"
    >
      <Icon className="size-4" />
    </button>
  );
}

import { InstallIndicator, useInstallNotices } from "@/features/runtimes/install-progress";
import { useCatalog } from "@/lib/api/queries";

import { NAV_GROUPS, type RouteId } from "./routes";

const PAGE_FOR: Record<Category, RouteId> = {
  web_server: "web-servers",
  database: "databases",
  cache: "cache",
  tool: "tools",
  language: "languages",
};

const RAIL_NAME: Partial<Record<ComponentId, string>> = { postgresql: "postgres", ninerouter: "9router" };

/** Live ports, one LED per running (or crashed) service: a strip under the menu bar. */
function PortRail({ onNavigate }: { onNavigate: (route: RouteId) => void }) {
  const services = useServices();
  if (!services.data) return null;
  const live = services.data.filter((s) => s.health !== "stopped");
  return (
    <div className="flex h-8 shrink-0 items-center gap-2 overflow-x-auto bg-panel px-4 [scrollbar-width:none]">
      {live.length === 0 ? (
        <span className="flex items-center gap-2 font-mono text-[11px] text-panel-muted">
          <span className="led-off size-1.5 rounded-full" />
          {services.data.length === 0 ? "no services" : "all services stopped"}
        </span>
      ) : (
        <ul className="flex items-center gap-1" aria-label="Running services">
          {live.map((s) => (
            <li key={s.service.id} className="shrink-0">
              <RailChip status={s} onClick={() => onNavigate(PAGE_FOR[s.kind])} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function RailChip({ status, onClick }: { status: ServiceStatus; onClick: () => void }) {
  const failed = status.health === "failed";
  const { service, installation } = status;
  const label = `${service.name}${installation ? ` ${installation.version}` : ""} · ${service.listenAddress}:${service.httpPort}${failed ? " · exited unexpectedly" : ""}`;
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="flex h-6 items-center gap-1.5 rounded-sm border border-panel-line bg-panel-raised px-1.5 font-mono text-[11px] text-panel-foreground transition-colors hover:border-panel-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led"
    >
      <span className={cn("size-1.5 rounded-full", failed ? "led-fail" : "led-on")} />
      <span className="text-panel-muted">{RAIL_NAME[service.component] ?? service.component}</span>
      <span className={failed ? "text-led-fail" : undefined}>:{service.httpPort}</span>
    </button>
  );
}

function NavTab({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative flex shrink-0 items-center px-2.5 text-[13px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-led",
        active ? "text-white" : "text-panel-muted hover:text-panel-foreground",
      )}
    >
      {children}
      <span
        className={cn(
          "absolute inset-x-2 bottom-0 h-[2px] rounded-t-full bg-led transition-opacity",
          active ? "opacity-100" : "opacity-0",
        )}
      />
    </button>
  );
}

/** Tells the user when a service that was running stops by itself. */
function useCrashNotices(onNavigate: (route: RouteId) => void) {
  const services = useServices();
  const last = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    if (!services.data) return;
    const seen = last.current;
    for (const s of services.data) {
      const before = seen.get(s.service.id);
      if (before === "running" && s.health === "failed") {
        toast({
          variant: "error",
          title: `${s.service.name} stopped unexpectedly`,
          description: "It exited without Zenv asking it to. Its log usually says why.",
          action: { label: "View logs", onClick: () => onNavigate("logs") },
        });
      }
      seen.set(s.service.id, s.health);
    }
  }, [services.data, onNavigate]);
}

export function AppShell({
  route,
  onNavigate,
  children,
}: {
  route: RouteId;
  onNavigate: (route: RouteId) => void;
  children: React.ReactNode;
}) {
  const info = useAppInfo();
  useCrashNotices(onNavigate);
  useInstallNotices();
  const catalog = useCatalog();
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-stretch gap-2 bg-panel pl-4 pr-2 text-panel-foreground">
        <button
          type="button"
          onClick={() => onNavigate("dashboard")}
          className="mr-2 flex items-center gap-2 text-panel-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led"
          aria-label="Zenv dashboard"
        >
          <span className="font-display text-[19px] font-semibold leading-none tracking-tight text-white">zenv</span>
          {info.data && <span className="mt-1 font-mono text-[10px] text-panel-muted">{info.data.version}</span>}
        </button>

        <nav aria-label="Main" className="flex min-w-0 items-stretch overflow-x-auto [scrollbar-width:none]">
          {NAV_GROUPS.map((group, gi) => (
            <div key={group.label} role="group" aria-label={group.label} className="flex shrink-0 items-stretch">
              {gi > 0 && <span aria-hidden className="mx-1 my-4 w-px shrink-0 bg-panel-line" />}
              {group.routes.map((r) => (
                <NavTab key={r.id} active={r.id === route} onClick={() => onNavigate(r.id)}>
                  {r.label}
                </NavTab>
              ))}
            </div>
          ))}
        </nav>

        <div className="ml-auto flex shrink-0 items-center gap-2">
          <InstallIndicator
            onOpen={(component) => {
              const category = catalog.data?.find((c) => c.id === component)?.category;
              onNavigate(category ? PAGE_FOR[category] : "languages");
            }}
          />
          <ThemeButton />
          <button
            type="button"
            onClick={() => onNavigate("settings")}
            aria-label="Settings"
            aria-current={route === "settings" ? "page" : undefined}
            title="Settings"
            className={cn(
              "grid size-8 place-items-center rounded-md transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-led",
              route === "settings" ? "bg-panel-raised text-led" : "text-panel-muted hover:bg-panel-raised hover:text-panel-foreground",
            )}
          >
            <SettingsIcon className="size-4" />
          </button>
        </div>
      </header>
      <div aria-hidden className="h-px shrink-0 bg-panel-line" />
      <PortRail onNavigate={onNavigate} />
      <div aria-hidden className="panel-trace h-px shrink-0" />

      <main className="dot-grid min-h-0 flex-1 overflow-y-auto">{children}</main>
      <Toaster />
    </div>
  );
}
