import { existsSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { fileIcon, folderIcon, ICONS_USED } from "./fileIcons";

describe("file icons", () => {
  it("picks by whole name, then the longest extension", () => {
    expect(fileIcon("composer.json")).toBe("/file-icons/php.svg");
    expect(fileIcon("Dockerfile.dev")).toBe("/file-icons/docker.svg");
    expect(fileIcon(".env.staging")).toBe("/file-icons/tune.svg");
    expect(fileIcon("welcome.blade.php")).toBe("/file-icons/laravel.svg");
    expect(fileIcon("index.php")).toBe("/file-icons/php.svg");
    expect(fileIcon("types.d.ts")).toBe("/file-icons/typescript-def.svg");
    expect(fileIcon("App.TSX")).toBe("/file-icons/react_ts.svg");
    expect(fileIcon("main.go")).toBe("/file-icons/go.svg");
    expect(fileIcon("Cargo.toml")).toBe("/file-icons/rust.svg");
    expect(fileIcon("notes")).toBe("/file-icons/file.svg");
    expect(fileIcon("archive.unknownext")).toBe("/file-icons/file.svg");
  });

  it("gives known folders their own icon, open and closed", () => {
    expect(folderIcon("src", false)).toBe("/file-icons/folder-src.svg");
    expect(folderIcon("node_modules", true)).toBe("/file-icons/folder-node-open.svg");
    expect(folderIcon("Routes", false)).toBe("/file-icons/folder-routes.svg");
    expect(folderIcon("whatever", true)).toBe("/file-icons/folder-open.svg");
  });

  it("ships every icon it names", () => {
    const dir = join(process.cwd(), "public", "file-icons");
    const missing = ICONS_USED.filter((i) => !existsSync(join(dir, `${i}.svg`)));
    expect(missing).toEqual([]);
    expect(existsSync(join(dir, "LICENSE"))).toBe(true);
  });
});
