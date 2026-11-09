import { ChevronDownIcon, CodeIcon, SparklesIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { api } from "@/lib/api/client";
import { useEditors, useOpenInEditor } from "@/lib/api/queries";
import type { EditorInfo, SiteView } from "@/lib/api/types";
import { toast } from "@/lib/toast";

export function EditorIcon({ editor }: { editor: EditorInfo | undefined }) {
  if (!editor?.icon) return <CodeIcon />;
  return <img src={editor.icon} alt="" className="size-4 shrink-0" draggable={false} />;
}

/** The editor "Open in editor" uses, if any is installed. */
export function usePreferredEditor() {
  const editors = useEditors();
  const list = editors.data?.editors ?? [];
  return { list, preferred: list.find((e) => e.id === editors.data?.preferred) ?? list[0] };
}

/** Zenv's own Studio (the wide button) and "VS Code" (with a menu of the other editors). */
export function EditButtons({ site }: { site: SiteView }) {
  const { list, preferred } = usePreferredEditor();
  const open = useOpenInEditor();
  const studio = () =>
    api.studioOpen(site.id).catch((e: Error) => toast({ variant: "error", title: "Couldn't open Studio", description: e.message }));

  return (
    <div className="flex gap-2">
      <Button
        variant="secondary"
        className="min-w-0 flex-1"
        title="Zenv's editor for this app: its files, Claude Code and the live preview"
        onClick={studio}
      >
        <SparklesIcon /> <span className="truncate">Open in Studio</span>
      </Button>
      <div className="flex max-w-[45%] shrink-0">
        <Button
          variant="secondary"
          className="min-w-0 rounded-r-none"
          disabled={!preferred || open.isPending}
          title={preferred ? `Open ${site.projectRoot} in ${preferred.name}` : "No supported editor is installed (VS Code, Cursor, PhpStorm, ...)"}
          onClick={() => preferred && open.mutate({ siteId: site.id, editor: preferred.id })}
        >
          <EditorIcon editor={preferred} />
          <span className="truncate">{preferred ? preferred.name : "No editor"}</span>
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="secondary" size="icon" className="w-7 rounded-l-none border-l-0" aria-label="Choose editor">
              <ChevronDownIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <p className="px-2 py-1 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Open in</p>
            {list.map((e) => (
              <DropdownMenuItem key={e.id} title={e.program} onSelect={() => open.mutate({ siteId: site.id, editor: e.id })}>
                <EditorIcon editor={e} /> {e.name}
                {e.id === preferred?.id && <span className="ml-auto pl-4 text-[11px] text-muted-foreground">default</span>}
              </DropdownMenuItem>
            ))}
            {list.length === 0 && (
              <DropdownMenuItem disabled>No editor found: VS Code, Cursor, Windsurf, Zed, PhpStorm, Sublime…</DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={studio}>
              <SparklesIcon /> Studio: editor, Claude Code, preview
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
