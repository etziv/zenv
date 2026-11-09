import { useMutation } from "@tanstack/react-query";
import { open as openPicker } from "@tauri-apps/plugin-dialog";
import { FolderOpenIcon } from "lucide-react";
import { type FormEvent, useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input, NativeSelect } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, type ApiError } from "@/lib/api/client";
import { useSaveService } from "@/lib/api/queries";
import type { Category, ComponentId, Installation, PortInspection, Service } from "@/lib/api/types";

const ADDRESSES = [
  { value: "127.0.0.1", label: "127.0.0.1 — this computer only (recommended)" },
  { value: "0.0.0.0", label: "0.0.0.0 — all network interfaces" },
  { value: "::1", label: "::1 — this computer only (IPv6)" },
] as const;

function PortResult({ result }: { result: PortInspection }) {
  switch (result.status) {
    case "free":
      return <p className="text-xs text-success">Port {result.port} is free.</p>;
    case "inUse":
      return (
        <p className="text-xs text-warning">
          Port {result.port} is in use by{" "}
          {result.owners.map((o) => `${o.processName ?? "unknown"} (PID ${o.pid})`).join(", ")}.
        </p>
      );
    case "unavailable":
      return <p className="text-xs text-warning">Port {result.port} cannot be used: {result.reason}</p>;
    default: {
      const unreachable: never = result;
      return unreachable;
    }
  }
}

const COMPONENT: Partial<Record<ComponentId, { label: string; port: number; data: boolean }>> = {
  apache: { label: "Apache", port: 8080, data: false },
  nginx: { label: "Nginx", port: 8080, data: false },
  mysql: { label: "MySQL", port: 3306, data: true },
  postgresql: { label: "PostgreSQL", port: 5432, data: true },
  redis: { label: "Redis", port: 6379, data: true },
  omniroute: { label: "OmniRoute", port: 20128, data: true },
  ninerouter: { label: "9router", port: 20128, data: true },
};

const TITLE: Record<Category, string> = {
  web_server: "web server",
  database: "database server",
  cache: "cache server",
  language: "service",
  tool: "service",
};

