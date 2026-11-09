import { useState } from "react";

import { Page, PageHeader } from "@/components/page";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

import { Diagnostics, PortCheck } from "./Diagnostics";
import { ProjectLogs } from "./ProjectLogs";
import { type ServiceLogTarget, ServiceLogs } from "./ServiceLogs";

type Tab = "projects" | "services" | "diagnostics" | "ports";

export function LogsPage() {
  const [tab, setTab] = useState<Tab>("projects");
  const [serviceTarget, setServiceTarget] = useState<ServiceLogTarget>({ type: "app" });

  return (
    <Page>
      <PageHeader title="Logs" />
      <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)}>
        <TabsList>
          <TabsTrigger value="projects">Projects</TabsTrigger>
          <TabsTrigger value="services">Services</TabsTrigger>
          <TabsTrigger value="diagnostics">Diagnostics</TabsTrigger>
          <TabsTrigger value="ports">Port inspector</TabsTrigger>
        </TabsList>
        <TabsContent value="projects">
          <ProjectLogs
            onShowService={(id) => {
              setServiceTarget({ type: "service", id });
              setTab("services");
            }}
          />
        </TabsContent>
        <TabsContent value="services">
          <ServiceLogs target={serviceTarget} onSelect={setServiceTarget} />
        </TabsContent>
        <TabsContent value="diagnostics">
          <Diagnostics />
        </TabsContent>
        <TabsContent value="ports">
          <PortCheck />
        </TabsContent>
      </Tabs>
    </Page>
  );
}
