import type { ServiceStatus } from "@/lib/api/types";

/** Numeric version compare, newest first ("9.7.0" before "9.3.0" before "8.0.30"). */
export function compareVersionsDesc(a: string, b: string): number {
  return b.localeCompare(a, undefined, { numeric: true });
}

/** Services grouped by product, newest version first within each. */
export function sortServices(list: ServiceStatus[]): ServiceStatus[] {
  return [...list].sort(
    (a, b) =>
      a.service.component.localeCompare(b.service.component) ||
      compareVersionsDesc(a.installation?.version ?? "", b.installation?.version ?? "") ||
      a.service.name.localeCompare(b.service.name),
  );
}
