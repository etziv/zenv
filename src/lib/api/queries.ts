import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, type ApiError } from "./client";
import { toast } from "@/lib/toast";

import type { EditorId, DockerBinding, AppProcessStatus, CodexStatus, RouterId, RouterStatus, PathStatus, TailnetApp, StartOutcome, ServiceStatus, LogTail, CaStatus, ComponentId, HostsApplyOutcome, ServiceInput, SiteInput, SiteQuery, SiteView } from "./types";

export const keys = {
  appInfo: ["appInfo"] as const,
  catalog: ["catalog"] as const,
  installations: ["installations"] as const,
  services: ["services"] as const,
  serviceEvents: (id: string) => ["serviceEvents", id] as const,
  logSources: (id: string) => ["logSources", id] as const,
  serviceLog: (id: string, logId: string) => ["serviceLog", id, logId] as const,
  diagnostics: ["diagnostics"] as const,
  sites: ["sites"] as const,
  sitePage: (q: SiteQuery) => ["sites", "page", q] as const,
  siteDiagnose: (id: string) => ["sites", "diagnose", id] as const,
  siteLog: (id: string, kind: string) => ["sites", "log", id, kind] as const,
  hosts: ["hostsStatus"] as const,
  tls: ["tlsStatus"] as const,
  runtimes: ["runtimes"] as const,
  installerJobs: ["installerJobs"] as const,
  available: (c: string) => ["available", c] as const,
  tailnet: ["tailnet"] as const,
  editors: ["editors"] as const,
};

export const useAppInfo = () => useQuery({ queryKey: keys.appInfo, queryFn: api.appInfo, staleTime: Infinity });
/** Editors installed here; looked up again when the window regains focus. */
export const useEditors = () => useQuery({ queryKey: keys.editors, queryFn: api.editors, staleTime: 60_000 });

/** Opens an app's folder in an editor; picking one makes it the preferred one. */
export function useOpenInEditor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ siteId, editor }: { siteId: string; editor: EditorId | null }) => api.openInEditor(siteId, editor),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.editors }),
    onError: (e: ApiError) => toast({ variant: "error", title: "Couldn't open the editor", description: e.message }),
  });
}

export const useCatalog = () => useQuery({ queryKey: keys.catalog, queryFn: api.catalog, staleTime: Infinity });
export const useInstallations = () => useQuery({ queryKey: keys.installations, queryFn: () => api.installations() });

/** Polls so crashes and external changes show up without user action. */
export const useServices = () =>
  useQuery({ queryKey: keys.services, queryFn: api.services, refetchInterval: 3000 });

export const useServiceEvents = (id: string) =>
  useQuery({ queryKey: keys.serviceEvents(id), queryFn: () => api.serviceEvents(id), refetchInterval: 5000 });

export const useLogSources = (id: string) =>
  useQuery({ queryKey: keys.logSources(id), queryFn: () => api.serviceLogSources(id) });

export const useServiceLog = (id: string, logId: string | null, follow: boolean) =>
  useQuery({
    queryKey: keys.serviceLog(id, logId ?? ""),
    queryFn: () => api.serviceLog(id, logId ?? ""),
    enabled: logId !== null,
    refetchInterval: follow ? 2000 : false,
  });

export const useSiteLogSources = (id: string | null) =>
  useQuery({ queryKey: ["sites", "logSources", id], queryFn: () => api.siteLogSources(id ?? ""), enabled: id !== null });

export const useRuntimeLogGroups = () =>
  useQuery({ queryKey: ["runtimeLogGroups"], queryFn: api.runtimeLogGroups, refetchInterval: 10000 });

/** Any log tail, re-read every 2 s while following. */
export const useLogTail = (key: readonly unknown[], read: () => Promise<LogTail>, enabled: boolean, follow: boolean) =>
  useQuery({ queryKey: ["logTail", ...key], queryFn: read, enabled, refetchInterval: follow ? 2000 : false });

export const useDiagnostics = () => useQuery({ queryKey: keys.diagnostics, queryFn: api.diagnostics });

function useInvalidate() {
  const qc = useQueryClient();
  return (...queryKeys: ReadonlyArray<readonly unknown[]>) =>
    Promise.all(queryKeys.map((queryKey) => qc.invalidateQueries({ queryKey })));
}

export function useRegisterInstallation() {
  const invalidate = useInvalidate();
  return useMutation<Awaited<ReturnType<typeof api.registerInstallation>>, ApiError, { component: ComponentId; path: string }>({
    mutationFn: ({ component, path }) => api.registerInstallation(component, path),
    onSuccess: () => invalidate(keys.installations),
  });
}

