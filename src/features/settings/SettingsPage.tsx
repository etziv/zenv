import { FolderOpenIcon } from "lucide-react";
import { useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Loading, Mono, Page, PageHeader, Section } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api } from "@/lib/api/client";
import { useAppInfo } from "@/lib/api/queries";
import type { RevealTarget } from "@/lib/api/types";

import { HttpsCard } from "./HttpsCard";
import { ThemePicker } from "./ThemePicker";

export function SettingsPage() {
  const info = useAppInfo();
  const [openError, setOpenError] = useState<unknown>(null);
  const reveal = (target: RevealTarget) => api.reveal(target).catch(setOpenError);

  if (info.isPending) {
    return (
      <Page>
        <Loading />
      </Page>
    );
  }
  if (info.error) {
    return (
      <Page>
        <ErrorPanel error={info.error} />
      </Page>
    );
  }

  const { paths } = info.data;
  const rows: { label: string; value: string; target?: RevealTarget }[] = [
    { label: "Zenv home", value: paths.root, target: { kind: "dataDir" } },
    { label: "Installed versions (bin)", value: paths.binDir },
    { label: "Database and cache data", value: paths.dataDir },
    { label: "Generated server configuration (etc)", value: paths.generatedDir },
    { label: "Logs", value: paths.logsDir, target: { kind: "logsDir" } },
    { label: "Default pages of the web servers (dev)", value: paths.devDir },
    { label: "Backups", value: paths.backupsDir },
    { label: "Metadata database", value: paths.dbFile },
  ];

  return (
    <Page>
      <PageHeader
        title="Settings"
      />
      <Section title="About">
        <Card className="grid grid-cols-2 gap-4 p-4 md:grid-cols-4">
          <div>
            <p className="text-xs text-muted-foreground">Version</p>
            <Mono>{info.data.version}</Mono>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">Database schema</p>
            <Mono>v{info.data.schemaVersion}</Mono>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">Platform</p>
            <Mono>
              {info.data.os} / {info.data.arch}
            </Mono>
          </div>
        </Card>
      </Section>
      <Section title="Appearance">
        <ThemePicker />
      </Section>
      <Section title="HTTPS">
        <HttpsCard />
      </Section>
      <Section
        title="Storage locations"
      >
        <Card>
          <ul className="divide-y">
            {rows.map((r) => (
              <li key={r.label} className="flex items-center gap-3 px-4 py-2.5">
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-muted-foreground">{r.label}</p>
                  <Mono>{r.value}</Mono>
                </div>
                {r.target && (
                  <Button size="sm" variant="outline" onClick={() => r.target && reveal(r.target)}>
                    <FolderOpenIcon /> Open
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </Card>
        <ErrorPanel error={openError} />
      </Section>
    </Page>
  );
}
