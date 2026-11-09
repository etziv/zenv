import { FolderOpenIcon, PowerIcon, RefreshCwIcon, StarIcon, TerminalIcon, Trash2Icon } from "lucide-react";
import type * as React from "react";
import { useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { api } from "@/lib/api/client";
import { useInstallationAction } from "@/lib/api/queries";
import type { Installation } from "@/lib/api/types";

/** Labels for an installed version: default, installed by Zenv or not, missing. */
export function VersionBadges({ inst }: { inst: Installation }) {
  return (
    <>
      {inst.isDefault && <Badge variant="primary">default</Badge>}
      <Badge variant="outline" title={inst.source === "managed" ? "Installed by Zenv" : "Installed outside Zenv; detected and used as it is"}>
        {inst.source === "managed" ? "zenv" : "system"}
      </Badge>
      {!inst.executableExists && <Badge variant="destructive">missing</Badge>}
      {!inst.enabled && <Badge variant="outline">off</Badge>}
    </>
  );
}

/**
 * Menu items and dialogs for an installed version (make default, re-detect,
 * open folder, switch off, uninstall), to put in any row's ⋯ menu.
 */
export function useInstallationMenu(inst: Installation, opts: { terminal?: boolean; switchable?: boolean } = {}) {
  const action = useInstallationAction();
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const label = `${inst.component} ${inst.version}`;
  const managed = inst.source === "managed";

  const items: React.ReactNode = (
    <>
      {!inst.isDefault && (
        <DropdownMenuItem onSelect={() => action.mutate({ action: "setDefault", id: inst.id })}>
          <StarIcon /> Make {inst.version} the default
        </DropdownMenuItem>
      )}
      <DropdownMenuItem onSelect={() => action.mutate({ action: "refresh", id: inst.id })}>
        <RefreshCwIcon /> Re-detect version
      </DropdownMenuItem>
      {opts.terminal && (
        <DropdownMenuItem onSelect={() => api.openTerminal([inst.id]).catch(setError)}>
          <TerminalIcon /> Open terminal with this version
        </DropdownMenuItem>
      )}
      <DropdownMenuItem onSelect={() => api.reveal({ kind: "installationRoot", id: inst.id }).catch(setError)}>
        <FolderOpenIcon /> Open installation folder
      </DropdownMenuItem>
      {opts.switchable && (
        <DropdownMenuItem onSelect={() => action.mutate({ action: inst.enabled ? "disable" : "enable", id: inst.id })}>
          <PowerIcon /> {inst.enabled ? `Switch ${inst.version} off` : `Switch ${inst.version} on`}
        </DropdownMenuItem>
      )}
      <DropdownMenuSeparator />
      <DropdownMenuItem destructive onSelect={() => setConfirming(true)}>
        <Trash2Icon /> {managed ? `Uninstall ${inst.version}` : `Remove ${inst.version} from Zenv`}
      </DropdownMenuItem>
    </>
  );

  const dialogs: React.ReactNode = (
    <ConfirmDialog
      open={confirming}
      onOpenChange={setConfirming}
      title={managed ? `Uninstall ${label}?` : `Remove ${label} from Zenv?`}
      confirmLabel={managed ? "Uninstall" : "Remove"}
      destructive={managed}
      pending={action.isPending}
      onConfirm={() => action.mutate({ action: managed ? "uninstall" : "unregister", id: inst.id }, { onSettled: () => setConfirming(false) })}
    >
      {managed ? <p>Zenv installed this version and will delete its folder:</p> : <p>Zenv forgets this installation. Its files stay where they are:</p>}
      <Mono>{inst.rootPath}</Mono>
    </ConfirmDialog>
  );

  return { items, dialogs, error: action.error ?? error, pending: action.isPending };
}
