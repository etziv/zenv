import { useQueryClient } from "@tanstack/react-query";
import { LoaderCircleIcon, RefreshCwIcon } from "lucide-react";
import { Fragment } from "react";
import { useEffect } from "react";

import { Badge } from "@/components/ui/badge";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { api } from "@/lib/api/client";
import { useThumbnail } from "@/lib/api/queries";
import type { SiteView } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { PROJECT_TYPE_LABEL } from "../sites/labels";
import { type PanelAction, useAppActions } from "./app-actions";
import { capturePreview, previewState, usePreviewQueue } from "./previews";

/** Placeholder until a screenshot exists: the app's initial on a quiet tile. */
function Placeholder({ site, note }: { site: SiteView; note: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1.5 bg-muted/60">
      <span className="font-display text-3xl font-semibold uppercase text-muted-foreground/70">
        {site.name.slice(0, 1)}
      </span>
      <span className="px-3 text-center text-[11px] text-muted-foreground">
        {note}
      </span>
    </div>
  );
}

export function AppCard({
  site,
  selected,
  serving,
  removing = false,
  onSelect,
  onAction,
}: {
  site: SiteView;
  selected: boolean;
  /** A web server that serves this app is running. */
  serving: boolean;
  /** Being removed right now. */
  removing?: boolean;
  onSelect: () => void;
  onAction: (a: PanelAction) => void;
}) {
  const qc = useQueryClient();
  const thumb = useThumbnail(site.id);
  usePreviewQueue();
  const preview = previewState(site.id);
  const menu = useAppActions(site, onAction);

  useEffect(() => {
    if (
      thumb.data === null &&
      serving &&
      site.enabled &&
      !preview.triedAutomatically
    ) {
      capturePreview(qc, site.id);
    }
  }, [
    thumb.data,
    serving,
    site.enabled,
    site.id,
    preview.triedAutomatically,
    qc,
  ]);

  const failed = site.configStatus === "failed";
  const note = !site.enabled
    ? "Switched off"
    : preview.capturing
      ? "Taking a preview…"
      : serving
        ? preview.error
          ? "No preview"
          : PROJECT_TYPE_LABEL[site.projectType]
        : "Start a web server to preview";

  return (
    <li>
      <ContextMenu onOpenChange={(open) => open && onSelect()}>
        <ContextMenuTrigger asChild disabled={removing}>
          <div
            role="button"
            tabIndex={0}
            aria-pressed={selected}
            onClick={onSelect}
            onKeyDown={(e) =>
              (e.key === "Enter" || e.key === " ") &&
              (e.preventDefault(), onSelect())
            }
            aria-busy={removing || undefined}
            className={cn(
              "group flex cursor-pointer flex-col gap-2 rounded-xl border bg-card p-2 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              selected
                ? "border-primary/60 bg-primary/[0.06]"
                : "hover:border-primary/30",
              removing && "pointer-events-none opacity-60",
            )}
          >
            <div
              className={cn(
                "relative aspect-[16/10] overflow-hidden rounded-lg border bg-background",
                !site.enabled && "opacity-50 grayscale",
              )}
            >
              {thumb.data ? (
                <img
                  src={thumb.data.dataUrl}
                  alt={`Preview of ${site.domain}`}
                  className="h-full w-full object-cover object-top"
                  draggable={false}
                />
              ) : (
                <Placeholder site={site} note={note} />
              )}
              {preview.capturing && thumb.data ? (
                <span className="absolute top-1.5 right-1.5 rounded-full bg-background/80 p-1">
                  <LoaderCircleIcon className="size-3.5 animate-spin" />
                </span>
              ) : (
                serving &&
                site.enabled &&
                !removing && (
                  <button
                    type="button"
                    title="Refresh preview"
                    aria-label={`Refresh the preview of ${site.domain}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      capturePreview(qc, site.id, { force: true });
                    }}
                    className="absolute top-1.5 right-1.5 rounded-full border bg-background/85 p-1 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100"
                  >
                    <RefreshCwIcon className="size-3.5" />
                  </button>
                )
              )}
              {removing && (
                <span className="absolute inset-0 flex items-center justify-center gap-2 bg-background/70 text-xs font-medium">
                  <LoaderCircleIcon className="size-4 animate-spin" /> Removing…
                </span>
              )}
              {(failed || !site.enabled) && (
                <span className="absolute top-1.5 left-1.5">
                  {failed ? (
                    <Badge variant="destructive">Failed</Badge>
                  ) : (
                    <Badge variant="outline" className="bg-background/80">
                      Off
                    </Badge>
                  )}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2 px-1 pb-0.5">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{site.domain}</p>
                <p className="truncate text-[11px] text-muted-foreground">
                  {site.strategy === "php" && site.phpVersion
                    ? `PHP ${site.phpVersion}`
                    : PROJECT_TYPE_LABEL[site.projectType]}
                </p>
              </div>
              <button
                type="button"
                disabled={!site.enabled}
                onClick={(e) => {
                  e.stopPropagation();
                  void api.openSiteUrl(site.id);
                }}
                className="shrink-0 rounded-full border bg-background px-3 py-1 text-xs font-medium transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-40"
              >
                Open
              </button>
            </div>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {menu.map((item) => (
            <Fragment key={item.key}>
              {item.separated && <ContextMenuSeparator />}
              <ContextMenuItem
                disabled={item.disabled}
                destructive={item.destructive}
                onSelect={item.onSelect}
              >
                {item.icon} {item.label}
              </ContextMenuItem>
            </Fragment>
          ))}
        </ContextMenuContent>
      </ContextMenu>
    </li>
  );
}
