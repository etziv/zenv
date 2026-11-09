import {
  ActivityIcon,
  ChevronDownIcon,
  CopyIcon,
  DatabaseIcon,
  ExternalLinkIcon,
  FileCogIcon,
  FolderOpenIcon,
  MoreHorizontalIcon,
  PencilIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  ShieldCheckIcon,
  Trash2Icon,
  TriangleAlertIcon,
} from "lucide-react";
import { useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { StatusSwitch } from "@/components/ui/status-switch";
import { ApiError, api } from "@/lib/api/client";
import {
  useDeleteService,
  useInitializeServiceData,
  useProbeService,
  useServiceAction,
  useValidateService,
} from "@/lib/api/queries";
import { isRouter, type RevealTarget, type ServiceStatus } from "@/lib/api/types";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";

import { ServiceDetails } from "./ServiceDetails";
import { RouterPanel } from "@/features/tools/RouterPanel";

const KIND_NOUN = { web_server: "web server", database: "database server", cache: "cache server", language: "service", tool: "service" } as const;

function Notice({ tone, children }: { tone: "warning" | "error" | "success"; children: React.ReactNode }) {
  return (
    <p
      className={cn(
        "flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
        tone === "warning" && "border-warning/40 bg-warning/5",
        tone === "error" && "border-destructive/30 bg-destructive/5 text-destructive",
        tone === "success" && "border-success/30 bg-success/5 text-success",
      )}
    >
      {children}
    </p>
  );
}

/** Extra content from the installed version the server runs on. */
export interface VersionExtras {
  badges: React.ReactNode;
  menu: React.ReactNode;
  dialogs: React.ReactNode;
  error: unknown;
}

/** One managed server on one line: switch, what it is, where it listens, and a menu for the rest. */
export function ServiceCard({ status, onEdit, version }: { status: ServiceStatus; onEdit: () => void; version?: VersionExtras }) {
  const { service, installation, health, kind } = status;
  const action = useServiceAction();
  const validate = useValidateService();
  const remove = useDeleteService();
  const init = useInitializeServiceData();
  const probe = useProbeService();
  const [expanded, setExpanded] = useState(false);
  const [confirm, setConfirm] = useState<"delete" | "init" | null>(null);
  const [openError, setOpenError] = useState<unknown>(null);

  const isWeb = kind === "web_server";
  const noun = KIND_NOUN[kind];
  const running = health === "running";
  const busy = action.isPending;
  const dataState = status.dataDirInfo?.state;
  const needsInit = status.needsInitialization && (dataState === "missing" || dataState === "empty");
  const dataBlocked = dataState === "foreign" || dataState === "incompatible";
  const connection = !isWeb && dataState === "initialized" ? status.connection : null;
  const reveal = (target: RevealTarget) => {
    setOpenError(null);
    api.reveal(target).catch(setOpenError);
  };
  const run = (a: "start" | "stop" | "restart" | "reload") => {
    validate.reset();
    probe.reset();
    action.mutate({ action: a, id: service.id }, { onSuccess: () => setOpenError(null) });
  };
  const copy = (value: string, what: string) => {
    void navigator.clipboard?.writeText(value);
    toast({ variant: "success", title: `${what} copied`, description: value });
  };

  return (
    <div
      className={cn(
        "rounded-lg border bg-card transition-colors",
        running && "border-primary/40",
        health === "failed" && "border-destructive/40",
      )}
    >
      <div className="flex items-center gap-3 px-3.5 py-3">
        <StatusSwitch
          checked={running}
          pending={busy}
          failed={health === "failed"}
          disabled={busy || (!running && (!installation?.executableExists || needsInit || dataBlocked))}
          onCheckedChange={(on) => run(on ? "start" : "stop")}
          aria-label={running ? `Stop ${service.name}` : `Start ${service.name}`}
          title={needsInit ? "Initialize the data folder first" : undefined}
        />
        <div className="min-w-0 flex-1">
          <p className="flex items-baseline gap-2">
            <span className="truncate text-sm font-semibold">{service.name}</span>
            {installation && <span className="shrink-0 font-mono text-[11px] text-muted-foreground">{installation.version}</span>}
            {version && <span className="flex shrink-0 items-center gap-1 self-center">{version.badges}</span>}
          </p>
          <p className="truncate font-mono text-[11px] text-muted-foreground" title={isWeb ? status.url : (service.dataDir ?? "")}>
            {service.listenAddress}:{service.httpPort}
            {isWeb && service.httpsPort ? ` · :${service.httpsPort}` : ""}
            {!isWeb && service.dataDir ? ` · ${service.dataDir}` : ""}
          </p>
        </div>

        {running && isWeb && (
          <Button size="icon" variant="ghost" title={`Open ${status.url}`} aria-label="Open in browser" onClick={() => api.openServiceUrl(service.id).catch(setOpenError)}>
            <ExternalLinkIcon />
          </Button>
        )}
        {connection && (
          <Button
            size="icon"
            variant="ghost"
            title={`Copy ${connection.url}${connection.user !== null ? `\n${connection.user} / ${connection.password === "" ? "(no password)" : "••••"}` : ""}`}
            aria-label="Copy connection URL"
            onClick={() => copy(connection.url, "Connection URL")}
          >
            <CopyIcon />
          </Button>
        )}
        {!running && needsInit && (
          <Button size="sm" onClick={() => setConfirm("init")} disabled={init.isPending}>
            <DatabaseIcon /> {init.isPending ? "Initializing…" : "Initialize"}
          </Button>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon" variant="ghost" aria-label={`More for ${service.name}`}>
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {running && isWeb && (
              <DropdownMenuItem onSelect={() => run("reload")}>
                <RefreshCwIcon /> Reload configuration
              </DropdownMenuItem>
            )}
            {running && (
              <DropdownMenuItem onSelect={() => run("restart")}>
                <RotateCcwIcon /> Restart
              </DropdownMenuItem>
            )}
            {running && !isWeb && (
              <DropdownMenuItem onSelect={() => probe.mutate(service.id)}>
                <ActivityIcon /> Test connection
              </DropdownMenuItem>
            )}
            {!dataBlocked && (
              <DropdownMenuItem onSelect={() => validate.mutate(service.id)}>
                <ShieldCheckIcon /> Check configuration
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onSelect={() => setExpanded((v) => !v)}>
              <ChevronDownIcon /> {expanded ? "Hide logs & activity" : "Logs & activity"}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => reveal({ kind: "serviceConfig", id: service.id })}>
              <FileCogIcon /> Configuration file
            </DropdownMenuItem>
            {isWeb ? (
              <DropdownMenuItem onSelect={() => reveal({ kind: "serviceDocumentRoot", id: service.id })}>
                <FolderOpenIcon /> Document root
              </DropdownMenuItem>
            ) : (
              <DropdownMenuItem onSelect={() => reveal({ kind: "serviceDataDir", id: service.id })}>
                <FolderOpenIcon /> Data folder
              </DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={running} onSelect={onEdit}>
              <PencilIcon /> Edit{running ? " (stop it first)" : ""}
            </DropdownMenuItem>
            <DropdownMenuItem destructive disabled={running} onSelect={() => setConfirm("delete")}>
              <Trash2Icon /> Delete server{running ? " (stop it first)" : ""}
            </DropdownMenuItem>
            {version && (
              <>
                <DropdownMenuSeparator />
                {version.menu}
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {Boolean(needsInit || dataBlocked || init.data !== undefined || probe.data || validate.data || action.error || validate.error || remove.error || init.error || probe.error || openError || version?.error) && (
        <div className="grid gap-2 px-3.5 pb-3">
          {needsInit && (
            <Notice tone="warning">
              <TriangleAlertIcon className="size-3.5 text-warning" /> The data folder is not initialized yet. Initialize it once, then
              switch the server on.
            </Notice>
          )}
          {dataBlocked && (
            <Notice tone="error">
              <TriangleAlertIcon className="size-3.5" /> Zenv will not use this data folder: {status.dataDirInfo?.detail}
            </Notice>
          )}
          {init.data !== undefined && <Notice tone="success">Data folder initialized.</Notice>}
          {probe.data && <Notice tone="success">{probe.data}</Notice>}
          {validate.data &&
            (validate.data.ok ? (
              <Notice tone="success">Configuration is valid.</Notice>
            ) : (
              <ErrorPanel error={new ApiError("config_invalid", "The configuration check failed", { output: validate.data.output })} />
            ))}
          <ErrorPanel error={action.error ?? validate.error ?? remove.error ?? init.error ?? probe.error ?? openError ?? version?.error} />
        </div>
      )}

      {expanded && (
        <div className="border-t">
          <ServiceDetails id={service.id} />
        </div>
      )}

      {version?.dialogs}
      {isRouter(installation?.component) && <RouterPanel component={installation.component} status={status} />}

      <ConfirmDialog
        open={confirm === "delete"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={`Delete ${service.name}?`}
        confirmLabel={`Delete ${noun}`}
        destructive
        pending={remove.isPending}
        onConfirm={() => remove.mutate(service.id, { onSettled: () => setConfirm(null) })}
      >
        <p>
          Zenv removes this {noun}'s record and the configuration it generated. The installation and the logs are kept
          {isWeb ? ", and so is the document root." : "."}
        </p>
        {!isWeb && service.dataDir && (
          <p>
            The data folder is <strong>not</strong> deleted: <Mono>{service.dataDir}</Mono>
          </p>
        )}
      </ConfirmDialog>

      <ConfirmDialog
        open={confirm === "init"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Initialize the data folder?"
        confirmLabel="Initialize"
        pending={init.isPending}
        onConfirm={() => init.mutate(service.id, { onSettled: () => setConfirm(null) })}
      >
        <p>
          Zenv creates a new, empty {installation?.component === "postgresql" ? "PostgreSQL" : "MySQL"} data folder at:
        </p>
        <Mono>{service.dataDir}</Mono>
        <p>
          {installation?.component === "postgresql"
            ? "The postgres user can connect from this computer without a password."
            : "The root user can connect from this computer without a password."}{" "}
          This only works on an empty folder; existing data is never touched.
        </p>
      </ConfirmDialog>
    </div>
  );
}
