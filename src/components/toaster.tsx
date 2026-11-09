import { CircleAlertIcon, CircleCheckIcon, InfoIcon, XIcon } from "lucide-react";

import { dismissToast, useToasts } from "@/lib/toast";
import { cn } from "@/lib/utils";

/** Notifications at the top right, below the header and level with the page title. */
export function Toaster() {
  const toasts = useToasts();
  return (
    <div aria-live="polite" className="pointer-events-none fixed top-[4.6rem] right-4 z-50 flex w-[24rem] max-w-[calc(100vw-2rem)] flex-col gap-2">
      {toasts.map((t) => {
        const Icon = t.variant === "error" ? CircleAlertIcon : t.variant === "success" ? CircleCheckIcon : InfoIcon;
        return (
          <div
            key={t.id}
            role={t.variant === "error" ? "alert" : "status"}
            className={cn(
              "pointer-events-auto flex gap-3 rounded-lg border border-l-[3px] bg-popover p-3 text-popover-foreground shadow-lg",
              t.variant === "error" && "border-l-destructive",
              t.variant === "success" && "border-l-success",
              t.variant === "info" && "border-l-primary",
            )}
          >
            <Icon
              className={cn(
                "mt-0.5 size-4 shrink-0",
                t.variant === "error" ? "text-destructive" : t.variant === "success" ? "text-success" : "text-primary",
              )}
            />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">{t.title}</p>
              {t.description && <p className="mt-0.5 line-clamp-4 text-xs break-words text-muted-foreground">{t.description}</p>}
              {t.action && (
                <button
                  type="button"
                  onClick={() => {
                    t.action?.onClick();
                    dismissToast(t.id);
                  }}
                  className="mt-1.5 text-xs font-medium text-primary hover:underline"
                >
                  {t.action.label}
                </button>
              )}
            </div>
            <button
              type="button"
              onClick={() => dismissToast(t.id)}
              aria-label="Dismiss"
              className="grid size-5 shrink-0 place-items-center rounded text-muted-foreground hover:text-foreground"
            >
              <XIcon className="size-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
