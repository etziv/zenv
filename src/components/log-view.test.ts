import { describe, expect, it } from "vitest";

import { lineLevel } from "./log-view";

describe("lineLevel", () => {
  it("colors access lines by HTTP status", () => {
    expect(lineLevel('127.0.0.1 - - [19/Oct/2026:10:21:05 +0700] "GET /x HTTP/1.1" 500 12')).toBe("error");
    expect(lineLevel('127.0.0.1 - - [19/Oct/2026:10:21:05 +0700] "GET /favicon.ico HTTP/1.1" 404 0')).toBe("warning");
    expect(lineLevel('127.0.0.1 - - [19/Oct/2026:10:21:05 +0700] "GET /error-page HTTP/1.1" 200 5')).toBe("plain");
  });

  it("colors server and PHP messages by severity", () => {
    expect(lineLevel("PHP Fatal error:  Uncaught Error: x")).toBe("error");
    expect(lineLevel("[2026-10-19 10:00:00] local.ERROR: SQLSTATE")).toBe("error");
    expect(lineLevel("PHP Warning:  Undefined variable $user")).toBe("warning");
    expect(lineLevel("[Mon Oct 19] [mpm_winnt:notice] AH00418: Parent: Created child process")).toBe("warning");
    expect(lineLevel("listening on 127.0.0.1:8080")).toBe("plain");
  });
});
