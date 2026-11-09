import { useState } from "react";

import { AppShell } from "@/app/AppShell";
import { NavigationContext } from "@/app/navigation";
import type { RouteId } from "@/app/routes";
import { ComponentsPage } from "@/features/runtimes/ComponentsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { AppsPage } from "@/features/apps/AppsPage";

function Screen({ route }: { route: RouteId }) {
  switch (route) {
    case "dashboard":
      return <DashboardPage />;
    case "sites":
      return <AppsPage />;
    case "web-servers":
      return <ComponentsPage category="web_server" title="Web Servers" />;
    case "languages":
      return <ComponentsPage category="language" title="Languages" />;
    case "databases":
      return <ComponentsPage category="database" title="Databases" />;
    case "cache":
      return <ComponentsPage category="cache" title="Cache & Queue" />;
    case "tools":
      return <ComponentsPage category="tool" title="Tools" />;
    case "logs":
      return <LogsPage />;
    case "settings":
      return <SettingsPage />;
    default: {
      const unreachable: never = route;
      throw new Error(`unknown route ${String(unreachable)}`);
    }
  }
}

export function App() {
  const [route, setRoute] = useState<RouteId>("dashboard");
  return (
    <NavigationContext.Provider value={setRoute}>
      <AppShell route={route} onNavigate={setRoute}>
        <Screen route={route} />
      </AppShell>
    </NavigationContext.Provider>
  );
}