export function useInstallationAction() {
  const invalidate = useInvalidate();
  return useMutation<unknown, ApiError, { action: "unregister" | "uninstall" | "setDefault" | "refresh" | "enable" | "disable"; id: string }>({
    mutationFn: ({ action, id }) => {
      switch (action) {
        case "unregister":
          return api.unregisterInstallation(id);
        case "uninstall":
          return api.uninstallInstallation(id);
        case "setDefault":
          return api.setDefaultInstallation(id);
        case "refresh":
          return api.refreshInstallation(id);
        case "enable":
          return api.setInstallationEnabled(id, true);
        case "disable":
          return api.setInstallationEnabled(id, false);
        default: {
          const unreachable: never = action;
          throw new Error(`unknown installation action ${String(unreachable)}`);
        }
      }
    },
    onSettled: () => invalidate(keys.installations, keys.services, keys.sites, keys.runtimes),
  });
}

export type ServiceAction = "start" | "stop" | "restart" | "reload";

const ACTION_VERB: Record<ServiceAction, string> = { start: "start", stop: "stop", restart: "restart", reload: "reload" };

export function useServiceAction() {
  const invalidate = useInvalidate();
  const qc = useQueryClient();
  return useMutation<unknown, ApiError, { action: ServiceAction; id: string }>({
    onError: (error, { action, id }) => {
      const name = qc.getQueryData<ServiceStatus[]>(keys.services)?.find((s) => s.service.id === id)?.service.name ?? "The service";
      toast({ variant: "error", title: `Couldn't ${ACTION_VERB[action]} ${name}`, description: error.message });
    },
    mutationFn: ({ action, id }) => {
      switch (action) {
        case "start":
          return api.startService(id);
        case "stop":
          return api.stopService(id);
        case "restart":
          return api.restartService(id);
        case "reload":
          return api.reloadService(id);
        default: {
          const unreachable: never = action;
          throw new Error(`unknown service action ${String(unreachable)}`);
        }
      }
    },
    onSettled: (_d, _e, { id }) => invalidate(keys.services, keys.serviceEvents(id), keys.logSources(id)),
  });
}

export function useValidateService() {
  const invalidate = useInvalidate();
  return useMutation({
    mutationFn: (id: string) => api.validateService(id),
    onSettled: (_d, _e, id) => invalidate(keys.serviceEvents(id)),
  });
}

export function useSaveService() {
  const invalidate = useInvalidate();
  return useMutation<unknown, ApiError, { id: string | null; input: ServiceInput }>({
    mutationFn: ({ id, input }) => (id ? api.updateService(id, input) : api.createService(input)),
    onSuccess: () => invalidate(keys.services),
  });
}

export function useDeleteService() {
  const invalidate = useInvalidate();
  return useMutation<string[], ApiError, string>({
    mutationFn: (id) => api.deleteService(id),
    onSuccess: () => invalidate(keys.services, keys.installations),
  });
}

// ----- local sites ------------------------------------------------------------

export const useSites = (query: SiteQuery) =>
  useQuery({ queryKey: keys.sitePage(query), queryFn: () => api.sites(query), placeholderData: keepPreviousData });

export const useHostsStatus = () => useQuery({ queryKey: keys.hosts, queryFn: api.hostsStatus });

export const useServicesForApps = () =>
  useQuery({ queryKey: ["servicesForApps"], queryFn: api.servicesForApps, refetchInterval: 5000 });

/** Starts everything the apps need and reports the outcome as notifications. */
export function useStartServicesForApps() {
  const qc = useQueryClient();
  return useMutation<StartOutcome[], ApiError, void>({
    mutationFn: () => api.startServicesForApps(),
    onSuccess: (outcomes) => {
      const started = outcomes.filter((o) => o.result === "started");
      const failed = outcomes.filter((o) => o.result === "failed");
      for (const f of failed) toast({ variant: "error", title: `Couldn't start ${f.name}`, description: f.error ?? undefined });
      if (started.length > 0) {
        toast({ variant: "success", title: `Started ${started.map((s) => s.name).join(", ")}` });
      } else if (failed.length === 0) {
        toast({ variant: "info", title: outcomes.length ? "Everything your apps need is already running" : "No app needs a service yet" });
      }
    },
    onError: (e) => toast({ variant: "error", title: "Couldn't start the apps' services", description: e.message }),
    onSettled: () => qc.invalidateQueries(),
  });
}

