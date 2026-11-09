import { useComponentIcon } from "@/lib/api/queries";
import type { ComponentId } from "@/lib/api/types";
import { cn } from "@/lib/utils";

const MONOGRAM: Partial<Record<ComponentId, string>> = {
  php: "php",
  go: "go",
  rust: "rs",
  ruby: "rb",
  python: "py",
  node: "js",
  apache: "ap",
  nginx: "ng",
  caddy: "cd",
  frankenphp: "fp",
  mysql: "my",
  postgresql: "pg",
  redis: "rd",
  omniroute: "or",
  ninerouter: "9r",
};

/**
 * The component's own logo (from its installation) on a tile, or a monogram
 * when the installation carries none.
 */
export function ComponentLogo({
  component,
  installedCount,
  className,
}: {
  component: ComponentId;
  installedCount: number;
  className?: string;
}) {
  const icon = useComponentIcon(component, installedCount);
  const installed = installedCount > 0;
  return (
    <span
      className={cn(
        "grid size-10 shrink-0 place-items-center overflow-hidden rounded-md border font-mono text-[13px] font-bold",
        installed ? "border-primary/40 bg-primary/10 text-primary" : "bg-muted text-muted-foreground",
        icon.data && "border-border bg-card",
        className,
      )}
    >
      {icon.data ? (
        <img src={icon.data} alt="" className="size-[70%] object-contain" draggable={false} />
      ) : (
        (MONOGRAM[component] ?? component.slice(0, 2))
      )}
    </span>
  );
}
