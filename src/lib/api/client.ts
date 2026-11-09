import { type Channel, invoke } from "@tauri-apps/api/core";

import type {
  ProjectEntry,
  ProjectFile,
  ProjectListing,
  AgentId,
  EditorId,
  Editors,
  AgentInfo,
  TerminalMessage,
  ChatLine,
  ChatOptions,
  ChatAttachment,
  ChatCatalog,
  ChatSession,
  ChatControl,
  ApplyReport,
  AppInfo,
  DetectedProject,
  HostsApplyOutcome,
  HostsStatus,
  RuntimeProcess,
  AvailableVersion,
  CaStatus,
  NeededService,
  StartOutcome,
  Thumbnail,
  TailnetApp,
  PathStatus,
  RouterId,
  RouterModel,
  RouterStatus,
  CodexStatus,
  TailnetStatus,
  AppProcessStatus,
  DockerBinding,
  DockerContainer,
  InstallJob,
  SiteInput,
  SitePage,
  SiteQuery,
  SiteView,
  Check,
  ComponentId,
  ComponentInfo,
  DiscoveredInstallation,
  ErrorCode,
  ErrorDetails,
  Installation,
  LogSource,
  RuntimeLogGroup,
  LogTail,
  PortInspection,
  RevealTarget,
  Service,
  ServiceEvent,
  ServiceInput,
  ServiceStatus,
  ValidationReport,
} from "./types";

/** A typed backend error (`AppError` on the Rust side). */
export class ApiError extends Error {
  readonly code: ErrorCode;
  readonly details: ErrorDetails;

  constructor(code: ErrorCode, message: string, details: ErrorDetails = null) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.details = details;
  }
}

function isErrorPayload(value: unknown): value is { code: ErrorCode; message: string; details: ErrorDetails } {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string"
  );
}

