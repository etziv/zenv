import { describe, expect, it } from "vitest";

import type { Installation } from "@/lib/api/types";

import { embeddedPhp, minorOf, resolveBackend } from "./php";

function inst(component: Installation["component"], version: string, versionDetail = "", over: Partial<Installation> = {}): Installation {
  return {
    id: `${component}-${version}`,
    component,
    version,
    versionDetail,
    rootPath: "C:\\x",
    executablePath: "C:\\x\\x.exe",
    source: "external",
    isDefault: false,
    enabled: true,
    executableExists: true,
    detectedAt: "",
    createdAt: "",
    updatedAt: "",
    ...over,
  };
}

const franken85 = inst("frankenphp", "1.12.2", "FrankenPHP 1.12.2 PHP 8.5.6 Caddy v2.11.2 h1:abc");

describe("embeddedPhp", () => {
  it("reads the PHP version, not FrankenPHP's own", () => {
    expect(embeddedPhp(franken85.versionDetail)).toBe("8.5.6");
    expect(minorOf("8.4.21")).toBe("8.4");
  });
});

describe("resolveBackend", () => {
  it("routes only the embedded PHP minor to FrankenPHP on auto", () => {
    expect(resolveBackend("auto", inst("php", "8.5.1"), [franken85]).backend).toBe("frankenphp");
    expect(resolveBackend("auto", inst("php", "8.4.21"), [franken85]).backend).toBe("fastcgi");
    expect(resolveBackend("fastcgi", inst("php", "8.5.1"), [franken85]).backend).toBe("fastcgi");
  });

  it("ignores switched-off FrankenPHP and explains forced failures", () => {
    const off = { ...franken85, enabled: false };
    expect(resolveBackend("auto", inst("php", "8.5.1"), [off]).backend).toBe("fastcgi");
    const r = resolveBackend("frankenphp", inst("php", "8.4.2"), [franken85]);
    expect(r.backend).toBeNull();
    expect("error" in r && r.error).toMatch(/8\.4/);
  });
});
