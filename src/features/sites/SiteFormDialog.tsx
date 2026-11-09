import { open as openPicker } from "@tauri-apps/plugin-dialog";
import { BoxIcon, FolderOpenIcon, InfoIcon, LoaderCircleIcon, WandSparklesIcon } from "lucide-react";
import { type FormEvent, type ReactNode, useState } from "react";

import { ErrorPanel } from "@/components/error-panel";
import { Mono } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input, NativeSelect } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api/client";
import { useCreateDockerApp, useDetectProject, useSaveSite, useTlsStatus } from "@/lib/api/queries";
import {
  PROJECT_TYPES,
  type Installation,
  type PhpEngine,
  type ProjectType,
  type ServiceStatus,
  type SiteView,
  type Strategy,
} from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { DockerContainerPicker, type PickedContainer } from "./DockerContainerPicker";
import { BACKEND_LABEL, PROJECT_TYPE_LABEL, STRATEGY_LABEL } from "./labels";
import { relativeDocRoot } from "./paths";
import { resolveBackend } from "./php";

type Step = 1 | 2 | 3;
/** Where a new app comes from. */
type Source = "folder" | "docker";

/** A domain-friendly name: "My_Shop new" → "my-shop-new". */
const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 50) || "app";

interface FormState {
  projectRoot: string;
  name: string;
  domain: string;
  documentRoot: string;
  projectType: ProjectType;
  serviceId: string;
  strategy: Strategy;
  upstreamPort: string;
  phpId: string;
  phpEngine: PhpEngine;
  https: boolean;
  httpsRedirect: boolean;
  enabled: boolean;
}

const STEPS: { step: Step; label: string }[] = [
  { step: 1, label: "Project folder" },
  { step: 2, label: "Domain & serving" },
  { step: 3, label: "Review" },
];

const STRATEGY_HINT: Record<Strategy, string> = {
  php: "Laravel and other PHP apps, with the PHP version you pick.",
  static: "Plain files from the document root.",
  reverse_proxy: "An app you run yourself on a local port.",
};

const ENGINE_LABEL: Record<PhpEngine, string> = {
  auto: "Automatic (FrankenPHP when it has this PHP version)",
  fastcgi: "PHP-CGI through the web server",
  frankenphp: "FrankenPHP",
};

