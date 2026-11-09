import { indentWithTab } from "@codemirror/commands";
import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { Compartment, EditorState, Prec, Text } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { useEffect, useRef, useState } from "react";

import { ApiError, api } from "@/lib/api/client";

import { FORMAT_EVENT, formatCode, formatOnSave, formatterFor } from "./format";
import { zenvEditorTheme } from "./theme";

export interface EditorStatus {
  dirty: boolean;
  readOnly: boolean;
  /** "Ln 12, Col 4". */
  cursor: string;
  language: string | null;
  /** "Prettier", "gofmt", ...; null when nothing formats this file. */
  formatter: string | null;
  formatting: boolean;
  /** Why the last format failed (a syntax error, a missing tool). */
  formatError: string | null;
  /** The formatter is not installed: "ruff", "rustfmt" or "gofmt". */
  formatMissing: string | null;
}

/** Laravel's `artisan` and friends have no extension. */
function languageFor(name: string): LanguageDescription | null {
  const lower = name.toLowerCase();
  if (lower === "artisan") return LanguageDescription.matchLanguageName(languages, "php");
  return LanguageDescription.matchFilename(languages, name);
}

/**
 * One open file. It stays mounted while its tab is hidden, keeping its undo
 * history. Ctrl+S saves; a file changed on disk (by Claude Code, say) reloads
 * by itself when it has no unsaved edits, else the user picks.
 */
