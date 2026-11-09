import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api/client", () => ({
  api: {
    projectRead: vi.fn(() => Promise.reject(new Error("missing"))),
    projectFormat: vi.fn((_s: string, _p: string, text: string) => Promise.resolve(`fmt:${text}`)),
  },
}));

import { formatCode, formatterFor, prettierParser } from "./format";

describe("formatters", () => {
  it("picks one per language", () => {
    expect(formatterFor("app/Http/Kernel.php")).toBe("Prettier");
    expect(formatterFor("resources/views/home.blade.php")).toBeNull();
    expect(formatterFor("cmd/server/main.go")).toBe("gofmt");
    expect(formatterFor("src/lib.rs")).toBe("rustfmt");
    expect(formatterFor("app/main.py")).toBe("ruff");
    expect(prettierParser("App.tsx")).toBe("typescript");
    expect(prettierParser("package.json")).toBe("json-stringify");
    expect(formatterFor(".env")).toBeNull();
  });

  it("formats PHP and TypeScript with Prettier, keeping CRLF", async () => {
    const php = await formatCode("s", "a.php", "<?php\r\nfunction  f( $a,$b ){return $a+$b;}\r\n", 0);
    expect(php.formatted).toBe("<?php\r\nfunction f($a, $b)\r\n{\r\n    return $a + $b;\r\n}\r\n");
    const ts = await formatCode("s", "a.ts", "const x = {a:1,b:[1,2]}\n", 0);
    expect(ts.formatted).toBe("const x = { a: 1, b: [1, 2] };\n");
  });

  it("sends Go, Rust and Python to Zenv", async () => {
    expect((await formatCode("s", "main.go", "package main", 0)).formatted).toBe("fmt:package main");
  });
});
