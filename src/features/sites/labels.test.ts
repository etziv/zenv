import { describe, expect, it } from "vitest";

import { hostsProblem, statusBadge } from "./labels";

describe("site labels", () => {
  it("maps configuration status to badges", () => {
    expect(statusBadge("applied")).toEqual({ label: "Active", variant: "success" });
    expect(statusBadge("failed").variant).toBe("destructive");
  });

  it("only flags hosts states that block resolution", () => {
    expect(hostsProblem("not_needed")).toBeNull();
    expect(hostsProblem("managed")).toBeNull();
    expect(hostsProblem("external")).toBeNull();
    expect(hostsProblem("missing")).toMatch(/hosts file/);
    expect(hostsProblem("conflict")).toMatch(/elsewhere/);
  });
});
