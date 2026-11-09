import { describe, expect, it } from "vitest";

import type { ServiceStatus } from "@/lib/api/types";

import { sortServices } from "./services";

const svc = (component: string, version: string, name: string) =>
  ({ service: { component, name }, installation: { version } }) as unknown as ServiceStatus;

describe("sortServices", () => {
  it("lists each product newest version first", () => {
    const sorted = sortServices([
      svc("mysql", "8.0.30", "MySQL 8.0"),
      svc("postgresql", "9.2.24", "PostgreSQL 9.2"),
      svc("mysql", "9.7.0", "MySQL 9.7"),
      svc("mysql", "9.3.0", "MySQL 9.3"),
    ]);
    expect(sorted.map((s) => s.service.name)).toEqual(["MySQL 9.7", "MySQL 9.3", "MySQL 8.0", "PostgreSQL 9.2"]);
  });
});