export function toApiError(raw: unknown, command: string): ApiError {
  if (raw instanceof ApiError) return raw;
  if (isErrorPayload(raw)) return new ApiError(raw.code, raw.message, raw.details ?? null);
  const text = raw instanceof Error ? raw.message : String(raw);
  const outsideShell = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
  return new ApiError(
    "ipc",
    outsideShell
      ? "Zenv's backend is unavailable: this page is running outside the desktop app."
      : `${command} failed: ${text}`,
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw toApiError(raw, command);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  catalog: () => call<ComponentInfo[]>("catalog_list"),

  installations: (component?: ComponentId) => call<Installation[]>("installations_list", { component }),
  discover: (component?: ComponentId) => call<DiscoveredInstallation[]>("installations_discover", { component }),
  registerInstallation: (component: ComponentId, path: string) =>
    call<Installation>("installation_register", { component, path }),
  unregisterInstallation: (id: string) => call<void>("installation_unregister", { id }),
  setDefaultInstallation: (id: string) => call<Installation>("installation_set_default", { id }),
  refreshInstallation: (id: string) => call<Installation>("installation_refresh", { id }),
  setInstallationEnabled: (id: string, enabled: boolean) =>
    call<Installation>("installation_set_enabled", { id, enabled }),
  runtimes: () => call<RuntimeProcess[]>("runtimes_status"),
  installerAvailable: (component: ComponentId) => call<AvailableVersion[]>("installer_available", { component }),
  installerInstall: (component: ComponentId, version: string) => call<string>("installer_install", { component, version }),
  installerJobs: () => call<InstallJob[]>("installer_jobs"),
  uninstallInstallation: (id: string) => call<void>("installation_uninstall", { id }),
  openTerminal: (installationIds: string[], cwd?: string) => call<void>("open_terminal", { installationIds, cwd }),

  services: () => call<ServiceStatus[]>("services_list"),
  createService: (input: ServiceInput) => call<Service>("service_create", { input }),
  updateService: (id: string, input: ServiceInput) => call<Service>("service_update", { id, input }),
  deleteService: (id: string) => call<string[]>("service_delete", { id }),
  validateService: (id: string) => call<ValidationReport>("service_validate", { id }),
  startService: (id: string) => call<ServiceStatus>("service_start", { id }),
  stopService: (id: string) => call<ServiceStatus>("service_stop", { id }),
  restartService: (id: string) => call<ServiceStatus>("service_restart", { id }),
  reloadService: (id: string) => call<ServiceStatus>("service_reload", { id }),
  initializeServiceData: (id: string) => call<string>("service_initialize_data", { id }),
  probeService: (id: string) => call<string>("service_probe", { id }),
  serviceEvents: (id: string, limit = 50) => call<ServiceEvent[]>("service_events", { id, limit }),
  serviceLogSources: (id: string) => call<LogSource[]>("service_log_sources", { id }),
  serviceLog: (id: string, logId: string, lines = 300) =>
    call<LogTail>("service_log_read", { id, logId, lines }),

  appLog: (lines = 300) => call<LogTail>("app_log_read", { lines }),
  inspectPort: (port: number, address?: string) => call<PortInspection>("port_inspect", { port, address }),
  diagnostics: () => call<Check[]>("diagnostics_run"),
  reveal: (target: RevealTarget) => call<void>("reveal", { target }),
  openServiceUrl: (id: string) => call<void>("open_service_url", { id }),

  sites: (query: SiteQuery) => call<SitePage>("sites_list", { query }),
  site: (id: string) => call<SiteView>("site_get", { id }),
  detectSite: (path: string) => call<DetectedProject>("site_detect", { path }),
  createSite: (input: SiteInput) => call<SiteView>("site_create", { input }),
  updateSite: (id: string, input: SiteInput) => call<SiteView>("site_update", { id, input }),
  setSiteEnabled: (id: string, enabled: boolean) => call<SiteView>("site_set_enabled", { id, enabled }),
  duplicateSite: (id: string) => call<SiteView>("site_duplicate", { id }),
  deleteSite: (id: string) => call<void>("site_delete", { id }),
  diagnoseSite: (id: string) => call<Check[]>("site_diagnose", { id }),
  siteLog: (id: string, kind: string, lines = 300) => call<LogTail>("site_log_read", { id, kind, lines }),
  siteLogSources: (id: string) => call<LogSource[]>("site_log_sources", { id }),
  runtimeLogGroups: () => call<RuntimeLogGroup[]>("runtime_log_groups"),
  runtimeLog: (id: string, logId: string, lines = 300) => call<LogTail>("runtime_log_read", { id, logId, lines }),
  reapplySites: (serviceId: string) => call<ApplyReport>("sites_reapply", { serviceId }),
  openSiteUrl: (id: string) => call<void>("open_site_url", { id }),
  studioOpen: (siteId: string) => call<void>("studio_open", { siteId }),
  editors: () => call<Editors>("editors_list"),
  projectList: (siteId: string, dir: string) => call<ProjectListing>("project_list", { siteId, dir }),
  projectRead: (siteId: string, path: string) => call<ProjectFile>("project_read", { siteId, path }),
  projectWrite: (siteId: string, path: string, content: string, expected: number | null) =>
    call<number>("project_write", { siteId, path, content, expected }),
  projectCreate: (siteId: string, path: string, kind: "file" | "folder") => call<ProjectEntry>("project_create", { siteId, path, kind }),
  projectStat: (siteId: string, paths: string[]) => call<(number | null)[]>("project_stat", { siteId, paths }),
  formatterInstall: (siteId: string, tool: string) => call<string>("formatter_install", { siteId, tool }),
  projectFormat: (siteId: string, path: string, content: string) => call<string>("project_format", { siteId, path, content }),
  projectSearch: (siteId: string, query: string) => call<ProjectEntry[]>("project_search", { siteId, query }),
  openInEditor: (siteId: string, editor: EditorId | null) => call<EditorId>("site_open_in_editor", { siteId, editor }),
  studioAgents: (siteId: string) => call<AgentInfo[]>("studio_agents", { siteId }),
  /** `screen`: one of Claude Code's own screens ("/mcp", "/hooks", ...) to open in it. */
  terminalOpen: (siteId: string, agent: AgentId, screen: string | null, cols: number, rows: number, events: Channel<TerminalMessage>) =>
    call<string>("terminal_open", { siteId, agent, screen, cols, rows, events }),
  terminalWrite: (id: string, data: string) => call<void>("terminal_write", { id, data }),
  terminalResize: (id: string, cols: number, rows: number) => call<void>("terminal_resize", { id, cols, rows }),
  terminalClose: (id: string) => call<void>("terminal_close", { id }),
  chatOpen: (siteId: string, resume: string | null, options: ChatOptions, events: Channel<ChatLine>) =>
    call<string>("chat_open", { siteId, resume, options, events }),
  chatSend: (id: string, text: string, attachments: ChatAttachment[]) => call<void>("chat_send", { id, text, attachments }),
  chatAnswer: (id: string, requestId: string, allow: boolean, always: boolean) =>
    call<void>("chat_answer", { id, requestId, allow, always }),
  chatInterrupt: (id: string) => call<void>("chat_interrupt", { id }),
  chatClose: (id: string) => call<void>("chat_close", { id }),
  chatControl: (id: string, control: ChatControl) => call<string>("chat_control", { id, control }),
  openClaudeDocs: () => call<void>("open_claude_docs"),
  chatCatalog: (siteId: string) => call<ChatCatalog>("chat_catalog", { siteId }),
  chatSessions: (siteId: string) => call<ChatSession[]>("chat_sessions", { siteId }),
  chatHistory: (siteId: string, sessionId: string) =>
    call<string[]>("chat_history", { siteId, sessionId }).then((lines) => lines.map((l) => JSON.parse(l) as ChatLine)),
  routerStatus: (component: RouterId) => call<RouterStatus>("router_status", { component }),
  routerUpdate: (component: RouterId, now: boolean) => call<RouterStatus>("router_update", { component, now }),
  routerSetAutoUpdate: (component: RouterId, on: boolean) => call<RouterStatus>("router_set_auto_update", { component, on }),
  routerOpen: (serviceId: string, page: string | null) => call<void>("router_open", { serviceId, page }),
  codexStatus: (component: RouterId) => call<CodexStatus>("codex_status", { component }),
  codexSetup: (serviceId: string, makeDefault: boolean) => call<CodexStatus>("codex_setup", { serviceId, makeDefault }),
  codexSetDefault: (component: RouterId, on: boolean) => call<CodexStatus>("codex_set_default", { component, on }),
  codexSnippet: (serviceId: string) => call<string>("codex_snippet", { serviceId }),
  routerModels: (serviceId: string) => call<RouterModel[]>("router_models", { serviceId }),
  codexSetModel: (component: RouterId, model: string, contextWindow: number | null, maxOutput: number | null) =>
    call<CodexStatus>("codex_set_model", { component, model, contextWindow, maxOutput }),
  pathStatus: (component: ComponentId) => call<PathStatus>("path_status", { component }),
  pathAdd: (installationId: string) => call<PathStatus>("path_add", { installationId }),
  pathRemove: (component: ComponentId) => call<PathStatus>("path_remove", { component }),
  tailnetStatus: () => call<TailnetStatus>("tailnet_status"),
  tailnetApps: () => call<TailnetApp[]>("tailnet_apps"),
  /** `service`: open it as that Tailscale Service (its own https name) instead of on a port. */
  tailnetOpen: (siteId: string, service?: string | null) => call<TailnetApp>("tailnet_open", { siteId, service: service ?? null }),
  tailnetClose: (siteId: string) => call<void>("tailnet_close", { siteId }),
  tailnetOpenUrl: (siteId: string) => call<void>("tailnet_open_url", { siteId }),
  appProcessStatus: (siteId: string) => call<AppProcessStatus>("app_process_status", { siteId }),
  dockerContainers: () => call<DockerContainer[]>("docker_containers"),
  dockerEngineStart: () => call<boolean>("docker_engine_start"),
  dockerAppCreate: (input: SiteInput, binding: DockerBinding) => call<SiteView>("docker_app_create", { input, binding }),
  appProcessStart: (siteId: string) => call<AppProcessStatus>("app_process_start", { siteId }),
  appProcessStop: (siteId: string) => call<AppProcessStatus>("app_process_stop", { siteId }),
  /** Best free port for a kind of project (nearest free from its usual port). */
  appPortSuggest: (projectType: string | null, excludeSiteId: string | null) =>
    call<number>("app_port_suggest", { projectType, excludeSiteId }),
  appCommandSet: (siteId: string, command: string | null, autostart: boolean) =>
    call<AppProcessStatus>("app_command_set", { siteId, command, autostart }),
  hostsStatus: () => call<HostsStatus>("hosts_status"),
  applyHosts: () => call<HostsApplyOutcome>("hosts_apply"),
  tlsStatus: () => call<CaStatus>("tls_status"),
  servicesForApps: () => call<NeededService[]>("services_for_apps"),
  startServicesForApps: () => call<StartOutcome[]>("start_services_for_apps"),
  stopAllServices: () => call<StartOutcome[]>("stop_all_services"),
  panelOpenMain: () => call<void>("panel_open_main"),
  panelHide: () => call<void>("panel_hide"),
  panelResize: (height: number) => call<void>("panel_resize", { height }),
  quit: () => call<void>("app_quit"),
  componentIcon: (component: ComponentId) => call<string | null>("component_icon", { component }),
  siteThumbnail: (id: string) => call<Thumbnail | null>("site_thumbnail", { id }),
  captureSiteThumbnail: (id: string) => call<Thumbnail>("site_thumbnail_capture", { id }),
  tlsCreateCa: () => call<CaStatus>("tls_create_ca"),
  tlsTrust: () => call<CaStatus>("tls_trust"),
  tlsUntrust: () => call<CaStatus>("tls_untrust"),
};
