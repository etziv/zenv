// Mirrors of the Rust IPC models (serde camelCase). Keep in sync with
// src-tauri/src/{runtime_catalog,runtime_manager,service_manager,...}.rs.

export const COMPONENT_IDS = [
  "apache",
  "nginx",
  "caddy",
  "frankenphp",
  "php",
  "go",
  "rust",
  "ruby",
  "python",
  "node",
  "mysql",
  "postgresql",
  "redis",
  "omniroute",
  "ninerouter",
] as const;
export type ComponentId = (typeof COMPONENT_IDS)[number];

export type Category = "web_server" | "language" | "database" | "cache" | "tool";
export type WindowsSupport = "native" | "native_with_caveats" | "no_official_build";
export type IntegrationLevel = "service_managed" | "detection_only";

export interface ComponentInfo {
  id: ComponentId;
  name: string;
  category: Category;
  windowsSupport: WindowsSupport;
  integration: IntegrationLevel;
  installable: boolean;
  distribution: string;
  notes: string[];
  plannedMilestone: number;
}

export interface Installation {
  id: string;
  component: ComponentId;
  version: string;
  versionDetail: string;
  rootPath: string;
  executablePath: string;
  source: "external" | "managed";
  isDefault: boolean;
  enabled: boolean;
  executableExists: boolean;
  detectedAt: string;
  createdAt: string;
  updatedAt: string;
}

export interface DiscoveredInstallation {
  component: ComponentId;
  executablePath: string;
  rootPath: string | null;
  version: string | null;
  versionDetail: string | null;
  error: string | null;
  registeredId: string | null;
}

export interface Service {
  id: string;
  component: ComponentId;
  name: string;
  installationId: string;
  listenAddress: string;
  httpPort: number;
  autostart: boolean;
  createdAt: string;
  updatedAt: string;
  /** Databases and caches: their data folder (never deleted by Zenv). */
  dataDir: string | null;
  /** Web servers: TLS port, listened on while a site it serves uses HTTPS. */
  httpsPort: number | null;
}

export interface ServiceInput {
  name: string;
  installationId: string;
  listenAddress: string;
  httpPort: number;
  autostart: boolean;
  dataDir?: string | null;
  httpsPort?: number | null;
}

export type DataDirState = "missing" | "empty" | "initialized" | "foreign" | "incompatible";

export interface DataDirInfo {
  state: DataDirState;
  detail: string;
}

export interface ConnectionInfo {
  host: string;
  port: number;
  user: string | null;
  /** "" means no password. */
  password: string | null;
  database: string | null;
  url: string;
  note: string | null;
}

export type Health = "running" | "stopped" | "failed";

export interface ExitInfo {
  code: number | null;
  at: string;
}

export type ProcessState =
  | { state: "running"; pid: number; pids: number[]; startedAt: string }
  | { state: "exited"; exit: ExitInfo }
  | { state: "notStarted" };

export interface ServiceStatus {
  service: Service;
  installation: Installation | null;
  health: Health;
  process: ProcessState;
  url: string;
  configPath: string;
  documentRoot: string;
  kind: Category;
  /** Name of the web server holding this one's port and passing its apps on. */
  behind: string | null;
  /** The data folder must be initialized by Zenv before the first start (MySQL, PostgreSQL). */
  needsInitialization: boolean;
  dataDirInfo: DataDirInfo | null;
  connection: ConnectionInfo | null;
}

export interface ServiceEvent {
  id: number;
  level: "info" | "warning" | "error";
  message: string;
  createdAt: string;
}

/** Log files of one PHP or FrankenPHP installation. */
export interface RuntimeLogGroup {
  installationId: string;
  label: string;
  sources: LogSource[];
}

export interface LogSource {
  id: string;
  label: string;
  path: string;
  exists: boolean;
  sizeBytes: number;
}

export interface LogTail {
  path: string;
  exists: boolean;
  sizeBytes: number;
  lines: string[];
  truncated: boolean;
}

