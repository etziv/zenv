import { useQueryClient } from "@tanstack/react-query";
import { ActivityIcon, CopyIcon, FileCogIcon, ImageIcon, PencilIcon, SparklesIcon, Trash2Icon } from "lucide-react";
import type * as React from "react";

import { api } from "@/lib/api/client";
import { useOpenInEditor, useSiteAction } from "@/lib/api/queries";
import type { SiteView } from "@/lib/api/types";
import { toast } from "@/lib/toast";

import { EditorIcon, usePreferredEditor } from "./edit-buttons";
import { capturePreview } from "./previews";

export type PanelAction = "edit" | "details" | "output" | "delete";

export interface AppActionItem {
  key: string;
  label: string;
  icon: React.ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
  /** Draw a separator above this item. */
  separated?: boolean;
}

/**
 * One app's actions, shared by the panel's "…" menu and the card's
 * right-click menu so both always offer the same things.
 */
export function useAppActions(site: SiteView, onAction: (a: PanelAction) => void): AppActionItem[] {
  const qc = useQueryClient();
  const action = useSiteAction();
  const fail = (title: string) => (e: Error) => toast({ variant: "error", title, description: e.message });
  const { preferred } = usePreferredEditor();
  const openInEditor = useOpenInEditor();
  return [
    {
      key: "editor",
      label: preferred ? `Open in ${preferred.name}` : "Open in editor",
      icon: <EditorIcon editor={preferred} />,
      disabled: !preferred,
      onSelect: () => preferred && openInEditor.mutate({ siteId: site.id, editor: preferred.id }),
    },
    {
      key: "studio",
      label: "Open in Studio",
      icon: <SparklesIcon />,
      onSelect: () => void api.studioOpen(site.id).catch(fail("Couldn't open Studio")),
    },
    { key: "edit", label: "Edit all settings", icon: <PencilIcon />, onSelect: () => onAction("edit") },
    { key: "details", label: "Diagnose & logs", icon: <ActivityIcon />, onSelect: () => onAction("details") },
    {
      key: "preview",
      label: "Refresh preview",
      icon: <ImageIcon />,
      disabled: !site.enabled,
      onSelect: () => capturePreview(qc, site.id, { force: true }),
    },
    {
      key: "duplicate",
      label: "Duplicate",
      icon: <CopyIcon />,
      onSelect: () => action.mutate({ action: "duplicate", id: site.id }, { onError: fail(`Couldn't duplicate ${site.domain}`) }),
    },
    {
      key: "config",
      label: "Show configuration file",
      icon: <FileCogIcon />,
      disabled: !site.enabled,
      onSelect: () => void api.reveal({ kind: "siteConfig", id: site.id }).catch(fail("Couldn't show the configuration file")),
    },
    { key: "delete", label: "Remove app", icon: <Trash2Icon />, destructive: true, separated: true, onSelect: () => onAction("delete") },
  ];
}
