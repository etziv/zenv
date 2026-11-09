import { describe, expect, it } from "vitest";

import { ago, NO_OPTIONS, prettyModel, sameOptions, toAttachment, untilText, validModel } from "./options";

describe("chat options", () => {
  it("names models as VS Code does", () => {
    expect(prettyModel("claude-opus-5-5[1m]")).toBe("Opus 5.5 (1M)");
    expect(prettyModel("claude-haiku-5-5")).toBe("Haiku 5.5");
    expect(prettyModel("claude-sonnet-5")).toBe("Sonnet 5");
    expect(prettyModel("custom-model")).toBe("custom-model");
    expect(prettyModel(null)).toBeNull();
  });

  it("counts down to a reset", () => {
    const now = 1_000_000_000_000;
    expect(untilText(now / 1000 + 57 * 60, now)).toBe("57m");
    expect(untilText(now / 1000 + 185 * 60, now)).toBe("3h 5m");
    expect(untilText(now / 1000 - 60, now)).toBe("0m");
  });

  it("says how long ago a session changed", () => {
    const now = 1_000_000_000_000;
    expect(ago(now - 20_000, now)).toBe("just now");
    expect(ago(now - 5 * 60_000, now)).toBe("5m ago");
    expect(ago(now - 3 * 3600_000, now)).toBe("3h ago");
    expect(ago(now - 2 * 86400_000, now)).toBe("2d ago");
  });

  it("keeps only model names Claude Code takes", () => {
    expect(validModel("claude-opus-5-5[1m]")).toBe(true);
    expect(validModel("--model")).toBe(false);
    expect(validModel("a b")).toBe(false);
    expect(validModel(3)).toBe(false);
  });

  it("compares options", () => {
    const a = { ...NO_OPTIONS, model: "opus", mode: "plan" as const };
    expect(sameOptions(a, { ...a })).toBe(true);
    expect(sameOptions(a, { ...a, effort: "high" })).toBe(false);
    expect(sameOptions(a, { ...a, thinking: false })).toBe(false);
  });

  it("sends images as images, code as text and the rest as files", async () => {
    const png = await toAttachment(new File([new Uint8Array([137, 80, 78, 71])], "shot.png", { type: "image/png" }));
    expect(png).toMatchObject({ kind: "image", mediaType: "image/png", data: "iVBORw==" });
    const code = await toAttachment(new File(["<?php echo 1;"], "index.php"));
    expect(code).toMatchObject({ kind: "text", data: "<?php echo 1;" });
    const pdf = await toAttachment(new File(["%PDF"], "doc.pdf", { type: "application/pdf" }));
    expect(pdf).toMatchObject({ kind: "file", mediaType: "application/pdf", data: "JVBERg==" });
  });
});