export interface ValidationReport {
  ok: boolean;
  output: string;
  checkedFile: string;
}

export interface PortOwner {
  pid: number;
  processName: string | null;
  address: string;
}

export type PortInspection = { port: number; address: string } & (
  | { status: "free" }
  | { status: "inUse"; owners: PortOwner[] }
  | { status: "unavailable"; reason: string }
);

export type CheckStatus = "ok" | "warning" | "error";

export interface Check {
  id: string;
  label: string;
  status: CheckStatus;
  detail: string;
}

export interface AppPaths {
  root: string;
  dbFile: string;
  binDir: string;
  dataDir: string;
  generatedDir: string;
  logsDir: string;
  devDir: string;
  backupsDir: string;
  downloadsDir: string;
}

export interface AppInfo {
  version: string;
  schemaVersion: number;
  os: string;
  arch: string;
  paths: AppPaths;
}

export type RevealTarget =
  | { kind: "dataDir" }
  | { kind: "logsDir" }
  | { kind: "serviceConfig"; id: string }
  | { kind: "serviceDocumentRoot"; id: string }
  | { kind: "serviceLogs"; id: string }
  | { kind: "serviceDataDir"; id: string }
  | { kind: "installationRoot"; id: string }
  | { kind: "siteProjectRoot"; id: string }
  | { kind: "siteConfig"; id: string }
  | { kind: "siteLog"; id: string; log: string }
  | { kind: "serviceLog"; id: string; log: string }
  | { kind: "runtimeLog"; id: string; log: string }
  | { kind: "appLog" };

export type ErrorCode =
  | "not_found"
  | "validation"
  | "invalid_path"
  | "conflict"
  | "port_unavailable"
  | "config_invalid"
  | "start_failed"
  | "detection_failed"
  | "unsupported"
  | "database"
  | "migration"
  | "template"
  | "io"
  | "platform"
  | "elevation_declined"
  | "ipc";

export type ErrorDetails =
  | { port: number; owners: PortOwner[]; reason: string | null }
  | { output: string }
  | { logTail: string[] }
  | { path: string; reason: string }
  | { entity: string; id: string }
  | null;

// ----- local sites ------------------------------------------------------------

export const PROJECT_TYPES = ["laravel", "php", "static", "go", "rust", "python", "ruby", "custom"] as const;
export type ProjectType = (typeof PROJECT_TYPES)[number];
export type Strategy = "static" | "reverse_proxy" | "php";
export type PhpEngine = "auto" | "fastcgi" | "frankenphp";
export type PhpBackend = "fastcgi" | "frankenphp";
export type ConfigStatus = "pending" | "applied" | "failed" | "disabled";
export type Resolution = "hosts_file" | "loopback";
export type DomainState = "not_needed" | "managed" | "external" | "conflict" | "missing";

