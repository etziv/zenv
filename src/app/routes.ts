export type RouteId =
  | "dashboard"
  | "sites"
  | "web-servers"
  | "languages"
  | "databases"
  | "cache"
  | "tools"
  | "logs"
  | "settings";

export interface RouteDef {
  id: RouteId;
  label: string;
}

/** Header navigation, grouped by what the pages are for. */
export const NAV_GROUPS: { label: string; routes: RouteDef[] }[] = [
  { label: "Overview", routes: [{ id: "dashboard", label: "Dashboard" }] },
  {
    label: "Serve",
    routes: [
      { id: "sites", label: "Apps" },
      { id: "web-servers", label: "Web Servers" },
    ],
  },
  {
    label: "Runtimes",
    routes: [
      { id: "languages", label: "Languages" },
      { id: "databases", label: "Databases" },
      { id: "cache", label: "Cache" },
      { id: "tools", label: "Tools" },
    ],
  },
  { label: "System", routes: [{ id: "logs", label: "Logs" }] },
];
