import { describe, expect, it } from "vitest";

import { folderName, relativeDocRoot } from "./paths";

describe("relativeDocRoot", () => {
  it("expresses document roots relative to the project", () => {
    expect(relativeDocRoot("D:\\Projects\\shop", "D:\\Projects\\shop\\public")).toBe("public");
    expect(relativeDocRoot("D:\\Projects\\shop\\", "d:\\projects\\shop")).toBe(".");
    expect(relativeDocRoot("D:\\Projects\\shop", "D:\\Projects\\shop\\dist\\app")).toBe("dist\\app");
  });

  it("keeps unrelated paths absolute", () => {
    expect(relativeDocRoot("D:\\Projects\\shop", "D:\\Projects\\shopping\\public")).toBe("D:\\Projects\\shopping\\public");
  });
});

describe("folderName", () => {
  it("returns the last segment", () => {
    expect(folderName("D:\\Projects\\my-laravel-app\\")).toBe("my-laravel-app");
  });
});