export interface Site {
  id: string;
  name: string;
  domain: string;
  projectRoot: string;
  documentRoot: string;
  projectType: ProjectType;
  /** null: served by whichever web server is running. */
  serviceId: string | null;
  strategy: Strategy;
  upstreamPort: number | null;
  runtimeInstallationId: string | null;
  phpEngine: PhpEngine;
  httpsEnabled: boolean;
  httpsRedirect: boolean;
  enabled: boolean;
  configStatus: ConfigStatus;
  configMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SiteView extends Site {
  serviceName: string;
  url: string;
  resolution: Resolution;
  hostsState: DomainState;
  documentRootExists: boolean;
  phpVersion: string | null;
  phpBackend: PhpBackend | null;
  /** Whether FrankenPHP/Caddy/Nginx can serve it, or it needs Apache (.htaccess rules). */
  compat: { frankenphpReady: boolean; needsApache: string[] };
}

export interface SiteInput {
  name: string;
  domain: string;
  projectRoot: string;
  documentRoot: string;
  projectType: ProjectType;
  /** null: served by whichever web server is running. */
  serviceId: string | null;
  strategy: Strategy;
  upstreamPort: number | null;
  runtimeInstallationId: string | null;
  phpEngine: PhpEngine;
  httpsEnabled: boolean;
  httpsRedirect: boolean;
  enabled: boolean;
}

export interface SiteQuery {
  search: string;
  offset: number;
  limit: number;
}

export interface SitePage {
  items: SiteView[];
  total: number;
  offset: number;
  limit: number;
}

export interface DetectedProject {
  projectRoot: string;
  name: string;
  domain: string;
  projectType: ProjectType;
  documentRoot: string;
  strategy: Strategy;
  upstreamPort: number | null;
  evidence: string;
  notes: string[];
}

export interface ApplyReport {
  serviceId: string;
  sites: number;
  changedFiles: number;
  removedFiles: number;
  running: boolean;
  reloaded: boolean;
}

export interface HostsEntry {
  ip: string;
  domain: string;
}

export interface HostsPlan {
  changed: boolean;
  add: HostsEntry[];
  remove: HostsEntry[];
  alreadyMapped: string[];
  conflicts: { domain: string; mappedTo: string }[];
  block: string[];
}

export interface HostsStatus {
  path: string;
  plan: HostsPlan;
}

export interface HostsApplyOutcome {
  method: "unchanged" | "direct" | "elevated";
  plan: HostsPlan;
  backup: string | null;
}

/** A service the enabled apps need. */
export interface NeededService {
  serviceId: string;
  name: string;
  kind: Category;
  running: boolean;
  /** e.g. "shop.test (DB_PORT=3307)" */
  usedBy: string[];
}

export interface StartOutcome {
  serviceId: string;
  name: string;
  result: "started" | "already_running" | "stopped" | "failed";
  error: string | null;
}

/** The program behind a proxy app (cargo run, npm run dev, ...). */
/** A port a container publishes on this computer. */
export interface PublishedPort {
  hostPort: number;
  containerPort: number;
}

/** The docker compose service a container belongs to. */
export interface ComposeService {
  project: string;
  service: string;
  dir: string | null;
  files: string[];
}

export interface DockerContainer {
  name: string;
  image: string;
  /** running, exited, created, paused, restarting, dead */
  state: string;
  status: string;
  /** Empty: the container publishes no port, so Windows can't reach it. */
  ports: PublishedPort[];
  compose: ComposeService | null;
}

/** The container an app is tied to. */
export interface DockerBinding {
  container: string;
  compose: ComposeService | null;
  autostart: boolean;
}

export interface DockerAppStatus {
  container: string;
  compose: ComposeService | null;
  /** Docker's state, "missing" (container gone) or "unknown" (Docker not answering). */
  state: string;
  exitCode: number | null;
  image: string | null;
  engineRunning: boolean;
  problem: string | null;
}

export interface AppProcessStatus {
  siteId: string;
  /** Saved command, else the one detected from the project; null when unknown. */
  command: string | null;
  /** Detected, not saved: runs only when the user starts the app (which approves and saves it). */
  detected: boolean;
  /** What the command runs, from package.json ("vite" for "npm run dev"). */
  runs: string | null;
  autostart: boolean;
  /** "starting": the process runs but its port does not answer yet. */
  state: "running" | "starting" | "stopped" | "exited";
  exitCode: number | null;
  port: number | null;
  /** Something answers on the port (Zenv's process or one started elsewhere). */
  listening: boolean;
  /** Other devices on the network can reach the port directly (it listens on every address). */
  exposed: boolean;
  /** How the program gets Zenv's port, e.g. ["APP_PORT=8101", "PORT=8101"]. */
  portVars: string[];
  logFile: string;
  /** The app runs in this container instead of a program Zenv starts. */
  docker: DockerAppStatus | null;
}

/** Another folder on PATH holding the same programs as a component. */
export interface PathConflict {
  entry: string;
  scope: "system" | "user";
  program: string;
}

/** Which version Zenv put on the user PATH, and what else on PATH competes. */
export interface PathStatus {
  component: ComponentId;
  eligible: boolean;
  installationId: string | null;
  conflicts: PathConflict[];
}

/** AI routers Zenv runs as Tools services. */
export const ROUTERS = ["omniroute", "ninerouter"] as const;
export type RouterId = (typeof ROUTERS)[number];
export const isRouter = (c: ComponentId | undefined): c is RouterId => ROUTERS.includes(c as RouterId);

/** Codex CLI's config (~/.codex/config.toml) as it concerns one AI router. */
export interface CodexStatus {
  configPath: string;
  exists: boolean;
  configured: boolean;
  baseUrl: string | null;
  isDefault: boolean;
  /** The provider Codex uses without a profile, e.g. "ninerouter". */
  topLevelProvider: string | null;
  /** The profile Codex uses by default, if any. */
  defaultProfile: string | null;
  /** The model the router's profile asks for, if set. */
  profileModel: string | null;
  /** The model Codex asks this router for (top-level when it is the default through model_provider). */
  model: string | null;
  /** Top-level limits; they apply to whichever provider is the default. */
  contextWindow: number | null;
  maxOutput: number | null;
}

/** One model or combo a router offers on /v1/models. */
export interface RouterModel {
  id: string;
  ownedBy: string | null;
  combo: boolean;
  contextWindow: number | null;
  maxOutput: number | null;
}

/** An AI router's installed and newest release, and the auto-update setting. */
export interface RouterStatus {
  component: RouterId;
  installed: string | null;
  latest: string | null;
  /** When auto-update may install `latest` (new releases wait a week); null once it may. */
  autoFrom: string | null;
  autoUpdate: boolean;
  lastChecked: string | null;
  updating: boolean;
  lastError: string | null;
  /** Dashboard pages Zenv links to. */
  pages: { id: string; label: string }[];
}

/** Tailscale on this computer. */
export interface TailnetStatus {
  installed: boolean;
  online: boolean;
  /** Short MagicDNS name, e.g. "my-laptop". */
  host: string | null;
  dnsName: string | null;
  ip: string | null;
  /** Why it cannot be used, when it cannot. */
  problem: string | null;
  /** This computer's tags (Tailscale Services need a tagged host). */
  tags: string[];
  /** HTTPS certificates are on in the tailnet. */
  https: boolean;
}

/** An app opened to the devices on the user's Tailscale network. */
export interface TailnetApp {
  siteId: string;
  port: number;
  /** http://my-laptop:9001 */
  url: string | null;
  fullUrl: string | null;
  /** http://100.101.102.103:9001 */
  ipUrl: string | null;
  /** Its Tailscale Service name, e.g. "vaultwarden". */
  service: string | null;
  /** https://vaultwarden.tail1234.ts.net */
  serviceUrl: string | null;
}

/** Screenshot of an app for the Apps grid. */
export interface Thumbnail {
  dataUrl: string;
  /** Seconds since the Unix epoch. */
  capturedAt: number;
}

export interface CaStatus {
  exists: boolean;
  certPath: string;
  subject: string | null;
  sha256: string | null;
  notAfter: string | null;
  /** In the current user's Trusted Root store. */
  trustedUser: boolean;
  /** In the machine-wide store (added outside Zenv). */
  trustedMachine: boolean;
  suffixes: string[];
}

export interface RuntimeProcess {
  kind: "php-cgi" | "frankenphp";
  installationId: string;
  version: string;
  ports: number[];
  running: number;
}

// ----- installer --------------------------------------------------------------

export interface AvailableVersion {
  component: ComponentId;
  version: string;
  stable: boolean;
  filename: string;
  url: string;
  sha256: string;
  size: number;
  source: string;
  installedId: string | null;
  /** How the download is checked, e.g. "SHA-256 published by the source". */
  verifiedBy: string;
}

export type InstallPhase = "downloading" | "verifying" | "extracting" | "validating" | "done" | "failed";

export interface InstallJob {
  id: string;
  component: ComponentId;
  version: string;
  phase: InstallPhase;
  downloaded: number;
  total: number;
  error: string | null;
  installationId: string | null;
  startedAt: string;
}

// ----- Studio -----------------------------------------------------------------

export type AgentId = "claudeCode" | "codex" | "shell";

export interface AgentInfo {
  id: AgentId;
  name: string;
  /** The program Zenv runs; null when it is not installed. */
  program: string | null;
  installHint: string;
}

/** A terminal's output (raw bytes) or its end. */
export type TerminalMessage = ArrayBuffer | Uint8Array | number[] | { exit: number | null };

/** A line from Studio's Claude Code chat: Claude Code's stream-json, trimmed by Zenv. */
export interface ChatLine {
  type: string;
  [key: string]: unknown;
}

export type ChatEffort = "low" | "medium" | "high" | "xhigh" | "max";
export type ChatMode = "manual" | "acceptEdits" | "auto" | "plan" | "dontAsk" | "bypassPermissions";

/** How Claude Code starts; null keeps the user's own Claude Code settings. */
export interface ChatOptions {
  model: string | null;
  effort: ChatEffort | null;
  mode: ChatMode | null;
  thinking: boolean | null;
  fast: boolean | null;
  outputStyle: string | null;
  /** Switch to another model when a message is flagged. */
  switchOnFlag: boolean | null;
}

/** What Studio may ask a running Claude Code besides prompts. */
export type ChatControl = { kind: "mcpStatus" } | { kind: "mcpToggle"; server: string; enabled: boolean } | { kind: "mcpReconnect"; server: string };

export interface McpServer {
  name: string;
  /** connected, failed, pending, needs-auth, disabled */
  status: string;
  scope?: string | null;
  error?: string | null;
  version?: string | null;
  tools: number;
}

/** A model Claude Code offers, as its own /model menu names it. */
export interface ChatModel {
  /** What `--model` takes; "default" is the account's default. */
  value: string;
  resolvedModel?: string;
  displayName: string;
  description?: string;
  supportedEffortLevels?: ChatEffort[];
  supportsFastMode?: boolean;
}

export interface ChatCommand {
  name: string;
  description?: string;
  argumentHint?: string;
}

export interface ChatCatalog {
  models: ChatModel[];
  commands: ChatCommand[];
  account?: { email?: string; organization?: string; subscriptionType?: string } | null;
  outputStyles?: string[] | null;
  outputStyle?: string | null;
  version?: string | null;
}

/** A Claude Code session saved for the project. */
export interface ChatSession {
  id: string;
  title: string;
  /** Milliseconds since 1970. */
  modified: number;
}

/** A file pasted or dropped into the chat: images and other files as base64, text as text. */
export interface ChatAttachment {
  kind: "image" | "text" | "file";
  name: string;
  mediaType: string;
  data: string;
}

export type EditorId =
  | "vscode"
  | "vscodeInsiders"
  | "cursor"
  | "windsurf"
  | "antigravity"
  | "kiro"
  | "trae"
  | "zed"
  | "phpstorm"
  | "webstorm"
  | "idea"
  | "goland"
  | "rustrover"
  | "pycharm"
  | "sublime"
  | "notepadPlusPlus";

export interface EditorInfo {
  id: EditorId;
  name: string;
  program: string;
  /** The editor's own icon, as a data URL. */
  icon: string | null;
}

export interface Editors {
  editors: EditorInfo[];
  /** The one "Open in editor" uses: the last one picked, else the first found. */
  preferred: EditorId | null;
}

// ----- Project files (Studio's editor) ------------------------------------------

export interface ProjectEntry {
  name: string;
  /** Relative to the project root, with `/`. */
  path: string;
  dir: boolean;
  size: number;
}

export interface ProjectListing {
  entries: ProjectEntry[];
  truncated: boolean;
}

export interface ProjectFile {
  path: string;
  content: string;
  /** Last change (ms since 1970); passed back when saving. */
  modified: number;
  readOnly: boolean;
}
