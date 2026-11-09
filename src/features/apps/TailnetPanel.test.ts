import { describe, expect, it } from "vitest";

import type { TailnetStatus } from "@/lib/api/types";

import { serviceNameFor, serviceProblem, validService } from "./TailnetPanel";

const st: TailnetStatus = { installed: true, online: true, host: "pc", dnsName: "pc.tail1.ts.net", ip: "100.1.2.3", problem: null, tags: [], https: false };

describe("Tailscale Service names", () => {
  it("come from the app's domain as a DNS label", () => {
    expect(serviceNameFor("vaultwarden.localhost")).toBe("vaultwarden");
    expect(serviceNameFor("Wa_Blast.test")).toBe("wa-blast");
    expect(serviceNameFor("___.test")).toBe("app");
  });

  it("accept DNS labels only", () => {
    expect(validService("vaultwarden")).toBe(true);
    expect(validService("wa-blast2")).toBe(true);
    for (const bad of ["", "-x", "x-", "a.b", "Vault", "svc:x"]) expect(validService(bad)).toBe(false);
  });

  it("say what the computer still needs", () => {
    expect(serviceProblem(st)).toContain("Tag this computer");
    expect(serviceProblem({ ...st, tags: ["tag:server"] })).toContain("HTTPS certificates");
    expect(serviceProblem({ ...st, tags: ["tag:server"], https: true })).toBeNull();
    expect(serviceProblem({ ...st, problem: "Tailscale is not connected" })).toBe("Tailscale is not connected");
  });
});