export function useStopAllServices() {
  const qc = useQueryClient();
  return useMutation<StartOutcome[], ApiError, void>({
    mutationFn: () => api.stopAllServices(),
    onSuccess: (outcomes) => {
      const failed = outcomes.filter((o) => o.result === "failed");
      for (const f of failed) toast({ variant: "error", title: `Couldn't stop ${f.name}`, description: f.error ?? undefined });
      const stopped = outcomes.filter((o) => o.result === "stopped").length;
      if (stopped > 0) toast({ variant: "success", title: `Stopped ${stopped} service${stopped === 1 ? "" : "s"}` });
    },
    onError: (e) => toast({ variant: "error", title: "Couldn't stop the services", description: e.message }),
    onSettled: () => qc.invalidateQueries(),
  });
}

/** The program behind a proxy app; polled while shown. */
export const useAppProcess = (siteId: string) =>
  useQuery({ queryKey: ["appProcess", siteId], queryFn: () => api.appProcessStatus(siteId), refetchInterval: 2000 });

type AppProcessAction =
  | { action: "start" | "stop" | "restart"; siteId: string; domain: string }
  | { action: "save"; siteId: string; domain: string; command: string | null; autostart: boolean };

export function useAppProcessAction() {
  const qc = useQueryClient();
  return useMutation<AppProcessStatus, ApiError, AppProcessAction>({
    mutationFn: async (a) => {
      switch (a.action) {
        case "start":
          return api.appProcessStart(a.siteId);
        case "stop":
          return api.appProcessStop(a.siteId);
        case "restart":
          await api.appProcessStop(a.siteId);
          return api.appProcessStart(a.siteId);
        case "save":
          return api.appCommandSet(a.siteId, a.command, a.autostart);
      }
    },
    onSuccess: (status, a) => qc.setQueryData(["appProcess", a.siteId], status),
    onError: (e, a) =>
      toast({ variant: "error", title: a.action === "save" ? `Couldn't save ${a.domain}'s command` : `Couldn't ${a.action} ${a.domain}`, description: e.message }),
  });
}

/** Codex's config as it concerns one AI router. */
export const useCodexStatus = (component: RouterId) => useQuery({ queryKey: ["codex", component], queryFn: () => api.codexStatus(component) });

type CodexAction =
  | { action: "setup"; serviceId: string; makeDefault: boolean }
  | { action: "default"; on: boolean }
  | { action: "model"; model: string; contextWindow: number | null; maxOutput: number | null };

/** The models and combos a running router offers; asked for when needed. */
export const useRouterModels = (serviceId: string, enabled: boolean) =>
  useQuery({ queryKey: ["routerModels", serviceId], queryFn: () => api.routerModels(serviceId), enabled, staleTime: 60_000, retry: false });

export function useCodexAction(component: RouterId, name: string) {
  const qc = useQueryClient();
  return useMutation<CodexStatus, ApiError, CodexAction>({
    mutationFn: (a) =>
      a.action === "setup"
        ? api.codexSetup(a.serviceId, a.makeDefault)
        : a.action === "default"
          ? api.codexSetDefault(component, a.on)
          : api.codexSetModel(component, a.model, a.contextWindow, a.maxOutput),
    onSuccess: (status, a) => {
      qc.setQueryData(["codex", component], status);
      // The other router's default flag changes too.
      void qc.invalidateQueries({ queryKey: ["codex"] });
      toast({
        variant: "success",
        title:
          a.action === "setup"
            ? `${name} is in Codex's config`
            : a.action === "model"
              ? `Codex asks ${name} for ${a.model}`
              : a.on
              ? `Codex uses ${name} by default`
              : `Codex is back to ${status.topLevelProvider ?? "its previous setup"}`,
        description: `${status.configPath} (backed up first). Start a new Codex session to use it.`,
      });
    },
    onError: (e) => toast({ variant: "error", title: "Couldn't change Codex's config", description: e.message }),
  });
}

/** A router's versions and auto-update; polled while an update runs. */
export const useRouterStatus = (component: RouterId) =>
  useQuery({
    queryKey: ["router", component],
    queryFn: () => api.routerStatus(component),
    refetchInterval: (q) => (q.state.data?.updating ? 3000 : 60_000),
  });