function Field({ label, htmlFor, hint, children }: { label: string; htmlFor: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}

export function SiteFormDialog({
  onOpenChange,
  services,
  phpVersions,
  frankens,
  editing,
}: {
  onOpenChange: (open: boolean) => void;
  services: ServiceStatus[];
  phpVersions: Installation[];
  frankens: Installation[];
  editing: SiteView | null;
}) {
  const defaultPhp = (phpVersions.find((p) => p.isDefault) ?? phpVersions[0])?.id ?? "";
  const runningService = services.find((s) => s.health === "running") ?? services[0];
  const [step, setStep] = useState<Step>(editing ? 2 : 1);
  const [source, setSource] = useState<Source>("folder");
  const [picked, setPicked] = useState<PickedContainer | null>(null);
  const docker = !editing && source === "docker";
  const [form, setForm] = useState<FormState>(
    editing
      ? {
          projectRoot: editing.projectRoot,
          name: editing.name,
          domain: editing.domain,
          documentRoot: relativeDocRoot(editing.projectRoot, editing.documentRoot),
          projectType: editing.projectType,
          serviceId: editing.serviceId ?? "",
          strategy: editing.strategy,
          upstreamPort: editing.upstreamPort ? String(editing.upstreamPort) : "",
          phpId: editing.runtimeInstallationId ?? defaultPhp,
          phpEngine: editing.phpEngine,
          https: editing.httpsEnabled,
          httpsRedirect: editing.httpsRedirect,
          enabled: editing.enabled,
        }
      : {
          projectRoot: "",
          name: "",
          domain: "",
          documentRoot: ".",
          projectType: "custom",
          serviceId: "",
          strategy: "static",
          upstreamPort: "",
          phpId: defaultPhp,
          phpEngine: "auto",
          https: false,
          httpsRedirect: false,
          enabled: true,
        },
  );
  const tls = useTlsStatus();
  const detect = useDetectProject();
  const save = useSaveSite();
  const createDocker = useCreateDockerApp();
  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm((f) => ({ ...f, [key]: value }));

  const runDetect = (path: string) => {
    if (!path.trim()) return;
    detect.mutate(path.trim(), {
      onSuccess: (d) => {
        setForm((f) => ({
          ...f,
          projectRoot: d.projectRoot,
          name: d.name,
          domain: d.domain,
          documentRoot: relativeDocRoot(d.projectRoot, d.documentRoot),
          projectType: d.projectType,
          strategy: d.strategy,
          upstreamPort: d.upstreamPort ? String(d.upstreamPort) : "",
        }));
        setStep(2);
      },
    });
  };

  /** The picked container fills the form: a proxy to its published port. */
  const fillFromContainer = (p: PickedContainer) => {
    const name = slug(p.container.compose && p.container.compose.service !== p.container.compose.project ? `${p.container.compose.project}-${p.container.compose.service}` : p.container.name);
    setForm((f) => ({
      ...f,
      projectRoot: p.container.compose?.dir ?? "",
      name,
      domain: `${name}.test`,
      documentRoot: ".",
      projectType: "custom",
      strategy: "reverse_proxy",
      upstreamPort: String(p.port.hostPort),
    }));
    setStep(2);
  };

  /** Editing: only the folder changes (name, domain and serving stay as set). */
  const browseFolder = async () => {
    const picked = await openPicker({ directory: true, title: "Select the project folder", defaultPath: form.projectRoot || undefined });
    if (typeof picked === "string") set("projectRoot", picked);
  };

  const browse = async () => {
    const picked = await openPicker({ directory: true, title: "Select the project folder" });
    if (typeof picked === "string") {
      set("projectRoot", picked);
      runDetect(picked);
    }
  };

  const port = Number(form.upstreamPort);
  const [finding, setFinding] = useState(false);
  const findPort = () => {
    setFinding(true);
    api
      .appPortSuggest(form.projectType, editing?.id ?? null)
      .then((p) => set("upstreamPort", String(p)))
      .catch(() => undefined)
      .finally(() => setFinding(false));
  };
  const php = phpVersions.find((p) => p.id === form.phpId);
  const resolution = php ? resolveBackend(form.phpEngine, php, frankens) : null;
  const strategyOk =
    form.strategy === "static" ||
    (form.strategy === "reverse_proxy" && Number.isInteger(port) && port >= 1 && port <= 65535) ||
    (form.strategy === "php" && resolution !== null && resolution.backend !== null);
  const step2Ok = Boolean(form.name.trim() && form.domain.trim() && services.length > 0 && strategyOk && (!editing || form.projectRoot.trim()));
  const pending = save.isPending || createDocker.isPending;
  const isLocalhost = form.domain.trim().toLowerCase().endsWith(".localhost");
  const domainLower = form.domain.trim().toLowerCase();
  const tlsSuffixOk = (tls.data?.suffixes ?? []).some((s) => domainLower === s || domainLower.endsWith(`.${s}`));
  const httpsBlocker = !tls.data?.exists
    ? "Create Zenv's local certificate authority in Settings to use HTTPS."
    : !tlsSuffixOk
      ? `HTTPS works for domains ending in .${(tls.data?.suffixes ?? []).join(", .")}.`
      : null;
  const service = form.serviceId ? services.find((s) => s.service.id === form.serviceId) : runningService;
  const serverLabel = form.serviceId ? service?.service.name : `any running web server${runningService?.health === "running" ? ` (now ${runningService.service.name})` : ""}`;
  const runsOn =
    resolution && resolution.backend === "frankenphp"
      ? `FrankenPHP ${resolution.franken?.version ?? ""}, behind ${serverLabel ?? "the web server"}`
      : `${serverLabel ?? "the web server"} with ${BACKEND_LABEL.fastcgi}`;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (step === 1) return docker ? picked && fillFromContainer(picked) : runDetect(form.projectRoot);
    if (step === 2 && !editing) return step2Ok && setStep(3);
    if (!step2Ok) return;
    const input = {
          name: form.name,
          domain: form.domain,
          projectRoot: form.projectRoot,
          documentRoot: form.documentRoot || ".",
          projectType: form.projectType,
          serviceId: form.serviceId || null,
          strategy: form.strategy,
          upstreamPort: form.strategy === "reverse_proxy" ? port : null,
          runtimeInstallationId: form.strategy === "php" ? form.phpId : null,
          phpEngine: form.strategy === "php" ? form.phpEngine : ("auto" as const),
          httpsEnabled: form.https && !httpsBlocker,
          httpsRedirect: form.https && !httpsBlocker && form.httpsRedirect,
          enabled: form.enabled,
        };
    if (docker && picked) {
      createDocker.mutate(
        { input, binding: { container: picked.container.name, compose: picked.container.compose, autostart: true } },
        { onSuccess: () => onOpenChange(false) },
      );
      return;
    }
    save.mutate(
      {
        id: editing?.id ?? null,
        input: {
          name: form.name,
          domain: form.domain,
          projectRoot: form.projectRoot,
          documentRoot: form.documentRoot || ".",
          projectType: form.projectType,
          serviceId: form.serviceId || null,
          strategy: form.strategy,
          upstreamPort: form.strategy === "reverse_proxy" ? port : null,
          runtimeInstallationId: form.strategy === "php" ? form.phpId : null,
          phpEngine: form.strategy === "php" ? form.phpEngine : "auto",
          httpsEnabled: form.https && !httpsBlocker,
          httpsRedirect: form.https && !httpsBlocker && form.httpsRedirect,
          enabled: form.enabled,
        },
      },
      { onSuccess: () => onOpenChange(false) },
    );
  };

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{editing ? `Edit ${editing.domain}` : "New app"}</DialogTitle>
          </DialogHeader>

          <ol className={cn("flex gap-1 text-xs", editing && "hidden")} aria-label="Steps">
            {STEPS.map((s) => (
              <li
                key={s.step}
                aria-current={s.step === step ? "step" : undefined}
                className={cn(
                  "flex-1 border-b-2 pb-1.5",
                  s.step === step ? "border-primary font-medium" : s.step < step ? "border-primary/40 text-muted-foreground" : "border-border text-muted-foreground",
                )}
              >
                {s.step}. {s.step === 1 && docker ? "Container" : s.label}
              </li>
            ))}
          </ol>

          {step === 1 && !editing && (
            <div className="grid grid-cols-2 gap-1 rounded-md border p-1" role="radiogroup" aria-label="Where the app comes from">
              {(
                [
                  ["folder", FolderOpenIcon, "Project folder", "PHP, static files or a program Zenv starts"],
                  ["docker", BoxIcon, "Docker container", "An app already running in Docker"],
                ] as const
              ).map(([value, Icon, label, hint]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={source === value}
                  onClick={() => setSource(value)}
                  className={cn(
                    "flex items-start gap-2 rounded-sm px-2.5 py-2 text-left text-sm transition-colors",
                    source === value ? "bg-primary/10 text-foreground" : "text-muted-foreground hover:bg-accent",
                  )}
                >
                  <Icon className="mt-0.5 size-4 shrink-0" />
                  <span>
                    <span className="block font-medium">{label}</span>
                    <span className="block text-xs text-muted-foreground">{hint}</span>
                  </span>
                </button>
              ))}
            </div>
          )}

          {step === 1 && docker && <DockerContainerPicker picked={picked} onPick={setPicked} />}

          {step === 1 && !docker && (
            <Field label="Project folder" htmlFor="site-root">
              <div className="flex gap-2">
                <Input
                  id="site-root"
                  value={form.projectRoot}
                  onChange={(e) => set("projectRoot", e.target.value)}
                  placeholder="D:\Projects\my-laravel-app"
                  className="font-mono text-[12px]"
                  autoFocus
                />
                <Button type="button" variant="outline" onClick={browse}>
                  <FolderOpenIcon /> Browse
                </Button>
              </div>
            </Field>
          )}

          {step === 2 && (
            <div className="grid gap-3">
              {editing && (
                <Field
                  label="Project folder"
                  htmlFor="site-root-edit"
                  hint={
                    form.strategy === "reverse_proxy"
                      ? "The program runs from this folder (and reads its .env); a running program restarts in the new folder."
                      : "The document root below stays relative to this folder."
                  }
                >
                  <div className="flex gap-2">
                    <Input
                      id="site-root-edit"
                      value={form.projectRoot}
                      onChange={(e) => set("projectRoot", e.target.value)}
                      className="font-mono text-[12px]"
                      spellCheck={false}
                    />
                    <Button type="button" variant="outline" onClick={() => void browseFolder()}>
                      <FolderOpenIcon /> Browse
                    </Button>
                  </div>
                </Field>
              )}
              {detect.data && !editing && (
                <div className="rounded-md border border-primary/30 bg-primary/5 p-2.5 text-xs">
                  <p>
                    Detected <span className="font-medium">{PROJECT_TYPE_LABEL[detect.data.projectType]}</span> (found {detect.data.evidence}).
                  </p>
                  {detect.data.notes.map((n) => (
                    <p key={n} className="mt-1 flex gap-1.5 text-muted-foreground">
                      <InfoIcon className="mt-px size-3.5 shrink-0" />
                      {n}
                    </p>
                  ))}
                </div>
              )}
              <div className="grid grid-cols-2 gap-3">
                <Field label="Name" htmlFor="site-name">
                  <Input id="site-name" value={form.name} onChange={(e) => set("name", e.target.value)} maxLength={100} />
                </Field>
                <Field label="Project type" htmlFor="site-type">
                  <NativeSelect id="site-type" value={form.projectType} onChange={(e) => set("projectType", e.target.value as ProjectType)}>
                    {PROJECT_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {PROJECT_TYPE_LABEL[t]}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
              </div>
              <Field
                label="Domain"
                htmlFor="site-domain"
                hint={isLocalhost ? "Works in browsers right away." : "Needs a hosts-file entry; Zenv asks before adding it."}
              >
                <Input id="site-domain" value={form.domain} onChange={(e) => set("domain", e.target.value)} className="font-mono" />
              </Field>
              <Field label="Web server" htmlFor="site-service">
                <NativeSelect id="site-service" value={form.serviceId} onChange={(e) => set("serviceId", e.target.value)}>
                  <option value="">Any running web server</option>
                  {services.map((s) => (
                    <option key={s.service.id} value={s.service.id}>
                      {s.service.name} — {s.service.listenAddress}:{s.service.httpPort}
                      {s.health === "running" ? " (running)" : ""}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              {docker && picked && (
                <div className="flex items-start gap-2 rounded-md border border-primary/30 bg-primary/5 p-2.5 text-xs">
                  <BoxIcon className="mt-px size-3.5 shrink-0" />
                  <p>
                    Proxy to container <span className="font-medium">{picked.container.name}</span>: this computer's{" "}
                    <Mono>:{picked.port.hostPort}</Mono> → the container's <Mono>{picked.port.containerPort}</Mono>.{" "}
                    {picked.container.compose
                      ? `Zenv starts it with docker compose (project ${picked.container.compose.project}) when the app starts.`
                      : "Zenv starts the container when the app starts."}
                  </p>
                </div>
              )}
              <fieldset className={cn("grid gap-1.5", docker && "hidden")}>
                <legend className="mb-1.5 text-xs font-medium">Serve as</legend>
                <div className="grid grid-cols-3 gap-2">
                  {(["php", "static", "reverse_proxy"] as const).map((s) => (
                    <label
                      key={s}
                      className={cn(
                        "flex cursor-pointer flex-col gap-0.5 rounded-md border p-2.5 text-sm",
                        form.strategy === s ? "border-primary bg-primary/5" : "hover:bg-accent",
                      )}
                    >
                      <span className="flex items-center gap-2 font-medium">
                        <input type="radio" name="strategy" value={s} checked={form.strategy === s} onChange={() => set("strategy", s)} className="accent-primary" />
                        {STRATEGY_LABEL[s]}
                      </span>
                      <span className="text-xs text-muted-foreground">{STRATEGY_HINT[s]}</span>
                    </label>
                  ))}
                </div>
              </fieldset>

              {form.strategy === "php" &&
                (phpVersions.length === 0 ? (
                  <p className="rounded-md border border-warning/40 bg-warning/5 p-2.5 text-xs">
                    No PHP version is set up. Add one under Languages & Runtimes first.
                  </p>
                ) : (
                  <div className="grid grid-cols-2 gap-3">
                    <Field label="PHP version" htmlFor="site-php">
                      <NativeSelect id="site-php" value={form.phpId} onChange={(e) => set("phpId", e.target.value)}>
                        {phpVersions.map((p) => (
                          <option key={p.id} value={p.id}>
                            PHP {p.version}
                            {p.isDefault ? " (default)" : ""}
                          </option>
                        ))}
                      </NativeSelect>
                    </Field>
                    <Field label="Runs on" htmlFor="site-engine">
                      <NativeSelect id="site-engine" value={form.phpEngine} onChange={(e) => set("phpEngine", e.target.value as PhpEngine)}>
                        {(["auto", "fastcgi", "frankenphp"] as const).map((e) => (
                          <option key={e} value={e}>
                            {ENGINE_LABEL[e]}
                          </option>
                        ))}
                      </NativeSelect>
                    </Field>
                    <p className={cn("col-span-2 text-xs", resolution?.backend === null ? "text-destructive" : "text-muted-foreground")}>
                      {resolution && resolution.backend === null ? resolution.error : `Served by ${runsOn}.`}
                    </p>
                  </div>
                ))}

              {docker ? null : form.strategy === "reverse_proxy" ? (
                <Field
                  label="App port"
                  htmlFor="site-port"
                  hint="Zenv gives the app this port (PORT / APP_PORT) when it starts it; other settings come from the project's .env, which is not changed."
                >
                  <div className="flex items-center gap-2">
                    <Input
                      id="site-port"
                      inputMode="numeric"
                      value={form.upstreamPort}
                      onChange={(e) => set("upstreamPort", e.target.value.replace(/\D/g, ""))}
                      className="w-32 font-mono"
                    />
                    <Button type="button" variant="outline" size="sm" onClick={findPort} disabled={finding}>
                      {finding ? <LoaderCircleIcon className="animate-spin" /> : <WandSparklesIcon />} Best free port
                    </Button>
                  </div>
                </Field>
              ) : (
                <Field label="Document root" htmlFor="site-docroot" hint="Relative to the project folder; “.” is the folder itself.">
                  <Input id="site-docroot" value={form.documentRoot} onChange={(e) => set("documentRoot", e.target.value)} className="font-mono" />
                </Field>
              )}
              <div className="grid gap-2 rounded-md border p-2.5">
                <label className="flex items-center justify-between gap-3 text-sm">
                  <span>
                    HTTPS
                    <span className="block text-xs text-muted-foreground">
                      {httpsBlocker ?? "Certificate from Zenv's local CA, issued and renewed automatically."}
                    </span>
                  </span>
                  <Switch checked={form.https && !httpsBlocker} disabled={Boolean(httpsBlocker)} onCheckedChange={(v) => set("https", v)} aria-label="HTTPS" />
                </label>
                {form.https && !httpsBlocker && (
                  <label className="flex items-center justify-between gap-3 text-sm">
                    <span>Redirect HTTP to HTTPS</span>
                    <Switch checked={form.httpsRedirect} onCheckedChange={(v) => set("httpsRedirect", v)} aria-label="Redirect HTTP to HTTPS" />
                  </label>
                )}
              </div>
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={form.enabled} onChange={(e) => set("enabled", e.target.checked)} className="size-4 accent-primary" />
                Serve the app now
              </label>
            </div>
          )}

          {step === 3 && (
            <dl className="grid grid-cols-[8rem_1fr] gap-x-3 gap-y-2 rounded-md border p-3 text-sm">
              <dt className="text-muted-foreground">Domain</dt>
              <dd><Mono>{form.domain}</Mono></dd>
              <dt className="text-muted-foreground">Project folder</dt>
              <dd>{form.projectRoot ? <Mono>{form.projectRoot}</Mono> : <span className="text-muted-foreground">Zenv's dev folder (the container has no compose folder)</span>}</dd>
              <dt className="text-muted-foreground">Served as</dt>
              <dd>
                {form.strategy === "php" && php && (
                  <>PHP {php.version} on {runsOn}, from <Mono>{form.documentRoot || "."}</Mono></>
                )}
                {form.strategy === "static" && <>Files from <Mono>{form.documentRoot || "."}</Mono></>}
                {form.strategy === "reverse_proxy" && <>Proxy to <Mono>127.0.0.1:{form.upstreamPort}</Mono></>}
                {docker && picked && <> (container <Mono>{picked.container.name}</Mono>)</>}
              </dd>
              <dt className="text-muted-foreground">Web server</dt>
              <dd>
                {form.serviceId ? service?.service.name : "Any running web server"}
                {service && service.health !== "running" && <span className="text-xs text-muted-foreground"> (stopped — live once it starts)</span>}
              </dd>
              <dt className="text-muted-foreground">HTTPS</dt>
              <dd>{form.https && !httpsBlocker ? (form.httpsRedirect ? "On, HTTP redirects to HTTPS" : "On, HTTP also served") : "Off"}</dd>
              <dt className="text-muted-foreground">State</dt>
              <dd>{form.enabled ? "Enabled" : "Saved, disabled"}</dd>
            </dl>
          )}

          <ErrorPanel error={detect.error ?? save.error ?? createDocker.error} />

          <DialogFooter>
            {step > 1 && !(editing && step === 2) && (
              <Button type="button" variant="ghost" className="mr-auto" onClick={() => setStep((s) => (s - 1) as Step)}>
                Back
              </Button>
            )}
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            {step === 1 && (
              <Button type="submit" disabled={docker ? !picked : !form.projectRoot.trim() || detect.isPending}>
                {detect.isPending ? "Looking at the folder…" : "Continue"}
              </Button>
            )}
            {step === 2 &&
              (editing ? (
                <Button type="submit" disabled={!step2Ok || pending}>
                  {save.isPending ? "Saving and checking configuration…" : "Save changes"}
                </Button>
              ) : (
                <Button type="submit" disabled={!step2Ok}>
                  Review
                </Button>
              ))}
            {step === 3 && (
              <Button type="submit" disabled={pending}>
                {createDocker.isPending
                  ? "Creating the app and starting the container…"
                  : save.isPending
                    ? "Starting PHP and checking configuration…"
                    : editing
                      ? "Save changes"
                      : "Create app"}
              </Button>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
