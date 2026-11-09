import type * as React from "react";
import { memo } from "react";

/**
 * The Markdown Claude writes, drawn without a library: code blocks, headings,
 * lists, quotes, tables, rules and inline code, bold, italics and links.
 * Everything becomes React elements (no HTML is injected). Links show their
 * address on hover instead of navigating Zenv's window away.
 */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return <div className="grid gap-2 break-words">{blocks(text)}</div>;
});

function blocks(text: string): React.ReactNode[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const out: React.ReactNode[] = [];
  let i = 0;
  const at = (n: number) => lines[n] ?? "";
  const key = () => out.length;
  while (i < lines.length) {
    const line = at(i);
    const fence = /^\s*(```|~~~)\s*([\w+-]*)/.exec(line);
    if (fence) {
      const body: string[] = [];
      i++;
      const close = fence[1] ?? "```";
      while (i < lines.length && !at(i).trimStart().startsWith(close)) body.push(at(i++));
      i++;
      out.push(
        <pre key={key()} className="overflow-x-auto rounded-md border border-panel-line bg-black/40 p-2.5 font-mono text-[11.5px] leading-relaxed text-panel-foreground">
          {fence[2] && <span className="mb-1 block text-[10px] uppercase tracking-wide text-panel-muted">{fence[2]}</span>}
          <code>{body.join("\n")}</code>
        </pre>,
      );
      continue;
    }
    if (!line.trim()) {
      i++;
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const size = (heading[1] ?? "").length <= 2 ? "text-[14px]" : "text-[13px]";
      out.push(
        <p key={key()} className={`${size} mt-1 font-semibold text-white`}>
          {inline(heading[2] ?? "")}
        </p>,
      );
      i++;
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      out.push(<hr key={key()} className="border-panel-line" />);
      i++;
      continue;
    }
    if (/^\s*\|.*\|\s*$/.test(line)) {
      const rows: string[][] = [];
      while (i < lines.length && /^\s*\|.*\|\s*$/.test(at(i))) {
        const cells = at(i).trim().slice(1, -1).split("|").map((c) => c.trim());
        if (!cells.every((c) => /^:?-+:?$/.test(c))) rows.push(cells);
        i++;
      }
      const [head = [], ...body] = rows;
      out.push(
        <div key={key()} className="overflow-x-auto">
          <table className="text-[12px]">
            <thead>
              <tr>
                {head.map((c, j) => (
                  <th key={j} className="border-b border-panel-line px-2 py-1 text-left font-semibold text-white">
                    {inline(c)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {body.map((r, j) => (
                <tr key={j}>
                  {r.map((c, k) => (
                    <td key={k} className="border-b border-panel-line/60 px-2 py-1 align-top">
                      {inline(c)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }
    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>/.test(at(i))) body.push(at(i++).replace(/^\s*>\s?/, ""));
      out.push(
        <blockquote key={key()} className="border-l-2 border-panel-line pl-3 text-panel-muted">
          {blocks(body.join("\n"))}
        </blockquote>,
      );
      continue;
    }
    const item = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(line);
    if (item) {
      const ordered = /\d/.test(item[2] ?? "");
      const entries: React.ReactNode[] = [];
      while (i < lines.length) {
        const m = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(at(i));
        if (m && /\d/.test(m[2] ?? "") === ordered) {
          entries.push(
            <li key={entries.length} style={{ marginLeft: Math.min((m[1] ?? "").length, 8) * 6 }}>
              {inline(m[3] ?? "")}
            </li>,
          );
          i++;
        } else if (at(i).trim() && /^\s{2,}/.test(at(i)) && entries.length) {
          // A wrapped continuation of the item above.
          entries.push(
            <div key={entries.length} className="pl-1">
              {inline(at(i).trim())}
            </div>,
          );
          i++;
        } else break;
      }
      out.push(
        ordered ? (
          <ol key={key()} className="grid list-decimal gap-0.5 pl-5">
            {entries}
          </ol>
        ) : (
          <ul key={key()} className="grid list-disc gap-0.5 pl-5">
            {entries}
          </ul>
        ),
      );
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && at(i).trim() && !/^(\s*(```|~~~|>|\||[-*+]\s|\d+[.)]\s)|#{1,6}\s)/.test(at(i))) para.push(at(i++));
    if (!para.length) para.push(at(i++));
    out.push(
      <p key={key()} className="whitespace-pre-wrap">
        {inline(para.join("\n"))}
      </p>,
    );
  }
  return out;
}

const INLINE = /(`+)([^`]+?)\1|\*\*(.+?)\*\*|__(.+?)__|(?<![\w*])\*(?!\s)(.+?)\*(?!\w)|(?<!\w)_(?!\s)(.+?)_(?!\w)|\[([^\]]+)\]\(([^)\s]+)\)/g;

function inline(text: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const k = out.length;
    if (m[2] !== undefined) {
      out.push(
        <code key={k} className="rounded bg-panel-raised px-1 py-px font-mono text-[0.92em] text-led">
          {m[2]}
        </code>,
      );
    } else if (m[3] !== undefined || m[4] !== undefined) {
      out.push(
        <strong key={k} className="font-semibold text-white">
          {inline(m[3] ?? m[4] ?? "")}
        </strong>,
      );
    } else if (m[5] !== undefined || m[6] !== undefined) {
      out.push(<em key={k}>{inline(m[5] ?? m[6] ?? "")}</em>);
    } else {
      out.push(
        <span key={k} title={m[8]} className="text-led underline decoration-led/40 underline-offset-2">
          {m[7]}
        </span>,
      );
    }
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}