export function CodeEditor({
  siteId,
  path,
  active,
  onStatus,
}: {
  siteId: string;
  path: string;
  active: boolean;
  onStatus: (s: EditorStatus) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const saved = useRef<{ doc: Text; modified: number } | null>(null);
  const statusRef = useRef(onStatus);
  statusRef.current = onStatus;
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const actions = useRef<{ save: (force?: boolean) => void; reload: () => void } | null>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let disposed = false;
    let view: EditorView | null = null;
    let readOnly = false;
    let languageName: string | null = null;
    let formatting = false;
    let formatError: string | null = null;
    let formatMissing: string | null = null;
    const formatter = formatterFor(path);
    let crlf = false;
    const language = new Compartment();

    const report = () => {
      if (!view || !saved.current) return;
      const head = view.state.selection.main.head;
      const line = view.state.doc.lineAt(head);
      statusRef.current({
        dirty: !view.state.doc.eq(saved.current.doc),
        readOnly,
        cursor: `Ln ${line.number}, Col ${head - line.from + 1}`,
        language: languageName,
        formatter,
        formatting,
        formatError,
        formatMissing,
      });
    };

    /** Formats the document; resolves once done (or failed, reported). */
    const format = async () => {
      if (!view || !formatter || readOnly || formatting) return;
      const before = view.state.doc.toString();
      formatting = true;
      formatError = null;
      formatMissing = null;
      report();
      try {
        const r = await formatCode(siteId, path, before, view.state.selection.main.head);
        // The editor splits lines on CRLF only in a CRLF file.
        const text = crlf ? r.formatted.replace(/\r?\n/g, "\r\n") : r.formatted;
        if (!disposed && view && view.state.doc.toString() === before && text !== before) {
          view.dispatch({
            changes: { from: 0, to: view.state.doc.length, insert: text },
            selection: { anchor: Math.min(r.cursorOffset, text.length) },
            scrollIntoView: true,
          });
        }
      } catch (e) {
        const d = e instanceof ApiError ? e.details : null;
        if (e instanceof ApiError && e.code === "not_found" && d && "entity" in d && d.entity === "formatter") {
          formatMissing = d.id;
          formatError = `${d.id} isn't installed`;
        } else {
          formatError = e instanceof Error ? e.message : String(e);
        }
      } finally {
        formatting = false;
        report();
      }
    };

    const save = async (force = false) => {
      if (!view || !saved.current || readOnly) return;
      if (formatOnSave() && formatter) await format();
      if (!view || !saved.current) return;
      const doc = view.state.doc;
      const text = doc.toString();
      api
        .projectWrite(siteId, path, text, force ? null : saved.current.modified)
        .then((modified) => {
          saved.current = { doc, modified };
          setConflict(false);
          setSaveError(null);
          report();
        })
        .catch((e: unknown) => {
          if (e instanceof ApiError && e.code === "conflict") setConflict(true);
          else setSaveError(e instanceof Error ? e.message : String(e));
        });
    };

    const replaceWith = (content: string, modified: number) => {
      if (!view) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: content } });
      saved.current = { doc: view.state.doc, modified };
      setConflict(false);
      report();
    };

    const reload = () => {
      api.projectRead(siteId, path).then((f) => replaceWith(f.content, f.modified), (e: Error) => setError(e.message));
    };
    actions.current = { save: (force) => void save(force), reload };
    const onFormatEvent = (e: Event) => {
      if ((e as CustomEvent<string>).detail === path) void format();
    };
    window.addEventListener(FORMAT_EVENT, onFormatEvent);

    api
      .projectRead(siteId, path)
      .then((file) => {
        if (disposed) return;
        readOnly = file.readOnly;
        // Keep Windows line endings as they are.
        crlf = file.content.includes("\r\n");
        const state = EditorState.create({
          doc: file.content,
          extensions: [
            basicSetup,
            zenvEditorTheme,
            language.of([]),
            keymap.of([indentWithTab]),
            Prec.highest(
              keymap.of([
                {
                  key: "Mod-s",
                  preventDefault: true,
                  run: () => {
                    void save();
                    return true;
                  },
                },
                // As in VS Code.
                {
                  key: "Shift-Alt-f",
                  preventDefault: true,
                  run: () => {
                    void format();
                    return true;
                  },
                },
              ]),
            ),
            crlf ? EditorState.lineSeparator.of("\r\n") : [],
            EditorState.readOnly.of(readOnly),
            EditorView.updateListener.of((u) => {
              if (u.docChanged || u.selectionSet) report();
            }),
          ],
        });
        saved.current = { doc: state.doc, modified: file.modified };
        view = new EditorView({ state, parent: el });
        viewRef.current = view;
        report();
        const lang = languageFor(path.split("/").pop() ?? path);
        if (lang) {
          languageName = lang.name;
          lang
            .load()
            .then((support) => {
              if (!disposed && view) view.dispatch({ effects: language.reconfigure(support) });
              report();
            })
            .catch(() => {});
        }
      })
      .catch((e: Error) => !disposed && setError(e.message));

    return () => {
      disposed = true;
      window.removeEventListener(FORMAT_EVENT, onFormatEvent);
      view?.destroy();
      viewRef.current = null;
      actions.current = null;
    };
  }, [siteId, path]);

  // Notices outside edits while this file is in front.
  useEffect(() => {
    if (!active) return;
    viewRef.current?.focus();
    let stop = false;
    const check = () => {
      const s = saved.current;
      const view = viewRef.current;
      if (stop || !s || !view || document.visibilityState !== "visible") return;
      api
        .projectStat(siteId, [path])
        .then(([modified]) => {
          if (stop || modified == null || modified === s.modified || saved.current !== s) return;
          if (view.state.doc.eq(s.doc)) actions.current?.reload();
          else setConflict(true);
        })
        .catch(() => {});
    };
    check();
    const id = window.setInterval(check, 1500);
    return () => {
      stop = true;
      window.clearInterval(id);
    };
  }, [active, siteId, path]);

  if (error) {
    return <div className="grid h-full place-items-center p-6 text-center text-sm text-panel-muted">{error}</div>;
  }
  return (
    <div className="flex h-full flex-col">
      {conflict && (
        <div className="flex shrink-0 items-center gap-3 border-b border-panel-line bg-[#2a2210] px-3 py-1.5 text-[12px] text-[#f3d27a]">
          <span className="min-w-0 flex-1">This file changed on disk (Claude Code or another program edited it) while you had unsaved changes.</span>
          <button type="button" className="rounded border border-current px-2 py-0.5 hover:bg-white/5" onClick={() => actions.current?.reload()}>
            Load theirs
          </button>
          <button type="button" className="rounded border border-current px-2 py-0.5 hover:bg-white/5" onClick={() => actions.current?.save(true)}>
            Keep mine
          </button>
        </div>
      )}
      {saveError && (
        <div className="shrink-0 border-b border-panel-line bg-[#2a1210] px-3 py-1.5 text-[12px] text-led-fail">Couldn't save: {saveError}</div>
      )}
      <div ref={host} className="min-h-0 flex-1 overflow-hidden" />
    </div>
  );
}
