import { describe, expect, it } from "vitest";

import { ApiError, toApiError } from "./client";

describe("toApiError", () => {
  it("keeps the backend's code, message and details", () => {
    const err = toApiError(
      { code: "port_unavailable", message: "port 80 is not available", details: { port: 80, owners: [], reason: null } },
      "service_start",
    );
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe("port_unavailable");
    expect(err.details).toEqual({ port: 80, owners: [], reason: null });
  });

  it("wraps unstructured failures as ipc errors", () => {
    const err = toApiError("boom", "services_list");
    expect(err.code).toBe("ipc");
    // jsdom has no Tauri internals, so the message explains the missing shell.
    expect(err.message).toContain("outside the desktop app");
  });
});
