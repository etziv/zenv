import type { ConfigStatus, DomainState, PhpBackend, ProjectType, Strategy } from "@/lib/api/types";

export const PROJECT_TYPE_LABEL: Record<ProjectType, string> = {
  laravel: "Laravel",
  php: "PHP",
  static: "Static site",
  go: "Go app",
  rust: "Rust app",
  python: "Python app",
  ruby: "Ruby app",
  custom: "Custom",
};

export const STRATEGY_LABEL: Record<Strategy, string> = {
  php: "PHP",
  static: "Static files",
  reverse_proxy: "Proxy to app",
};

export const BACKEND_LABEL: Record<PhpBackend, string> = {
  fastcgi: "PHP-CGI",
  frankenphp: "FrankenPHP",
};

export function statusBadge(status: ConfigStatus): { label: string; variant: "success" | "warning" | "destructive" | "outline" } {
  switch (status) {
    case "applied":
      return { label: "Active", variant: "success" };
    case "pending":
      return { label: "Pending", variant: "warning" };
    case "failed":
      return { label: "Failed", variant: "destructive" };
    case "disabled":
      return { label: "Disabled", variant: "outline" };
    default: {
      const unreachable: never = status;
      return unreachable;
    }
  }
}

/** Short explanation of how a domain reaches this computer, or null when fine. */
export function hostsProblem(state: DomainState): string | null {
  switch (state) {
    case "not_needed":
    case "managed":
    case "external":
      return null;
    case "missing":
      return "Not in the hosts file yet";
    case "conflict":
      return "The hosts file points this domain elsewhere";
    default: {
      const unreachable: never = state;
      return unreachable;
    }
  }
}
