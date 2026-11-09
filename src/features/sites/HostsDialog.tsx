import { ShieldCheckIcon } from "lucide-react";

import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useApplyHosts } from "@/lib/api/queries";
import type { HostsApplyOutcome, HostsStatus } from "@/lib/api/types";

function Result({ outcome }: { outcome: HostsApplyOutcome }) {
  const text =
    outcome.method === "unchanged"
      ? "The hosts file already had everything; nothing was changed."
      : "Hosts file updated. New domains work immediately.";
  return (
    <p className="rounded-md border border-success/30 bg-success/5 px-3 py-2 text-sm text-success">
      {text}
      {outcome.backup && <span className="mt-1 block text-xs text-muted-foreground">Previous version saved to {outcome.backup}</span>}
    </p>
  );
}

export function HostsDialog({
  open,
  onOpenChange,
  status,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  status: HostsStatus;
}) {
  const apply = useApplyHosts();
  const { plan } = status;
  const declined = apply.error instanceof ApiError && apply.error.code === "elevation_declined";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>Update the hosts file</DialogTitle>
          <DialogDescription>
            Domains like <span className="font-mono">shop.test</span> only reach this computer through the Windows hosts
            file. Zenv changes nothing but its own marked section, and saves a backup first.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3 text-sm">
          {plan.add.length > 0 && (
            <div>
              <p className="font-medium">Add</p>
              <ul className="mt-1 space-y-0.5">
                {plan.add.map((e) => (
                  <li key={e.domain}>
                    <Mono>{e.ip} → {e.domain}</Mono>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {plan.remove.length > 0 && (
            <div>
              <p className="font-medium">Remove (sites deleted or disabled)</p>
              <ul className="mt-1 space-y-0.5">
                {plan.remove.map((e) => (
                  <li key={e.domain}>
                    <Mono className="text-muted-foreground line-through">{e.ip} → {e.domain}</Mono>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {plan.conflicts.length > 0 && (
            <div className="rounded-md border border-warning/40 bg-warning/5 p-2.5 text-xs">
              <p className="font-medium text-warning">Left alone: your own lines point these domains elsewhere</p>
              {plan.conflicts.map((c) => (
                <p key={c.domain} className="font-mono">
                  {c.domain} → {c.mappedTo}
                </p>
              ))}
            </div>
          )}
          <div>
            <p className="text-xs text-muted-foreground">
              Zenv's section of <Mono>{status.path}</Mono> will read:
            </p>
            <pre className="mt-1 max-h-40 overflow-auto rounded-md border bg-muted/40 p-2 font-mono text-[11.5px]">
              {plan.block.length ? plan.block.join("\n") : "(removed — no domains need it)"}
            </pre>
          </div>
          <p className="flex items-start gap-2 text-xs text-muted-foreground">
            <ShieldCheckIcon className="mt-0.5 size-4 shrink-0" />
            Windows will ask for administrator permission for this one change. Zenv itself keeps running without it.
          </p>
        </div>

        {apply.data && <Result outcome={apply.data} />}
        {declined ? (
          <div className="rounded-md border border-warning/40 bg-warning/5 p-3 text-sm">
            <p className="font-medium">Nothing was changed because permission was declined.</p>
            <p className="mt-1 text-muted-foreground">
              Add the lines above to the hosts file yourself (as administrator), or use <Mono>.localhost</Mono> domains,
              which work in browsers without a hosts entry.
            </p>
          </div>
        ) : (
          <ErrorPanel error={apply.error} />
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {apply.data ? "Close" : "Cancel"}
          </Button>
          {!apply.data && (
            <Button onClick={() => apply.mutate()} disabled={apply.isPending || !plan.changed}>
              {apply.isPending ? "Waiting for permission…" : "Update hosts file"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
