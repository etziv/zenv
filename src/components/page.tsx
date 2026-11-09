import { LoaderCircleIcon } from "lucide-react";
import type * as React from "react";

import { cn } from "@/lib/utils";

export function Page({ children }: { children: React.ReactNode }) {
  return <div className="mx-auto flex w-full max-w-7xl flex-col gap-6 px-8 pt-7 pb-10">{children}</div>;
}

export function PageHeader({ title, meta, actions }: { title: string; meta?: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0">
        <h1 className="font-display text-[26px] font-semibold leading-none tracking-tight">{title}</h1>
        {meta && <p className="mt-2 font-mono text-[11px] text-muted-foreground">{meta}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  );
}

/** Section label set like PCB silkscreen: small caps, then a hairline. */
export function Section({
  title,
  actions,
  children,
  className,
}: {
  title: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("flex flex-col gap-3", className)}>
      <div className="flex items-center gap-3">
        <h2 className="shrink-0 font-mono text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">{title}</h2>
        <span aria-hidden className="h-px flex-1 bg-border" />
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

export function Loading({ label = "Loading…" }: { label?: string }) {
  return (
    <div className="flex items-center gap-2 py-6 text-sm text-muted-foreground" role="status">
      <LoaderCircleIcon className="size-4 animate-spin" />
      {label}
    </div>
  );
}

export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children?: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-start gap-2 rounded-lg border border-dashed bg-card/60 p-5">
      <p className="text-sm font-medium">{title}</p>
      {children && <div className="max-w-xl text-sm text-muted-foreground">{children}</div>}
      {action && <div className="mt-1">{action}</div>}
    </div>
  );
}

/** Machine data (paths, ports, PIDs) is always monospace. */
export function Mono({ children, className, title }: { children: React.ReactNode; className?: string; title?: string }) {
  return (
    <span className={cn("font-mono text-[12px] break-all", className)} title={title}>
      {children}
    </span>
  );
}
