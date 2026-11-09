import { LoaderCircleIcon } from "lucide-react";
import { Switch as SwitchPrimitive } from "radix-ui";
import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * An on/off switch that carries its own state: "ON" / "OFF" inside the track,
 * a spinner in the thumb while the change is in progress, and a red track
 * when the service stopped by itself.
 */
export function StatusSwitch({
  checked,
  pending = false,
  failed = false,
  className,
  ...props
}: React.ComponentProps<typeof SwitchPrimitive.Root> & { pending?: boolean; failed?: boolean }) {
  return (
    <SwitchPrimitive.Root
      checked={checked}
      aria-busy={pending || undefined}
      className={cn(
        "relative inline-flex h-6 w-[3.4rem] shrink-0 cursor-pointer items-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-not-allowed",
        "data-[state=checked]:border-primary data-[state=checked]:bg-primary",
        "data-[state=unchecked]:border-input data-[state=unchecked]:bg-muted",
        failed && "data-[state=unchecked]:border-destructive/60 data-[state=unchecked]:bg-destructive/15",
        className,
      )}
      {...props}
    >
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute font-mono text-[9.5px] font-bold tracking-wider transition-opacity",
          checked ? "left-2 text-primary-foreground" : "right-1.5",
          !checked && (failed ? "text-destructive" : "text-muted-foreground"),
        )}
      >
        {checked ? "ON" : failed ? "ERR" : "OFF"}
      </span>
      <SwitchPrimitive.Thumb className="pointer-events-none grid size-[1.1rem] place-items-center rounded-full bg-background shadow-sm ring-0 transition-transform data-[state=checked]:translate-x-[1.95rem] data-[state=unchecked]:translate-x-0.5">
        {pending && <LoaderCircleIcon className="size-3 animate-spin text-muted-foreground" />}
      </SwitchPrimitive.Thumb>
    </SwitchPrimitive.Root>
  );
}
