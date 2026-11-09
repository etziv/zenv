import { LoaderCircleIcon, TerminalSquareIcon, TriangleAlertIcon, XIcon } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { usePathAction, usePathStatus } from "@/lib/api/queries";
import type { ComponentId, Installation } from "@/lib/api/types";

/**
 * Other folders on PATH that already hold this component's programs. Windows
 * runs the first one it finds, so Zenv asks for them to be removed first.
 */
export function PathConflicts({ component }: { component: ComponentId }) {
  const status = usePathStatus(component);
  const list = status.data?.conflicts ?? [];
  if (list.length === 0) return null;
  return (
    <div className="flex gap-2 border-b bg-warning/5 px-4 py-2.5 text-xs">
      <TriangleAlertIcon className="mt-px size-3.5 shrink-0 text-warning" />
      <div className="min-w-0 space-y-1">
        <p>
          Already on PATH from another folder, so no version here can be added to PATH yet. Remove{" "}
          {list.length === 1 ? "it" : "them"} in System Properties → Environment Variables:
        </p>
        <ul className="space-y-0.5">
          {list.map((c) => (
            <li key={`${c.scope}:${c.entry}`} className="font-mono text-[11px]">
              {c.entry} <span className="text-muted-foreground">({c.scope} PATH, {c.program})</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/** "On PATH" badge and the button to put this version on (or take it off) the user PATH. */
export function PathButton({ inst }: { inst: Installation }) {
  const status = usePathStatus(inst.component);
  const action = usePathAction();
  if (!status.data?.eligible) return null;
  const onPath = status.data.installationId === inst.id;
  const pending = action.isPending && action.variables?.component === inst.component;
  if (onPath) {
    return (
      <span className="flex shrink-0 items-center gap-1">
        <Badge variant="success" title="New terminals use this version">
          PATH
        </Badge>
        <Button
          size="icon"
          variant="ghost"
          className="size-7"
          disabled={pending}
          title="Take this version off PATH"
          aria-label={`Take ${inst.version} off PATH`}
          onClick={() => action.mutate({ action: "remove", component: inst.component, label: inst.version })}
        >
          {pending ? <LoaderCircleIcon className="animate-spin" /> : <XIcon />}
        </Button>
      </span>
    );
  }
  const blocked = status.data.conflicts.length > 0;
  // A disabled button shows no tooltip, so the wrapper carries the reason.
  return (
    <span
      className="shrink-0"
      title={
        blocked
          ? `Remove ${status.data.conflicts.map((c) => c.entry).join(", ")} from PATH first (see the notice above)`
          : "New terminals and editors use this version"
      }
    >
      <Button
        size="sm"
        variant="outline"
        className="h-7 px-2 text-xs"
        disabled={pending || blocked || !inst.executableExists}
        onClick={() => action.mutate({ action: "add", component: inst.component, installationId: inst.id, label: inst.version })}
      >
        {pending ? <LoaderCircleIcon className="animate-spin" /> : <TerminalSquareIcon />} Add to PATH
      </Button>
    </span>
  );
}
