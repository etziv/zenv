import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { Markdown } from "./Markdown";

const html = (text: string) => renderToStaticMarkup(<Markdown text={text} />);

describe("Markdown", () => {
  it("draws code blocks, lists, tables and inline marks", () => {
    const out = html("# Title\n\nUse **bold**, *it* and `code`.\n\n- one\n- two\n\n```ts\nconst a = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |");
    expect(out).toContain("Title");
    expect(out).toContain("<strong");
    expect(out).toContain("<em>it</em>");
    expect(out).toMatch(/<code[^>]*>code<\/code>/);
    expect(out.match(/<li/g)).toHaveLength(2);
    expect(out).toContain("const a = 1;");
    expect(out).toContain("<table");
    expect(out).not.toContain("---");
  });

  it("never injects HTML or navigates", () => {
    const out = html('<img src=x onerror=alert(1)> [site](https://example.com)');
    expect(out).not.toContain("<img");
    expect(out).not.toContain("href");
    expect(out).toContain('title="https://example.com"');
  });

  it("keeps an unclosed code block while it streams", () => {
    expect(html("```\npartial")).toContain("partial");
  });
});