export function useRouterAction(component: RouterId, name: string) {
  const qc = useQueryClient();
  return useMutation<RouterStatus, ApiError, { action: "update"; now: boolean } | { action: "auto"; on: boolean }>({
    mutationFn: (a) => (a.action === "update" ? api.routerUpdate(component, a.now) : api.routerSetAutoUpdate(component, a.on)),
    onSuccess: (status, a) => {
      qc.setQueryData(["router", component], status);
      if (a.action === "update") {
        const newest = status.latest && status.latest === status.installed;
        toast(
          status.lastError
            ? { variant: "error", title: `${name} was not updated`, description: status.lastError }
            : newest
              ? { variant: "success", title: `${name} ${status.installed} is the newest` }
              : status.latest && status.autoFrom
                ? {
                    variant: "success",
                    title: `${name} ${status.latest} is available`,
                    description: `New releases wait a week before auto-update installs them (from ${new Date(status.autoFrom).toLocaleDateString()}). Use Install now to get it earlier.`,
                  }
                : { variant: "success", title: `${name} checked` },
        );
      }
    },
    onError: (e) => toast({ variant: "error", title: `Couldn't update ${name}`, description: e.message }),
    onSettled: () => void qc.invalidateQueries(),
  });
}

/** The version on the user PATH and competing folders; re-read when the window regains focus. */
export const usePathStatus = (component: ComponentId) =>
  useQuery({ queryKey: ["path", component], queryFn: () => api.pathStatus(component), staleTime: 5_000 });

type PathAction =
  | { action: "add"; component: ComponentId; installationId: string; label: string }
  | { action: "remove"; component: ComponentId; label: string };

export function usePathAction() {
  const qc = useQueryClient();
  return useMutation<PathStatus, ApiError, PathAction>({
    mutationFn: (a) => (a.action === "add" ? api.pathAdd(a.installationId) : api.pathRemove(a.component)),
    onSuccess: (status, a) => {
      qc.setQueryData(["path", a.component], status);
      toast({
        variant: "success",
        title: a.action === "add" ? `${a.label} is on PATH` : `${a.label} is off PATH`,
        description: "Open a new terminal to use it; terminals already open keep their old PATH.",
      });
    },
    onError: (e, a) => {
      void qc.invalidateQueries({ queryKey: ["path", a.component] });
      toast({ variant: "error", title: a.action === "add" ? `Couldn't add ${a.label} to PATH` : `Couldn't take ${a.label} off PATH`, description: e.message });
    },
  });
}

/** Tailscale on this computer (installed, signed in, its MagicDNS name). */
export const useTailnetStatus = () =>
  useQuery({ queryKey: [...keys.tailnet, "status"], queryFn: api.tailnetStatus, staleTime: 10_000, refetchInterval: 30_000 });

export const useTailnetApps = () => useQuery({ queryKey: [...keys.tailnet, "apps"], queryFn: api.tailnetApps });

export function useTailnetAction() {
  const qc = useQueryClient();
  return useMutation<TailnetApp | void, ApiError, { action: "open" | "close"; siteId: string; domain: string; service?: string | null }>({
    mutationFn: ({ action, siteId, service }) => (action === "open" ? api.tailnetOpen(siteId, service) : api.tailnetClose(siteId)),
    onSuccess: (app, { action, domain }) => {
      if (action === "open" && app) toast({ variant: "success", title: `${domain} is open on your tailnet`, description: app.url ?? undefined });
    },
    onError: (e, { action, domain }) =>
      toast({ variant: "error", title: action === "open" ? `Couldn't open ${domain} on the tailnet` : `Couldn't close ${domain} on the tailnet`, description: e.message }),
    onSettled: () => void qc.invalidateQueries({ queryKey: keys.tailnet }),
  });
}

/** A language's or server's logo from its own installation (null: none). */
export const useComponentIcon = (component: ComponentId, installedCount: number) =>
  useQuery({
    queryKey: ["componentIcon", component, installedCount],
    queryFn: () => api.componentIcon(component),
    staleTime: Infinity,
    enabled: installedCount > 0,
  });

export const thumbnailKey = (id: string) => ["thumbnail", id] as const;

export const useThumbnail = (id: string) =>
  useQuery({ queryKey: thumbnailKey(id), queryFn: () => api.siteThumbnail(id), staleTime: Infinity });

export const useTlsStatus = () => useQuery({ queryKey: keys.tls, queryFn: api.tlsStatus });

export function useTlsAction() {
  const qc = useQueryClient();
  return useMutation<CaStatus, ApiError, "create" | "trust" | "untrust">({
    mutationFn: (action) =>
      action === "create" ? api.tlsCreateCa() : action === "trust" ? api.tlsTrust() : api.tlsUntrust(),
    onSuccess: (status) => qc.setQueryData(keys.tls, status),
  });
}

