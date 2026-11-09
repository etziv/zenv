import {
  ExternalLinkIcon,
  FolderOpenIcon,
  MoreHorizontalIcon,
  TriangleAlertIcon,
} from "lucide-react";
import type * as React from "react";
import { Fragment, useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { NativeSelect } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useSaveSite, useSiteAction, useTlsStatus } from "@/lib/api/queries";
import type {
  Installation,
  PhpEngine,
  ServiceStatus,
  SiteInput,
  SiteView,
} from "@/lib/api/types";

import {
  BACKEND_LABEL,
  PROJECT_TYPE_LABEL,
  STRATEGY_LABEL,
  hostsProblem,
  statusBadge,
} from "../sites/labels";
import { resolveBackend } from "../sites/php";
import { type PanelAction, useAppActions } from "./app-actions";
import { previewState, usePreviewQueue } from "./previews";
import { AppProgram } from "./AppProgram";
import { EditButtons } from "./edit-buttons";
import { TailnetPanel } from "./TailnetPanel";

export type { PanelAction } from "./app-actions";

/** The site as it is, as an input to change one setting at a time. */
export function toInput(
  site: SiteView,
  patch: Partial<SiteInput> = {},
): SiteInput {
  return {
    name: site.name,
    domain: site.domain,
    projectRoot: site.projectRoot,
    documentRoot: site.documentRoot,
    projectType: site.projectType,
    serviceId: site.serviceId,
    strategy: site.strategy,
    upstreamPort: site.upstreamPort,
    runtimeInstallationId: site.runtimeInstallationId,
    phpEngine: site.phpEngine,
    httpsEnabled: site.httpsEnabled,
    httpsRedirect: site.httpsRedirect,
    enabled: site.enabled,
    ...patch,
  };
}

function Group({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <section className="grid min-w-0 gap-2.5 border-t pt-3.5">
      <p className="font-mono text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </p>
      {children}
    </section>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="grid gap-1">
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span className="min-w-0 truncate text-sm">{label}</span>
        <div className="flex min-w-0 shrink-0 items-center justify-end">
          {children}
        </div>
      </div>
      {hint && <p className="text-[11px] text-muted-foreground">{hint}</p>}
    </div>
  );
}

const ENGINE_LABEL: Record<PhpEngine, string> = {
  auto: "Automatic",
  fastcgi: "PHP-CGI",
  frankenphp: "FrankenPHP",
};

