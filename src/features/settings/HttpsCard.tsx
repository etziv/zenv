import { ShieldCheckIcon, ShieldIcon, ShieldOffIcon } from "lucide-react";
import { useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { ErrorPanel } from "@/components/error-panel";
import { Loading, Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useTlsAction, useTlsStatus } from "@/lib/api/queries";

/** Zenv's local certificate authority: create it, then (separately) trust it. */
export function HttpsCard() {
  const status = useTlsStatus();
  const action = useTlsAction();
  const [confirm, setConfirm] = useState<"trust" | "untrust" | null>(null);

  if (status.isPending) return <Loading />;
  if (status.error) return <ErrorPanel error={status.error} />;
  const ca = status.data;
  const trusted = ca.trustedUser || ca.trustedMachine;

  return (
    <Card className="grid gap-3 p-4">
      <div className="flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-md border bg-muted/40">
          {!ca.exists ? <ShieldOffIcon className="size-4" /> : trusted ? <ShieldCheckIcon className="size-4 text-success" /> : <ShieldIcon className="size-4 text-warning" />}
        </span>
        <div className="min-w-0 flex-1">
          <p className="flex flex-wrap items-center gap-2 font-medium">
            Local certificate authority
            {ca.exists ? (
              trusted ? <Badge variant="success">Trusted</Badge> : <Badge variant="warning">Not trusted yet</Badge>
            ) : (
              <Badge variant="outline">Not created</Badge>
            )}
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            Issues certificates for <Mono>.{ca.suffixes.join(", .")}</Mono> only. It cannot sign certificates for real
            domains, and its key never leaves this computer.
          </p>
        </div>
      </div>

      {ca.exists && (
        <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-muted-foreground">Subject</dt>
          <dd><Mono>{ca.subject}</Mono></dd>
          <dt className="text-muted-foreground">SHA-256</dt>
          <dd className="break-all"><Mono>{ca.sha256}</Mono></dd>
          <dt className="text-muted-foreground">Valid until</dt>
          <dd><Mono>{ca.notAfter?.slice(0, 10)}</Mono></dd>
          <dt className="text-muted-foreground">Certificate</dt>
          <dd><Mono>{ca.certPath}</Mono></dd>
        </dl>
      )}

      <div className="flex flex-wrap gap-2">
        {!ca.exists && (
          <Button size="sm" disabled={action.isPending} onClick={() => action.mutate("create")}>
            Create certificate authority
          </Button>
        )}
        {ca.exists && !ca.trustedUser && (
          <Button size="sm" disabled={action.isPending} onClick={() => setConfirm("trust")}>
            Trust in Windows…
          </Button>
        )}
        {ca.trustedUser && (
          <Button size="sm" variant="outline" disabled={action.isPending} onClick={() => setConfirm("untrust")}>
            Remove trust…
          </Button>
        )}
      </div>
      {ca.exists && !trusted && (
        <p className="text-xs text-muted-foreground">
          HTTPS sites already work; until the authority is trusted, browsers show a certificate warning.
        </p>
      )}
      <ErrorPanel error={action.error} />

      <ConfirmDialog
        open={confirm === "trust"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Trust Zenv's certificate authority?"
        confirmLabel="Continue to Windows"
        pending={action.isPending}
        onConfirm={() => action.mutate("trust", { onSettled: () => setConfirm(null) })}
      >
        <p>
          It is added to your Windows user's Trusted Root store, so Edge, Chrome and other Windows apps accept your local
          HTTPS sites. Windows shows its own security warning next; choose “Yes” there to finish.
        </p>
        <p>Firefox uses its own store unless <Mono>security.enterprise_roots.enabled</Mono> is on.</p>
      </ConfirmDialog>
      <ConfirmDialog
        open={confirm === "untrust"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Remove trust?"
        confirmLabel="Continue to Windows"
        pending={action.isPending}
        onConfirm={() => action.mutate("untrust", { onSettled: () => setConfirm(null) })}
      >
        <p>Browsers will warn about your local HTTPS sites again. The authority and its certificates are kept.</p>
      </ConfirmDialog>
    </Card>
  );
}