export function ServiceFormDialog({
  open,
  onOpenChange,
  installations,
  editing,
  kind,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  installations: Installation[];
  editing: Service | null;
  kind: Category;
}) {
  const componentOf = (id: string) => installations.find((i) => i.id === id)?.component;
  const componentName = (id: string) => COMPONENT[componentOf(id) ?? "apache"]?.label ?? "Server";
  const defaultPort = (id: string) => COMPONENT[componentOf(id) ?? "apache"]?.port ?? 8080;
  const firstInstall = editing?.installationId ?? installations.find((i) => i.isDefault)?.id ?? installations[0]?.id ?? "";
  const [name, setName] = useState(editing?.name ?? componentName(firstInstall));
  const [installationId, setInstallationId] = useState(firstInstall);
  const [listenAddress, setListenAddress] = useState(editing?.listenAddress ?? "127.0.0.1");
  const [port, setPort] = useState(String(editing?.httpPort ?? defaultPort(firstInstall)));
  const [httpsPort, setHttpsPort] = useState(String(editing?.httpsPort ?? 443));
  const [autostart, setAutostart] = useState(editing?.autostart ?? false);
  const [dataDir, setDataDir] = useState(editing?.dataDir ?? "");
  const needsData = COMPONENT[componentOf(installationId) ?? "apache"]?.data ?? false;
  const browseData = async () => {
    const picked = await openPicker({ directory: true, title: "Select the data folder" });
    if (typeof picked === "string") setDataDir(picked);
  };
  const save = useSaveService();
  const check = useMutation<PortInspection, ApiError, void>({
    mutationFn: () => api.inspectPort(Number(port), listenAddress),
  });

  const portNumber = Number(port);
  const httpsNumber = Number(httpsPort);
  const isWeb = kind === "web_server";
  const portValid =
    Number.isInteger(portNumber) &&
    portNumber >= 1 &&
    portNumber <= 65535 &&
    (!isWeb || (Number.isInteger(httpsNumber) && httpsNumber >= 1 && httpsNumber <= 65535 && httpsNumber !== portNumber));

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!portValid) return;
    save.mutate(
      {
        id: editing?.id ?? null,
        input: {
          name,
          installationId,
          listenAddress,
          httpPort: portNumber,
          autostart,
          dataDir: needsData ? dataDir.trim() || null : null,
          httpsPort: isWeb ? httpsNumber : null,
        },
      },
      { onSuccess: () => onOpenChange(false) },
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{editing ? `Edit ${editing.name}` : `Add a ${TITLE[kind]}`}</DialogTitle>
            <DialogDescription>
              Zenv writes its own configuration for this server and leaves the installation's config files alone.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-1.5">
            <Label htmlFor="svc-name">Name</Label>
            <Input id="svc-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} required />
          </div>

          <div className="grid gap-1.5">
            <Label htmlFor="svc-install">Installation</Label>
            <NativeSelect
              id="svc-install"
              value={installationId}
              onChange={(e) => {
                if (!editing && name === componentName(installationId)) setName(componentName(e.target.value));
                if (!editing && port === String(defaultPort(installationId))) setPort(String(defaultPort(e.target.value)));
                setInstallationId(e.target.value);
              }}
            >
              {installations.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.component} {i.version} — {i.rootPath}
                </option>
              ))}
            </NativeSelect>
          </div>

          <div className={isWeb ? "grid grid-cols-[1fr_7rem_7rem] gap-3" : "grid grid-cols-[1fr_8rem] gap-3"}>
            <div className="grid gap-1.5">
              <Label htmlFor="svc-addr">Listen on</Label>
              <NativeSelect id="svc-addr" value={listenAddress} onChange={(e) => setListenAddress(e.target.value)}>
                {ADDRESSES.map((a) => (
                  <option key={a.value} value={a.value}>
                    {a.label}
                  </option>
                ))}
              </NativeSelect>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="svc-port">HTTP port</Label>
              <Input
                id="svc-port"
                inputMode="numeric"
                value={port}
                onChange={(e) => {
                  setPort(e.target.value.replace(/\D/g, ""));
                  check.reset();
                }}
                aria-invalid={!portValid}
                className="font-mono"
              />
            </div>
            {isWeb && (
              <div className="grid gap-1.5">
                <Label htmlFor="svc-https-port">HTTPS port</Label>
                <Input
                  id="svc-https-port"
                  inputMode="numeric"
                  value={httpsPort}
                  onChange={(e) => setHttpsPort(e.target.value.replace(/\D/g, ""))}
                  className="font-mono"
                />
              </div>
            )}
          </div>
          <div className="-mt-2 flex items-center gap-3">
            <Button type="button" variant="link" size="sm" disabled={!portValid || check.isPending} onClick={() => check.mutate()}>
              Check whether the port is free
            </Button>
            {check.data && <PortResult result={check.data} />}
          </div>
          {check.error && <ErrorPanel error={check.error} />}

          {needsData && (
            <div className="grid gap-1.5">
              <Label htmlFor="svc-data">Data folder</Label>
              <div className="flex gap-2">
                <Input
                  id="svc-data"
                  value={dataDir}
                  onChange={(e) => setDataDir(e.target.value)}
                  placeholder="Leave empty to keep it inside Zenv's data folder"
                  className="font-mono text-[12px]"
                />
                <Button type="button" variant="outline" onClick={browseData}>
                  <FolderOpenIcon /> Browse
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                An existing data folder is used as it is; Zenv never initializes or overwrites one that has data.
              </p>
            </div>
          )}

          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={autostart} onChange={(e) => setAutostart(e.target.checked)} className="size-4 accent-primary" />
            Start automatically when Zenv opens
          </label>

          <ErrorPanel error={save.error} />

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!portValid || !installationId || save.isPending}>
              {editing ? "Save changes" : `Add ${TITLE[kind]}`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
