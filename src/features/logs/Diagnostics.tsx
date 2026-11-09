import { useMutation } from "@tanstack/react-query";
import { CircleAlertIcon, CircleCheckIcon, RefreshCwIcon, TriangleAlertIcon } from "lucide-react";
import { type FormEvent, useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Loading, Mono } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, type ApiError } from "@/lib/api/client";
import { useDiagnostics } from "@/lib/api/queries";
import type { CheckStatus, PortInspection } from "@/lib/api/types";

function StatusIcon({ status }: { status: CheckStatus }) {
  switch (status) {
    case "ok":
      return <CircleCheckIcon className="size-4 text-success" aria-label="OK" />;
    case "warning":
      return <TriangleAlertIcon className="size-4 text-warning" aria-label="Warning" />;
    case "error":
      return <CircleAlertIcon className="size-4 text-destructive" aria-label="Error" />;
    default: {
      const unreachable: never = status;
      return unreachable;
    }
  }
}

export function Diagnostics() {
  const checks = useDiagnostics();
  return (
    <div className="grid gap-3">
      <div>
        <Button variant="outline" onClick={() => checks.refetch()} disabled={checks.isFetching}>
          <RefreshCwIcon className={checks.isFetching ? "animate-spin" : undefined} /> Run checks again
        </Button>
      </div>
      {checks.isPending ? (
        <Loading label="Running checks…" />
      ) : checks.error ? (
        <ErrorPanel error={checks.error} />
      ) : (
        <Card>
          <ul className="divide-y">
            {checks.data.map((c) => (
              <li key={c.id} className="flex gap-3 px-4 py-2.5">
                <StatusIcon status={c.status} />
                <div className="min-w-0">
                  <p className="text-sm font-medium">{c.label}</p>
                  <Mono className="text-muted-foreground">{c.detail}</Mono>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

export function PortCheck() {
  const [port, setPort] = useState("80");
  const [address, setAddress] = useState("127.0.0.1");
  const inspect = useMutation<PortInspection, ApiError, void>({ mutationFn: () => api.inspectPort(Number(port), address) });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    inspect.mutate();
  };
  const r = inspect.data;
  return (
    <div className="grid max-w-xl gap-3">
      <form onSubmit={submit} className="flex items-end gap-2">
        <div className="grid gap-1.5">
          <Label htmlFor="pc-addr">Address</Label>
          <Input id="pc-addr" value={address} onChange={(e) => setAddress(e.target.value)} className="w-36 font-mono" />
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="pc-port">Port</Label>
          <Input
            id="pc-port"
            value={port}
            onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
            className="w-24 font-mono"
            inputMode="numeric"
          />
        </div>
        <Button type="submit" disabled={!port || inspect.isPending}>
          Check port
        </Button>
      </form>
      <ErrorPanel error={inspect.error} />
      {r && (
        <Card className="p-4 text-sm">
          {r.status === "free" && (
            <p>
              <Mono>
                {r.address}:{r.port}
              </Mono>{" "}
              is free.
            </p>
          )}
          {r.status === "inUse" && (
            <div>
              <p>
                <Mono>
                  {r.address}:{r.port}
                </Mono>{" "}
                is in use by:
              </p>
              <ul className="mt-1">
                {r.owners.map((o) => (
                  <li key={o.pid}>
                    <Mono>
                      {o.processName ?? "unknown process"} · PID {o.pid} · {o.address}
                    </Mono>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {r.status === "unavailable" && <p>Cannot bind: {r.reason}</p>}
        </Card>
      )}
    </div>
  );
}
