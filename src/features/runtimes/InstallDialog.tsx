import { CheckIcon, DownloadIcon } from "lucide-react";
import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Loading, Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { NativeSelect } from "@/components/ui/input";
import { useAvailableVersions, useInstallJobs, useStartInstall } from "@/lib/api/queries";
import type { ComponentInfo } from "@/lib/api/types";
import { formatBytes } from "@/lib/format";

import { InstallProgress } from "./install-progress";

export function InstallDialog({ components, onOpenChange }: { components: ComponentInfo[]; onOpenChange: (open: boolean) => void }) {
  const installable = components.filter((c) => c.installable);
  const [component, setComponent] = useState(installable[0]?.id ?? null);
  const [showAll, setShowAll] = useState(false);
  const available = useAvailableVersions(component);
  const jobs = useInstallJobs();
  const start = useStartInstall();
  const [jobId, setJobId] = useState<string | null>(null);
  const job = jobs.data?.find((j) => j.id === jobId) ?? null;


  const info = installable.find((c) => c.id === component);
  const versions = (available.data ?? []).filter((v) => showAll || v.stable).slice(0, showAll ? 200 : 25);

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Install a version</DialogTitle>
          <DialogDescription>
            Downloaded from the official source and installed into the Zenv folder. Versions already on this computer
            keep working.
          </DialogDescription>
        </DialogHeader>

        {installable.length > 1 && (
          <NativeSelect value={component ?? ""} onChange={(e) => setComponent(e.target.value as typeof component)} aria-label="Component">
            {installable.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </NativeSelect>
        )}

        {job && <InstallProgress job={job} name={info?.name} />}

        {available.isPending ? (
          <Loading label={`Fetching ${info?.name ?? ""} releases…`} />
        ) : available.error ? (
          <ErrorPanel error={available.error} />
        ) : (
          <div className="grid gap-2">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>
                {info?.name} releases from <Mono>{available.data?.[0]?.source}</Mono>
                {available.data?.[0]?.verifiedBy && <span className="block">Checked by: {available.data[0].verifiedBy}</span>}
              </span>
              <label className="flex items-center gap-1.5">
                <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} className="accent-primary" />
                Include pre-releases and older
              </label>
            </div>
            <ul className="max-h-72 divide-y overflow-auto rounded-md border">
              {versions.map((v) => (
                <li key={v.version} className="flex items-center gap-3 px-3 py-2">
                  <span className="font-mono text-[13px] font-semibold">{v.version}</span>
                  {!v.stable && <Badge variant="warning">pre-release</Badge>}
                  {v.size > 0 && <span className="text-xs text-muted-foreground">{formatBytes(v.size)}</span>}
                  <span className="ml-auto">
                    {v.installedId ? (
                      <span className="flex items-center gap-1 text-xs text-muted-foreground">
                        <CheckIcon className="size-3.5" /> Installed
                      </span>
                    ) : (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={start.isPending || (job !== null && job.phase !== "done" && job.phase !== "failed")}
                        onClick={() =>
                          component && start.mutate({ component, version: v.version }, { onSuccess: (id) => setJobId(id) })
                        }
                      >
                        <DownloadIcon /> Install
                      </Button>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
        <ErrorPanel error={start.error} />
      </DialogContent>
    </Dialog>
  );
}