export const useSiteDiagnosis = (id: string) =>
  useQuery({ queryKey: keys.siteDiagnose(id), queryFn: () => api.diagnoseSite(id) });

export const useSiteLog = (id: string, kind: "error" | "access" | "process", follow: boolean) =>
  useQuery({ queryKey: keys.siteLog(id, kind), queryFn: () => api.siteLog(id, kind), refetchInterval: follow ? 2000 : false });

export const useDetectProject = () => useMutation({ mutationFn: (path: string) => api.detectSite(path) });

function useSiteInvalidate() {
  const invalidate = useInvalidate();
  return () => invalidate(keys.sites, keys.hosts, keys.services);
}

export function useSaveSite() {
  const invalidate = useSiteInvalidate();
  return useMutation<SiteView, ApiError, { id: string | null; input: SiteInput }>({
    mutationFn: ({ id, input }) => (id ? api.updateSite(id, input) : api.createSite(input)),
    onSuccess: () => invalidate(),
  });
}

/** Adds an app served from a Docker container. */
export function useCreateDockerApp() {
  const invalidate = useSiteInvalidate();
  return useMutation<SiteView, ApiError, { input: SiteInput; binding: DockerBinding }>({
    mutationFn: ({ input, binding }) => api.dockerAppCreate(input, binding),
    onSuccess: () => invalidate(),
  });
}

/** Containers on this computer, for adding one as an app. */
export const useDockerContainers = (enabled: boolean) =>
  useQuery({ queryKey: ["dockerContainers"], queryFn: api.dockerContainers, enabled, retry: false, staleTime: 5_000 });

/** Starts Docker Desktop and waits for it (up to two minutes). */
export function useDockerEngineStart() {
  const qc = useQueryClient();
  return useMutation<boolean, ApiError, void>({
    mutationFn: () => api.dockerEngineStart(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["dockerContainers"] });
      void qc.invalidateQueries({ queryKey: ["appProcess"] });
    },
  });
}

export type SiteAction = { action: "enable" | "disable" | "duplicate" | "delete"; id: string };

export function useSiteAction() {
  const invalidate = useSiteInvalidate();
  return useMutation<unknown, ApiError, SiteAction>({
    mutationFn: ({ action, id }) => {
      switch (action) {
        case "enable":
          return api.setSiteEnabled(id, true);
        case "disable":
          return api.setSiteEnabled(id, false);
        case "duplicate":
          return api.duplicateSite(id);
        case "delete":
          return api.deleteSite(id);
        default: {
          const unreachable: never = action;
          throw new Error(`unknown site action ${String(unreachable)}`);
        }
      }
    },
    onSettled: () => invalidate(),
  });
}

export function useApplyHosts() {
  const invalidate = useSiteInvalidate();
  return useMutation<HostsApplyOutcome, ApiError, void>({
    mutationFn: () => api.applyHosts(),
    onSettled: () => invalidate(),
  });
}

export const useRuntimes = () => useQuery({ queryKey: keys.runtimes, queryFn: api.runtimes, refetchInterval: 5000 });

export function useInitializeServiceData() {
  const invalidate = useInvalidate();
  return useMutation<string, ApiError, string>({
    mutationFn: (id) => api.initializeServiceData(id),
    onSettled: (_d, _e, id) => invalidate(keys.services, keys.serviceEvents(id)),
  });
}

export function useProbeService() {
  return useMutation<string, ApiError, string>({ mutationFn: (id) => api.probeService(id) });
}

export const useAvailableVersions = (component: ComponentId | null) =>
  useQuery({
    queryKey: keys.available(component ?? ""),
    queryFn: () => api.installerAvailable(component as ComponentId),
    enabled: component !== null,
    staleTime: 5 * 60_000,
  });

/** Polls quickly while an install is in progress. */
export const useInstallJobs = () =>
  useQuery({
    queryKey: keys.installerJobs,
    queryFn: api.installerJobs,
    refetchInterval: (q) =>
      (q.state.data ?? []).some((j) => j.phase !== "done" && j.phase !== "failed") ? 500 : 5000,
  });

export function useStartInstall() {
  const qc = useQueryClient();
  return useMutation<string, ApiError, { component: ComponentId; version: string }>({
    mutationFn: ({ component, version }) => api.installerInstall(component, version),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.installerJobs }),
  });
}
