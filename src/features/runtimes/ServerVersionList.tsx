import { MoreHorizontalIcon, PlusIcon } from "lucide-react";
import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Loading } from "@/components/page";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ServiceCard } from "@/features/web-servers/ServiceCard";
import { ServiceFormDialog } from "@/features/web-servers/ServiceFormDialog";
import { useServices } from "@/lib/api/queries";
import type { Category, Installation, Service, ServiceStatus } from "@/lib/api/types";
import { sortServices } from "@/lib/services";

import { useInstallationMenu, VersionBadges } from "./installation-menu";
import { PathButton, PathConflicts } from "./path-controls";

type Form = { editing: Service | null; installation: Installation } | null;

function ServerRow({ status, inst, onEdit, onAnother }: { status: ServiceStatus; inst: Installation; onEdit: () => void; onAnother: () => void }) {
  const menu = useInstallationMenu(inst, { switchable: inst.component === "frankenphp" });
  return (
    <ServiceCard
      status={status}
      onEdit={onEdit}
      version={{
        badges: (
          <>
            <VersionBadges inst={inst} />
            <PathButton inst={inst} />
          </>
        ),
        menu: (
          <>
            <DropdownMenuItem onSelect={onAnother}>
              <PlusIcon /> Another server on {inst.version}
            </DropdownMenuItem>
            {menu.items}
          </>
        ),
        dialogs: menu.dialogs,
        error: menu.error,
      }}
    />
  );
}

/** An installed version that has no server yet. */
function VersionRow({ inst, onSetUp }: { inst: Installation; onSetUp: () => void }) {
  const menu = useInstallationMenu(inst, { switchable: inst.component === "frankenphp" });
  return (
    <div className="rounded-lg border border-dashed bg-card/60">
      <div className="flex items-center gap-3 px-3.5 py-3">
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2">
            <span className="font-mono text-sm font-semibold">{inst.version}</span>
            <VersionBadges inst={inst} />
          </p>
          <p className="truncate font-mono text-[11px] text-muted-foreground" title={inst.rootPath}>
            {inst.rootPath}
          </p>
        </div>
        <PathButton inst={inst} />
        <Button size="sm" variant="outline" onClick={onSetUp} disabled={!inst.executableExists}>
          <PlusIcon /> Set up server
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon" variant="ghost" aria-label={`More for ${inst.version}`}>
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onSetUp} disabled={!inst.executableExists}>
              <PlusIcon /> Set up a server on {inst.version}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            {menu.items}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {Boolean(menu.error) && (
        <div className="px-3.5 pb-3">
          <ErrorPanel error={menu.error} />
        </div>
      )}
      {menu.dialogs}
    </div>
  );
}

/**
 * Installed versions and the servers running them, as one list: each server
 * is a row with its switch; a version without a server offers to set one up.
 */
export function ServerVersionList({ category, installations }: { category: Exclude<Category, "language">; installations: Installation[] }) {
  const services = useServices();
  const [form, setForm] = useState<Form>(null);

  if (services.isPending) return <Loading />;
  if (services.error) return <ErrorPanel error={services.error} />;
  const all = sortServices(services.data);

  return (
    <div className="grid gap-2">
      {installations[0] && (
        <div className="overflow-hidden rounded-lg border empty:hidden">
          <PathConflicts component={installations[0].component} />
        </div>
      )}
      {installations.flatMap((inst) => {
        const servers = all.filter((s) => s.service.installationId === inst.id);
        if (servers.length === 0) {
          return [<VersionRow key={inst.id} inst={inst} onSetUp={() => setForm({ editing: null, installation: inst })} />];
        }
        return servers.map((s) => (
          <ServerRow
            key={s.service.id}
            status={s}
            inst={inst}
            onEdit={() => setForm({ editing: s.service, installation: inst })}
            onAnother={() => setForm({ editing: null, installation: inst })}
          />
        ));
      })}

      {form && (
        <ServiceFormDialog
          open
          kind={category}
          onOpenChange={(o) => !o && setForm(null)}
          installations={form.editing ? installations.filter((i) => i.component === form.installation.component) : [form.installation]}
          editing={form.editing}
        />
      )}
    </div>
  );
}
