import { LoaderCircleIcon, RefreshCwIcon } from "lucide-react";
import { useId, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useCodexAction, useRouterModels } from "@/lib/api/queries";
import type { CodexStatus, RouterId, RouterModel } from "@/lib/api/types";

/** 1000000 → "1M", 32000 → "32K". */
function tokens(n: number | null | undefined): string {
  if (!n) return "?";
  if (n >= 1_000_000) return `${+(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1000) return `${+(n / 1000).toFixed(1)}K`;
  return String(n);
}

const toNumber = (s: string): number | null => {
  const n = Number(s.replace(/[_,.\s]/g, ""));
  return Number.isFinite(n) && n > 0 ? Math.round(n) : null;
};

/**
 * The model Codex asks the router for, chosen from the router's own list
 * (combos first). When the router is Codex's default, the context limits go
 * to config.toml too, filled in from the router when it knows them.
 */
export function CodexModelPicker({
  component,
  name,
  serviceId,
  running,
  codex,
}: {
  component: RouterId;
  name: string;
  serviceId: string;
  running: boolean;
  codex: CodexStatus;
}) {
  const [open, setOpen] = useState(false);
  const models = useRouterModels(serviceId, open && running);
  const action = useCodexAction(component, name);
  const listId = useId();
  const [model, setModel] = useState(codex.model ?? "");
  const [context, setContext] = useState(codex.contextWindow ? String(codex.contextWindow) : "");
  const [output, setOutput] = useState(codex.maxOutput ? String(codex.maxOutput) : "");
  const known = models.data?.find((m) => m.id === model.trim());

  const pick = (id: string) => {
    setModel(id);
    const m: RouterModel | undefined = models.data?.find((x) => x.id === id.trim());
    // The router's own numbers replace the old ones; unknown ones stay for the user to fill.
    if (m?.contextWindow) setContext(String(m.contextWindow));
    if (m?.maxOutput) setOutput(String(m.maxOutput));
  };

  if (!open) {
    return (
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="text-muted-foreground">
          Model{" "}
          {codex.model ? (
            <span className="font-mono text-foreground">{codex.model}</span>
          ) : (
            <span className="text-warning">not set: Codex would ask for a model {name} doesn't have</span>
          )}
          {codex.isDefault && codex.model && ` · context ${tokens(codex.contextWindow)} · output ${tokens(codex.maxOutput)}`}
        </span>
        <Button
          size="sm"
          variant="outline"
          className="h-7 px-2 text-xs"
          disabled={!running}
          title={running ? `Pick from ${name}'s models and combos` : `Start ${name} first`}
          onClick={() => {
            setModel(codex.model ?? "");
            setContext(codex.contextWindow ? String(codex.contextWindow) : "");
            setOutput(codex.maxOutput ? String(codex.maxOutput) : "");
            setOpen(true);
          }}
        >
          Choose model
        </Button>
      </div>
    );
  }

  const save = () =>
    action.mutate(
      { action: "model", model: model.trim(), contextWindow: codex.isDefault ? toNumber(context) : null, maxOutput: codex.isDefault ? toNumber(output) : null },
      { onSuccess: () => setOpen(false) },
    );

  return (
    <div className="grid gap-2 rounded-md border bg-muted/30 p-2.5">
      <div className="flex items-center gap-2">
        <label htmlFor={`${listId}-model`} className="w-16 shrink-0 text-muted-foreground">
          Model
        </label>
        <Input
          id={`${listId}-model`}
          list={listId}
          value={model}
          onChange={(e) => pick(e.target.value)}
          placeholder={models.isLoading ? "Loading models…" : "Type to search, e.g. a combo name"}
          className="h-7 font-mono text-xs"
          autoFocus
        />
        <datalist id={listId}>
          {(models.data ?? []).map((m) => (
            <option key={m.id} value={m.id}>
              {[m.combo ? "combo" : m.ownedBy, m.contextWindow && `${tokens(m.contextWindow)} context`].filter(Boolean).join(" · ")}
            </option>
          ))}
        </datalist>
        <Button size="icon" variant="ghost" className="size-7 shrink-0" title="Reload the list" disabled={models.isFetching} onClick={() => void models.refetch()}>
          {models.isFetching ? <LoaderCircleIcon className="animate-spin" /> : <RefreshCwIcon />}
        </Button>
      </div>
      <p className="pl-[4.5rem] text-[11px] text-muted-foreground">
        {models.error
          ? <span className="text-destructive">{models.error.message}</span>
          : models.data
            ? known
              ? `${known.combo ? "Combo" : `From ${known.ownedBy ?? "the router"}`} · context ${tokens(known.contextWindow)} · output ${tokens(known.maxOutput)}`
              : model.trim()
                ? `Not in ${name}'s list of ${models.data.length}; Codex will still ask for it`
                : `${models.data.filter((m) => m.combo).length} combos and ${models.data.filter((m) => !m.combo).length} models`
            : "Asking the router…"}
      </p>
      {codex.isDefault ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="w-16 shrink-0 text-muted-foreground">Limits</span>
          <label className="flex items-center gap-1.5">
            context
            <Input value={context} onChange={(e) => setContext(e.target.value)} inputMode="numeric" className="h-7 w-28 font-mono text-xs" aria-label="model_context_window" />
          </label>
          <label className="flex items-center gap-1.5">
            output
            <Input value={output} onChange={(e) => setOutput(e.target.value)} inputMode="numeric" className="h-7 w-24 font-mono text-xs" aria-label="model_max_output_tokens" />
          </label>
          <span className="text-[11px] text-muted-foreground">tokens; {name} fills these in when it knows them</span>
        </div>
      ) : (
        <p className="pl-[4.5rem] text-[11px] text-muted-foreground">
          Only the {component} profile changes. Context limits are set when {name} is Codex's default.
        </p>
      )}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => setOpen(false)}>
          Cancel
        </Button>
        <Button size="sm" className="h-7 px-2 text-xs" disabled={!model.trim() || action.isPending} onClick={save}>
          {action.isPending && <LoaderCircleIcon className="animate-spin" />} Save to config.toml
        </Button>
      </div>
    </div>
  );
}
