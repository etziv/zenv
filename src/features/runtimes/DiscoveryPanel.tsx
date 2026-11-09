import { CheckIcon, XIcon } from "lucide-react";

import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useInstallations, useRegisterInstallation } from "@/lib/api/queries";
import type { ComponentInfo, DiscoveredInstallation } from "@/lib/api/types";

export function DiscoveryPanel({
  results,
  components,
  onClose,
}: {
  results: DiscoveredInstallation[];
  components: ComponentInfo[];
  onClose: () => void;
}) {
  const register = useRegisterInstallation();
  const installations = useInstallations();
  // Scan results are a snapshot; registration state comes from the live list.
  const isRegistered = (r: DiscoveredInstallation) =>
    (installations.data ?? []).some(
      (i) => i.component === r.component && i.executablePath.toLowerCase() === r.executablePath.toLowerCase(),
    );
  const name = (id: string) => components.find((c) => c.id === id)?.name ?? id;
  const relevant = results.filter((r) => components.some((c) => c.id === r.component));

  return (
    <Card className="border-primary/30">
      <div className="flex items-center justify-between border-b px-4 py-2.5">
        <p className="text-sm font-medium">
          Found {relevant.length} installation{relevant.length === 1 ? "" : "s"} on this computer
        </p>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close scan results">
          <XIcon />
        </Button>
      </div>
      {relevant.length === 0 ? (
        <p className="px-4 py-4 text-sm text-muted-foreground">
          Nothing was found on PATH or in common install locations. Use “Register existing” to point Zenv at a
          folder.
        </p>
      ) : (
        <ul className="divide-y">
          {relevant.map((r) => (
            <li key={r.executablePath} className="flex items-center gap-3 px-4 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-sm">
                  <span className="font-medium">{name(r.component)}</span>
                  {r.version ? <Badge variant="outline">{r.version}</Badge> : <Badge variant="warning">unverified</Badge>}
                </div>
                <Mono className="text-muted-foreground">{r.executablePath}</Mono>
                {r.error && <p className="text-xs text-warning">{r.error}</p>}
              </div>
              {isRegistered(r) ? (
                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                  <CheckIcon className="size-3.5" /> Registered
                </span>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!r.version || register.isPending}
                  onClick={() => register.mutate({ component: r.component, path: r.executablePath })}
                >
                  Register
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {register.error && <ErrorPanel error={register.error} className="m-3" />}
    </Card>
  );
}
