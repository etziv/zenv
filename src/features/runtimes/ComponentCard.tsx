import {
  FolderOpenIcon,
  InfoIcon,
  MoreHorizontalIcon,
  RefreshCwIcon,
  StarIcon,
  TerminalIcon,
  Trash2Icon,
} from "lucide-react";
import { useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useInstallationAction } from "@/lib/api/queries";
import type {
  ComponentInfo,
  Installation,
  RuntimeProcess,
} from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { PathButton, PathConflicts } from "./path-controls";

/**
 * Languages are always available (each site picks its version), so only
 * servers get an on/off switch; switching FrankenPHP off, for example,
 * re-routes its sites through the web server's PHP-CGI.
 */
function InstallationRow({
  inst,
  runtime,
  switchable,
}: {
  inst: Installation;
  runtime?: RuntimeProcess;
  switchable: boolean;
}) {
  const action = useInstallationAction();
  const [confirming, setConfirming] = useState(false);
  const [openError, setOpenError] = useState<unknown>(null);
  const label = `${inst.component} ${inst.version}`;

  return (
    <li
      className={cn("px-4 py-2.5", switchable && !inst.enabled && "opacity-60")}
    >
      <div className="flex items-center gap-3">
        {switchable && (
          <Switch
            checked={inst.enabled}
            disabled={action.isPending}
            onCheckedChange={(on) =>
              action.mutate({ action: on ? "enable" : "disable", id: inst.id })
            }
            aria-label={
              inst.enabled ? `Switch off ${label}` : `Switch on ${label}`
            }
          />
        )}
        <span className="font-mono text-[13px] font-semibold">
          {inst.version}
        </span>
        {inst.isDefault && <Badge variant="primary">default</Badge>}
        <Badge
          variant="outline"
          title={
            inst.source === "managed"
              ? "Installed by Zenv"
              : "Installed outside Zenv; detected and used as it is"
          }
        >
          {inst.source === "managed" ? "zenv" : "system"}
        </Badge>
        {!inst.executableExists && <Badge variant="destructive">missing</Badge>}
        {runtime && runtime.running > 0 && (
          <Badge
            variant="success"
            title={`127.0.0.1:${runtime.ports.join(", ")}`}
          >
            {runtime.kind === "php-cgi"
              ? `${runtime.running} workers`
              : "running"}
          </Badge>
        )}
        <Mono
          className="min-w-0 flex-1 truncate text-muted-foreground"
          title={inst.versionDetail}
        >
          {inst.rootPath}
        </Mono>
        <PathButton inst={inst} />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Actions for ${label}`}
            >
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {!inst.isDefault && (
              <DropdownMenuItem
                onSelect={() =>
                  action.mutate({ action: "setDefault", id: inst.id })
                }
              >
                <StarIcon /> Make default
              </DropdownMenuItem>
            )}
            <DropdownMenuItem
              onSelect={() => action.mutate({ action: "refresh", id: inst.id })}
            >
              <RefreshCwIcon /> Re-detect version
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => api.openTerminal([inst.id]).catch(setOpenError)}
            >
              <TerminalIcon /> Open terminal with this version
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() =>
                api
                  .reveal({ kind: "installationRoot", id: inst.id })
                  .catch(setOpenError)
              }
            >
              <FolderOpenIcon /> Open folder
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem destructive onSelect={() => setConfirming(true)}>
              <Trash2Icon />{" "}
              {inst.source === "managed" ? "Uninstall" : "Remove from list"}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <ErrorPanel error={action.error ?? openError} className="mt-2" />

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={
          inst.source === "managed"
            ? `Uninstall ${label}?`
            : `Remove ${label} from Zenv?`
        }
        confirmLabel={inst.source === "managed" ? "Uninstall" : "Remove"}
        destructive={inst.source === "managed"}
        pending={action.isPending}
        onConfirm={() =>
          action.mutate(
            {
              action: inst.source === "managed" ? "uninstall" : "unregister",
              id: inst.id,
            },
            { onSettled: () => setConfirming(false) },
          )
        }
      >
        {inst.source === "managed" ? (
          <p>Zenv installed this version and will delete its folder:</p>
        ) : (
          <p>Zenv forgets this installation. Its files stay where they are:</p>
        )}
        <Mono>{inst.rootPath}</Mono>
      </ConfirmDialog>
    </li>
  );
}

export function ComponentCard({
  info,
  installations,
  runtimes,
  showHeader = true,
}: {
  info: ComponentInfo;
  installations: Installation[];
  runtimes: RuntimeProcess[];
  showHeader?: boolean;
}) {
  return (
    <Card>
      {showHeader && (
        <div className="flex items-center gap-2 border-b px-4 py-2.5">
          <h3 className="text-sm font-semibold">{info.name}</h3>
          <span
            className="text-muted-foreground"
            title={[info.distribution, ...info.notes].join("\n\n")}
          >
            <InfoIcon className="size-3.5" aria-label={`About ${info.name}`} />
          </span>
          {info.windowsSupport === "no_official_build" && (
            <Badge variant="warning">unofficial Windows build</Badge>
          )}
        </div>
      )}
      <PathConflicts component={info.id} />
      <ul className="divide-y">
        {installations.map((i) => (
          <InstallationRow
            key={i.id}
            inst={i}
            runtime={runtimes.find((r) => r.installationId === i.id)}
            switchable={info.category !== "language"}
          />
        ))}
      </ul>
    </Card>
  );
}