export function AppPanel({
  site,
  webServers,
  phpVersions,
  frankens,
  onAction,
}: {
  site: SiteView;
  webServers: ServiceStatus[];
  phpVersions: Installation[];
  frankens: Installation[];
  onAction: (action: PanelAction) => void;
}) {
  const save = useSaveSite();
  const action = useSiteAction();
  const tls = useTlsStatus();
  const [error, setError] = useState<unknown>(null);
  usePreviewQueue();
  const menu = useAppActions(site, onAction);
  const preview = previewState(site.id);

  const change = (patch: Partial<SiteInput>) =>
    save.mutate({ id: site.id, input: toInput(site, patch) });
  const busy = save.isPending || action.isPending;
  const status = statusBadge(site.configStatus);
  const hosts = site.enabled ? hostsProblem(site.hostsState) : null;
  const php = phpVersions.find((p) => p.id === site.runtimeInstallationId);
  const servedBy = site.serviceId
    ? webServers.find((s) => s.service.id === site.serviceId)
    : undefined;
  const resolution = php ? resolveBackend(site.phpEngine, php, frankens) : null;
  const domain = site.domain.toLowerCase();
  const tlsOk = (tls.data?.suffixes ?? []).some(
    (s) => domain === s || domain.endsWith(`.${s}`),
  );
  const httpsBlocker = !tls.data?.exists
    ? "Create the local certificate authority in Settings first."
    : !tlsOk
      ? `Available for .${(tls.data?.suffixes ?? []).join(", .")} domains.`
      : null;

  return (
    <aside className="grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-3.5 overflow-hidden rounded-xl border bg-card p-4">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <h2 className="truncate font-display text-lg font-semibold leading-tight">
            {site.domain}
          </h2>
          <p
            className="truncate font-mono text-[11px] text-muted-foreground"
            title={site.projectRoot}
          >
            {site.projectRoot}
          </p>
        </div>
        <Badge variant={status.variant} title={site.configMessage ?? undefined}>
          {status.label}
        </Badge>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size="icon"
              variant="ghost"
              aria-label={`More for ${site.domain}`}
            >
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {menu.map((item) => (
              <Fragment key={item.key}>
                {item.separated && <DropdownMenuSeparator />}
                <DropdownMenuItem
                  disabled={item.disabled}
                  destructive={item.destructive}
                  onSelect={item.onSelect}
                >
                  {item.icon} {item.label}
                </DropdownMenuItem>
              </Fragment>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div className="flex gap-2">
        <Button
          className="min-w-0 flex-1"
          disabled={!site.enabled}
          title={site.url}
          onClick={() => api.openSiteUrl(site.id).catch(setError)}
        >
          <ExternalLinkIcon /> <span className="truncate">Open in browser</span>
        </Button>
        <Button
          variant="outline"
          size="icon"
          aria-label="Open project folder"
          title="Open project folder"
          onClick={() =>
            api.reveal({ kind: "siteProjectRoot", id: site.id }).catch(setError)
          }
        >
          <FolderOpenIcon />
        </Button>
      </div>
      <EditButtons site={site} />

      {site.configStatus === "failed" && site.configMessage && (
        <p className="text-xs text-destructive">{site.configMessage}</p>
      )}
      {hosts && (
        <p className="flex items-center gap-1.5 text-xs text-warning">
          <TriangleAlertIcon className="size-3.5" /> {hosts}
        </p>
      )}
      {preview.error && (
        <p className="text-[11px] text-muted-foreground">
          Preview: {preview.error}
        </p>
      )}

      <Group label="Status">
        <Row
          label="Serve this app"
          hint={site.enabled ? undefined : "Kept in Zenv, not served."}
        >
          <Switch
            checked={site.enabled}
            disabled={busy}
            onCheckedChange={(on) =>
              action.mutate({ action: on ? "enable" : "disable", id: site.id })
            }
            aria-label="Serve this app"
          />
        </Row>
      </Group>

      <Group label="Serving">
        <Row label="Web server">
          <NativeSelect
            aria-label="Web server"
            value={site.serviceId ?? ""}
            disabled={busy}
            onChange={(e) => change({ serviceId: e.target.value || null })}
            className="h-8 w-40 text-xs"
          >
            <option value="">Any running</option>
            {webServers.map((s) => (
              <option key={s.service.id} value={s.service.id}>
                {s.service.name}
              </option>
            ))}
          </NativeSelect>
        </Row>
        {site.strategy === "php" && (
          <Row
            label="FrankenPHP / Nginx"
            hint={
              site.compat.frankenphpReady
                ? undefined
                : `Only Apache reads .htaccess: ${site.compat.needsApache.join("; ")}`
            }
          >
            {site.compat.frankenphpReady ? (
              <span className="text-sm text-success">Ready</span>
            ) : (
              <span className="text-sm text-warning">Needs Apache</span>
            )}
          </Row>
        )}
        {!site.compat.frankenphpReady &&
          servedBy &&
          servedBy.service.component !== "apache" && (
            <p className="flex items-start gap-1.5 text-xs text-warning">
              <TriangleAlertIcon className="mt-px size-3.5 shrink-0" />
              {servedBy.service.name} ignores this app's .htaccess rules; choose
              Apache or “Any running”.
            </p>
          )}
        <Row label="Serve as">
          <span className="truncate text-sm text-muted-foreground">
            {site.strategy === "reverse_proxy"
              ? `Proxy to :${site.upstreamPort}`
              : STRATEGY_LABEL[site.strategy]}
          </span>
        </Row>
        <Row label="Type">
          <span className="text-sm text-muted-foreground">
            {PROJECT_TYPE_LABEL[site.projectType]}
          </span>
        </Row>
      </Group>

      {site.strategy === "reverse_proxy" && (
        <Group label="Program">
          <AppProgram
            site={site}
            onShowOutput={() => onAction("output")}
            onChangePort={(port) =>
              save.mutateAsync({
                id: site.id,
                input: toInput(site, { upstreamPort: port }),
              })
            }
          />
        </Group>
      )}

      {site.strategy === "php" && (
        <Group label="Environment">
          <Row label="PHP version">
            <NativeSelect
              aria-label="PHP version"
              value={site.runtimeInstallationId ?? ""}
              disabled={busy}
              onChange={(e) =>
                change({ runtimeInstallationId: e.target.value })
              }
              className="h-8 w-40 text-xs"
            >
              {!php && site.runtimeInstallationId && (
                <option value={site.runtimeInstallationId}>
                  {site.phpVersion ?? "unknown"} (missing)
                </option>
              )}
              {phpVersions.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.version}
                </option>
              ))}
            </NativeSelect>
          </Row>
          <Row
            label="Runs on"
            hint={
              resolution
                ? resolution.backend === null
                  ? resolution.error
                  : `Now: ${BACKEND_LABEL[resolution.backend]}`
                : undefined
            }
          >
            <NativeSelect
              aria-label="Runs on"
              value={site.phpEngine}
              disabled={busy}
              onChange={(e) =>
                change({ phpEngine: e.target.value as PhpEngine })
              }
              className="h-8 w-40 text-xs"
            >
              {(["auto", "fastcgi", "frankenphp"] as const).map((e) => (
                <option key={e} value={e}>
                  {ENGINE_LABEL[e]}
                </option>
              ))}
            </NativeSelect>
          </Row>
        </Group>
      )}

      <Group label="HTTPS">
        <Row label="HTTPS" hint={httpsBlocker ?? undefined}>
          <Switch
            checked={site.httpsEnabled}
            disabled={busy || (Boolean(httpsBlocker) && !site.httpsEnabled)}
            onCheckedChange={(on) =>
              change({
                httpsEnabled: on,
                httpsRedirect: on && site.httpsRedirect,
              })
            }
            aria-label="HTTPS"
          />
        </Row>
        {site.httpsEnabled && (
          <Row label="Redirect HTTP to HTTPS">
            <Switch
              checked={site.httpsRedirect}
              disabled={busy}
              onCheckedChange={(on) => change({ httpsRedirect: on })}
              aria-label="Redirect HTTP to HTTPS"
            />
          </Row>
        )}
      </Group>

      <Group label="Tailscale">
        <TailnetPanel
          site={site}
          serving={
            servedBy
              ? servedBy.health === "running"
              : webServers.some((s) => s.health === "running")
          }
        />
      </Group>

      <Group label="Paths">
        <div className="grid gap-1 text-xs">
          <span className="text-muted-foreground">
            {site.strategy === "reverse_proxy"
              ? "Project folder"
              : "Document root"}
          </span>
          <Mono className="break-all">
            {site.strategy === "reverse_proxy"
              ? site.projectRoot
              : site.documentRoot}
          </Mono>
        </div>
      </Group>

      <ErrorPanel error={save.error ?? action.error ?? error} />
    </aside>
  );
}
