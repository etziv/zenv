import { CircleAlertIcon, CircleCheckIcon, RefreshCwIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { LogView } from "@/components/log-view";
import { Loading, Mono } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSiteDiagnosis, useSiteLog } from "@/lib/api/queries";
import type { CheckStatus, SiteView } from "@/lib/api/types";

function Icon({ status }: { status: CheckStatus }) {
  switch (status) {
    case "ok":
      return <CircleCheckIcon className="size-4 shrink-0 text-success" aria-label="OK" />;
    case "warning":
      return <TriangleAlertIcon className="size-4 shrink-0 text-warning" aria-label="Warning" />;
    case "error":
      return <CircleAlertIcon className="size-4 shrink-0 text-destructive" aria-label="Problem" />;
    default: {
      const unreachable: never = status;
      return unreachable;
    }
  }
}

function Diagnosis({ id }: { id: string }) {
  const checks = useSiteDiagnosis(id);
  return (
    <div className="grid gap-2">
      <div>
        <Button size="sm" variant="outline" onClick={() => checks.refetch()} disabled={checks.isFetching}>
          <RefreshCwIcon className={checks.isFetching ? "animate-spin" : undefined} /> Check again
        </Button>
      </div>
      {checks.isPending ? (
        <Loading label="Checking…" />
      ) : checks.error ? (
        <ErrorPanel error={checks.error} />
      ) : (
        <ul className="divide-y rounded-md border">
          {checks.data.map((c) => (
            <li key={c.id} className="flex gap-2.5 px-3 py-2">
              <Icon status={c.status} />
              <div className="min-w-0">
                <p className="text-sm font-medium">{c.label}</p>
                <p className="text-xs break-words text-muted-foreground">{c.detail}</p>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function SiteLog({ id, kind }: { id: string; kind: "error" | "access" | "process" }) {
  const [follow, setFollow] = useState(true);
  const log = useSiteLog(id, kind, follow);
  return (
    <div className="grid gap-2">
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} className="accent-primary" />
        Follow
      </label>
      {log.isPending ? <Loading /> : log.error ? <ErrorPanel error={log.error} /> : <LogView tail={log.data} follow={follow} />}
    </div>
  );
}

export function SiteDetailsDialog({
  site,
  onOpenChange,
  initialTab = "diagnose",
}: {
  site: SiteView;
  onOpenChange: (open: boolean) => void;
  initialTab?: "diagnose" | "error" | "access" | "process";
}) {
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{site.name}</DialogTitle>
          <DialogDescription asChild>
            <div>
              <Mono>{site.url}</Mono> · served by {site.serviceName}
            </div>
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue={initialTab}>
          <TabsList>
            <TabsTrigger value="diagnose">Diagnose</TabsTrigger>
            <TabsTrigger value="error">Error log</TabsTrigger>
            <TabsTrigger value="access">Access log</TabsTrigger>
            {site.strategy === "reverse_proxy" && <TabsTrigger value="process">Output</TabsTrigger>}
          </TabsList>
          <TabsContent value="diagnose">
            <Diagnosis id={site.id} />
          </TabsContent>
          <TabsContent value="error">
            <SiteLog id={site.id} kind="error" />
          </TabsContent>
          <TabsContent value="access">
            <SiteLog id={site.id} kind="access" />
          </TabsContent>
          {site.strategy === "reverse_proxy" && (
            <TabsContent value="process">
              <SiteLog id={site.id} kind="process" />
            </TabsContent>
          )}
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
