import { type ReactNode, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Eye, EyeOff, FolderOpen, FolderPlus, Info, Loader2, PlugZap, RefreshCw, ShieldAlert, SlidersHorizontal, Trash2 } from "lucide-react";
import { AboutDialog } from "@/components/about-dialog";
import { EmptyState } from "@/components/empty-state";
import { EngineSelector } from "@/components/engine-selector";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { aiHealthCheck, aiListModels, listOcrEngines, secretsDelete, secretsGet, secretsSet, setDefaultOcrEngine, watcherAddFolder, watcherListFolders, watcherRemoveFolder, watcherScanNow, watcherSetEnabled, type EngineInfo, type FolderConfig } from "@/lib/ipc";
import { getSetting, setSetting } from "@/lib/db";
import { notifySuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";

const OPENROUTER_MODELS = [
  "openai/gpt-4o-mini",
  "anthropic/claude-3.5-haiku",
  "google/gemini-flash-1.5",
  "meta-llama/llama-3.1-8b-instruct",
];

const DEFAULT_OLLAMA_URL = "http://localhost:11434";

type Provider = "none" | "openrouter" | "ollama";
type Status = "idle" | "testing" | "connected" | "not-configured" | "error";

export function SettingsPage() {
  const { engines, refresh: refreshEngines, loading: enginesLoading } = useOcrEngines();
  const [activeSection, setActiveSection] = useState("ocr");
  const [outputDir, setOutputDir] = useState("");
  const [provider, setProvider] = useState<Provider>("none");
  const [aiNamingEnabled, setAiNamingEnabled] = useState(false);
  const [openRouterKey, setOpenRouterKey] = useState("");
  const [showOpenRouterKey, setShowOpenRouterKey] = useState(false);
  const [openRouterModel, setOpenRouterModel] = useState("openai/gpt-4o-mini");
  const [openRouterStatus, setOpenRouterStatus] = useState<Status>("idle");
  const [openRouterMessage, setOpenRouterMessage] = useState("");
  const [ollamaBaseUrl, setOllamaBaseUrl] = useState(DEFAULT_OLLAMA_URL);
  const [ollamaModel, setOllamaModel] = useState("llama3.1");
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaStatus, setOllamaStatus] = useState<Status>("idle");
  const [ollamaMessage, setOllamaMessage] = useState("");
  const [settingsMessage, setSettingsMessage] = useState("");
  const [folders, setFolders] = useState<FolderConfig[]>([]);
  const [foldersMessage, setFoldersMessage] = useState("");
  const [aboutOpen, setAboutOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const [storedOutput, storedProvider, namingEnabled, orModel, ooModel, key, ollamaUrl] = await Promise.all([
        getSetting("output_dir"),
        getSetting("ai.default_provider"),
        getSetting("ai.naming_enabled"),
        getSetting("openrouter.model"),
        getSetting("ollama.model"),
        secretsGet("openrouter.api_key").catch(() => null),
        secretsGet("ollama.base_url").catch(() => null),
      ]);
      if (cancelled) return;
      setOutputDir(storedOutput ?? "%USERPROFILE%\\Documents\\PDF-Parser\\Processed");
      setProvider(isProvider(storedProvider) ? storedProvider : "none");
      setAiNamingEnabled(namingEnabled === "1" || namingEnabled === "true");
      setOpenRouterModel(orModel ?? "openai/gpt-4o-mini");
      setOllamaModel(ooModel ?? "llama3.1");
      setOpenRouterKey(key ?? "");
      setOllamaBaseUrl(ollamaUrl ?? DEFAULT_OLLAMA_URL);
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function persistProvider(nextProvider: Provider) {
    setProvider(nextProvider);
    await setSetting("ai.default_provider", nextProvider);
  }

  async function persistNamingEnabled(enabled: boolean) {
    setAiNamingEnabled(enabled);
    await setSetting("ai.naming_enabled", enabled ? "1" : "0");
  }

  async function chooseOutputDir() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setOutputDir(selected);
      await setSetting("output_dir", selected);
      setSettingsMessage("Output folder saved.");
      notifySuccess("Output folder saved.");
    }
  }

  async function refreshFolders() {
    try {
      setFolders(await watcherListFolders());
    } catch (error) {
      setFoldersMessage(error instanceof Error ? error.message : String(error));
    }
  }

  async function chooseWatchedFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      const folder = await watcherAddFolder(selected, true);
      setFolders((current) => [folder, ...current.filter((item) => item.path !== folder.path)]);
      setFoldersMessage("Watched folder added.");
      notifySuccess("Watched folder added.");
      await refreshFolders();
    }
  }

  async function toggleWatchedFolder(folder: FolderConfig, enabled: boolean) {
    await watcherSetEnabled(folder.path, enabled);
    await refreshFolders();
  }

  async function setFolderRecursive(folder: FolderConfig, recursive: boolean) {
    await watcherAddFolder(folder.path, recursive);
    await refreshFolders();
  }

  async function scanWatchedFolder(folder: FolderConfig) {
    const queued = await watcherScanNow(folder.path);
    setFoldersMessage(`${queued} new file${queued === 1 ? "" : "s"} queued.`);
    await refreshFolders();
  }

  async function removeWatchedFolder(folder: FolderConfig) {
    await watcherRemoveFolder(folder.path);
    setFolders((current) => current.filter((item) => item.path !== folder.path));
    setFoldersMessage("Watched folder removed.");
    notifySuccess("Watched folder removed.");
  }

  async function saveOpenRouter() {
    setOpenRouterStatus("testing");
    setOpenRouterMessage("Saving securely…");
    try {
      if (openRouterKey.trim()) {
        await secretsSet("openrouter.api_key", openRouterKey.trim());
      } else {
        await secretsDelete("openrouter.api_key");
      }
      await setSetting("openrouter.model", openRouterModel.trim() || "openai/gpt-4o-mini");
      setOpenRouterStatus(openRouterKey.trim() ? "idle" : "not-configured");
      setOpenRouterMessage(openRouterKey.trim() ? "Saved in Stronghold." : "No key stored.");
    } catch (error) {
      setOpenRouterStatus("error");
      setOpenRouterMessage(error instanceof Error ? error.message : String(error));
    }
  }

  async function testOpenRouter() {
    await saveOpenRouter();
    try {
      const ok = await aiHealthCheck("openrouter");
      setOpenRouterStatus(ok ? "connected" : "not-configured");
      setOpenRouterMessage(ok ? "Connected." : "Add an API key to enable OpenRouter.");
    } catch (error) {
      setOpenRouterStatus("error");
      setOpenRouterMessage(error instanceof Error ? error.message : String(error));
    }
  }

  async function saveOllama() {
    setOllamaStatus("testing");
    try {
      const base = ollamaBaseUrl.trim() || DEFAULT_OLLAMA_URL;
      if (base === DEFAULT_OLLAMA_URL) {
        await secretsDelete("ollama.base_url").catch(() => undefined);
      } else {
        await secretsSet("ollama.base_url", base);
      }
      await setSetting("ollama.model", ollamaModel.trim() || "llama3.1");
      setOllamaBaseUrl(base);
      setOllamaStatus("idle");
      setOllamaMessage("Saved.");
    } catch (error) {
      setOllamaStatus("error");
      setOllamaMessage(error instanceof Error ? error.message : String(error));
    }
  }

  async function testOllama() {
    await saveOllama();
    try {
      const [ok, models] = await Promise.all([aiHealthCheck("ollama"), aiListModels("ollama").catch((): string[] => [])]);
      setOllamaModels(models);
      if (models.length > 0 && !models.includes(ollamaModel)) {
        setOllamaModel(models[0]);
        await setSetting("ollama.model", models[0]);
      }
      setOllamaStatus(ok ? "connected" : "not-configured");
      setOllamaMessage(ok ? `Connected${models.length ? ` · ${models.length} model${models.length === 1 ? "" : "s"}` : ""}.` : "Ollama is not reachable on this URL.");
    } catch (error) {
      setOllamaStatus("error");
      setOllamaMessage(error instanceof Error ? error.message : String(error));
    }
  }

  const sections = useMemo(
    () => [
      ["ocr", "OCR"],
      ["ai", "AI providers"],
      ["folders", "Folders"],
      ["library", "Library"],
      ["appearance", "Appearance"],
      ["updates", "Updates"],
      ["about", "About"],
    ] as const,
    [],
  );

  useEffect(() => {
    if (activeSection === "folders") {
      void refreshFolders();
    }
  }, [activeSection]);

  useEffect(() => {
    const setSection = (event: Event) => {
      const section = (event as CustomEvent<string>).detail;
      if (section) setActiveSection(section);
    };
    window.addEventListener("pdf-parser:settings-section", setSection);
    return () => window.removeEventListener("pdf-parser:settings-section", setSection);
  }, []);

  return (
    <div className="mx-auto grid max-w-6xl gap-6 lg:grid-cols-[13rem_1fr]">
      <aside className="h-fit rounded-xl border border-border bg-card/60 p-2">
        {sections.map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setActiveSection(id)}
            className={cn(
              "flex w-full items-center rounded-lg px-3 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-secondary/70 hover:text-foreground",
              activeSection === id && "bg-secondary text-foreground",
            )}
          >
            {label}
          </button>
        ))}
      </aside>

      <main className="space-y-5">
        <header className="overflow-hidden rounded-xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20">
          <div className="flex items-start gap-4">
            <div className="grid size-10 place-items-center rounded-lg border border-border bg-background/60">
              <SlidersHorizontal className="size-5" />
            </div>
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.24em] text-muted-foreground">Control room</p>
              <h1 className="mt-2 text-3xl font-semibold tracking-[-0.05em]">Settings</h1>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
                Keep OCR local by default, choose an optional AI naming provider, and review every proposed filename before it is applied.
              </p>
            </div>
          </div>
        </header>

        <section className="rounded-xl border border-border bg-card/70 p-4">
          <h2 className="text-sm font-medium text-foreground">Get started</h2>
          <ol className="mt-2 list-decimal space-y-1 pl-5 text-sm leading-6 text-muted-foreground">
            <li>Choose an output folder (currently default).</li>
            <li>Optional: configure an AI provider.</li>
            <li>Optional: add a watched folder.</li>
          </ol>
        </section>

        {activeSection === "ocr" && (
          <SettingsCard title="OCR" eyebrow="Local text layer">
            <div className="grid gap-4 md:grid-cols-2">
              <label className="space-y-2">
                <span className="text-sm font-medium">OCR engine</span>
                <select
                  className="h-9 w-full rounded-lg border border-input bg-background px-3 text-sm"
                  disabled={enginesLoading}
                  value={engines.find((engine) => engine.is_default)?.id ?? "tesseract"}
                  onChange={async (event) => {
                    await setDefaultOcrEngine(event.target.value);
                    await refreshEngines();
                  }}
                >
                  {engines.map((engine) => (
                    <option key={engine.id} value={engine.id} disabled={engine.status !== "installed"}>
                      {engine.name} · {engine.status}
                    </option>
                  ))}
                </select>
                <p className="text-xs text-muted-foreground">Install RapidOCR here when higher-accuracy OCR is needed.</p>
              </label>
              <label className="space-y-2">
                <span className="text-sm font-medium">Output folder</span>
                <div className="flex gap-2">
                  <Input value={outputDir} onChange={(event) => setOutputDir(event.target.value)} onBlur={() => void setSetting("output_dir", outputDir)} />
                  <Button type="button" variant="outline" onClick={() => void chooseOutputDir()} aria-label="Choose folder">
                    <FolderOpen className="size-4" />
                    Choose folder
                  </Button>
                </div>
              </label>
            </div>
            <EngineSelector />
            {settingsMessage && <p className="text-xs text-muted-foreground">{settingsMessage}</p>}
          </SettingsCard>
        )}

        {activeSection === "ai" && (
          <SettingsCard title="AI providers" eyebrow="Review before rename">
            <div className="rounded-lg border border-border bg-background/45 p-4 text-sm leading-6 text-muted-foreground">
              OpenRouter sends document text to a cloud API (paid). Ollama runs entirely on your machine (free, offline). API keys are stored in Stronghold; if the keychain is unavailable, cloud AI is disabled rather than saved as plaintext.
            </div>
            <div className="grid gap-4 md:grid-cols-[1fr_auto] md:items-center">
              <label className="space-y-2">
                <span className="text-sm font-medium">Default provider</span>
                <select className="h-9 w-full rounded-lg border border-input bg-background px-3 text-sm" value={provider} onChange={(event) => void persistProvider(event.target.value as Provider)}>
                  <option value="none">None</option>
                  <option value="openrouter">OpenRouter</option>
                  <option value="ollama">Ollama</option>
                </select>
              </label>
              <label className="flex items-center gap-2 rounded-lg border border-border bg-background/45 px-3 py-2 text-sm">
                <input type="checkbox" checked={aiNamingEnabled} onChange={(event) => void persistNamingEnabled(event.target.checked)} />
                Review AI names after OCR
              </label>
            </div>

            <div className="grid gap-4 xl:grid-cols-2">
              <ProviderCard title="OpenRouter" status={openRouterStatus} message={openRouterMessage} cloud>
                <label className="space-y-2">
                  <span className="text-sm font-medium">API key</span>
                  <div className="flex gap-2">
                    <Input type={showOpenRouterKey ? "text" : "password"} value={openRouterKey} onChange={(event) => setOpenRouterKey(event.target.value)} placeholder="sk-or-v1-…" />
                    <Button type="button" variant="outline" onClick={() => setShowOpenRouterKey((shown) => !shown)} aria-label={showOpenRouterKey ? "Hide API key" : "Show API key"}>
                      {showOpenRouterKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                    </Button>
                  </div>
                </label>
                <label className="space-y-2">
                  <span className="text-sm font-medium">Model</span>
                  <Input list="openrouter-models" value={openRouterModel} onChange={(event) => setOpenRouterModel(event.target.value)} />
                  <datalist id="openrouter-models">
                    {OPENROUTER_MODELS.map((model) => <option key={model} value={model} />)}
                  </datalist>
                </label>
                <div className="flex flex-wrap gap-2">
                  <Button type="button" variant="outline" onClick={() => void saveOpenRouter()}>Save</Button>
                  <Button type="button" onClick={() => void testOpenRouter()} disabled={openRouterStatus === "testing"}>
                    {openRouterStatus === "testing" ? <Loader2 className="size-4 animate-spin" /> : <PlugZap className="size-4" />}
                    Test connection
                  </Button>
                </div>
              </ProviderCard>

              <ProviderCard title="Ollama" status={ollamaStatus} message={ollamaMessage}>
                <label className="space-y-2">
                  <span className="text-sm font-medium">Base URL</span>
                  <Input value={ollamaBaseUrl} onChange={(event) => setOllamaBaseUrl(event.target.value)} placeholder={DEFAULT_OLLAMA_URL} />
                </label>
                <label className="space-y-2">
                  <span className="text-sm font-medium">Model</span>
                  <Input list="ollama-models" value={ollamaModel} onChange={(event) => setOllamaModel(event.target.value)} />
                  <datalist id="ollama-models">
                    {[...new Set(["llama3.1", ...ollamaModels])].map((model) => <option key={model} value={model} />)}
                  </datalist>
                </label>
                <div className="flex flex-wrap gap-2">
                  <Button type="button" variant="outline" onClick={() => void saveOllama()}>Save</Button>
                  <Button type="button" onClick={() => void testOllama()} disabled={ollamaStatus === "testing"}>
                    {ollamaStatus === "testing" ? <Loader2 className="size-4 animate-spin" /> : <PlugZap className="size-4" />}
                    Test connection
                  </Button>
                </div>
              </ProviderCard>
            </div>
          </SettingsCard>
        )}

        {activeSection === "folders" && (
          <FoldersSection
            folders={folders}
            message={foldersMessage}
            onAdd={() => void chooseWatchedFolder()}
            onRefresh={() => void refreshFolders()}
            onToggle={(folder, enabled) => void toggleWatchedFolder(folder, enabled)}
            onRecursiveChange={(folder, recursive) => void setFolderRecursive(folder, recursive)}
            onScan={(folder) => void scanWatchedFolder(folder)}
            onRemove={(folder) => void removeWatchedFolder(folder)}
          />
        )}
        {activeSection === "library" && <Stub title="Library defaults" text="Review, preview, delete, and copy-path actions live in the Library panel." />}
        {activeSection === "appearance" && <Stub title="Appearance" text="Dark mode is locked for v0.1.0. Theme options are reserved for a later release." />}
        {activeSection === "updates" && <Stub title="Updates" text="The updater checks signed GitHub release artifacts and prepares installs on quit." />}
        {activeSection === "about" && (
          <SettingsCard title="About" eyebrow="Version and logs">
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-background/45 p-4">
              <div>
                <h3 className="font-medium text-foreground">PDF-Parser</h3>
                <p className="mt-1 text-sm text-muted-foreground">Open app details, folders, updates, and logs.</p>
              </div>
              <Button type="button" onClick={() => setAboutOpen(true)}><Info className="size-4" />Open about</Button>
            </div>
          </SettingsCard>
        )}
        <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />
      </main>
    </div>
  );
}

function useOcrEngines() {
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [loading, setLoading] = useState(true);

  async function refresh() {
    setLoading(true);
    try {
      setEngines(await listOcrEngines());
    } catch {
      setEngines([{ id: "tesseract", name: "Tesseract", description: "Bundled OCR engine", status: "installed", size_mb: 50, is_default: true }]);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  return { engines, loading, refresh };
}

function ProviderCard({ title, status, message, children, cloud = false }: { title: string; status: Status; message: string; children: ReactNode; cloud?: boolean }) {
  return (
    <section className="rounded-xl border border-border bg-background/40 p-4">
      <div className="mb-4 flex items-start justify-between gap-3">
        <div>
          <h3 className="font-medium text-foreground">{title}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{cloud ? "Cloud API · key stored securely" : "Local endpoint · offline capable"}</p>
        </div>
        <StatusBadge status={status} />
      </div>
      <div className="space-y-4">{children}</div>
      {message && <p className="mt-3 flex items-center gap-2 text-xs text-muted-foreground"><ShieldAlert className="size-3.5" />{message}</p>}
    </section>
  );
}

function StatusBadge({ status }: { status: Status }) {
  const label = status === "connected" ? "connected" : status === "testing" ? "testing" : status === "error" ? "error" : status === "not-configured" ? "not configured" : "idle";
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full border px-2 py-1 font-mono text-[10px] uppercase tracking-[0.16em]", status === "connected" && "border-emerald-400/30 text-emerald-300", status === "testing" && "border-amber-400/30 text-amber-300", status === "error" && "border-destructive/40 text-destructive", (status === "idle" || status === "not-configured") && "border-border text-muted-foreground")}>
      {status === "connected" ? <CheckCircle2 className="size-3" /> : <span className="size-1.5 rounded-full bg-current" />}
      {label}
    </span>
  );
}

function FoldersSection({
  folders,
  message,
  onAdd,
  onRefresh,
  onToggle,
  onRecursiveChange,
  onScan,
  onRemove,
}: {
  folders: FolderConfig[];
  message: string;
  onAdd: () => void;
  onRefresh: () => void;
  onToggle: (folder: FolderConfig, enabled: boolean) => void;
  onRecursiveChange: (folder: FolderConfig, recursive: boolean) => void;
  onScan: (folder: FolderConfig) => void;
  onRemove: (folder: FolderConfig) => void;
}) {
  return (
    <SettingsCard title="Watched folders" eyebrow="Automatic intake">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-background/45 p-4">
        <div>
          <h3 className="font-medium text-foreground">Watched folders</h3>
          <p className="mt-1 text-sm text-muted-foreground">New PDFs in enabled folders are debounced, checked for write stability, and queued automatically.</p>
        </div>
        <div className="flex gap-2">
          <Button type="button" variant="outline" onClick={onRefresh}><RefreshCw className="size-4" />Refresh</Button>
          <Button type="button" onClick={onAdd}><FolderPlus className="size-4" />Add folder</Button>
        </div>
      </div>

      {folders.length === 0 ? (
        <EmptyState icon={FolderPlus} title="No watched folders" description="Watch a folder to auto-process new PDFs." actionLabel="Add folder" onAction={onAdd} className="min-h-64 rounded-lg border border-dashed border-border bg-background/35" />
      ) : (
        <div className="space-y-3">
          {folders.map((folder) => {
            const errored = Boolean(folder.last_error);
            return (
              <div key={folder.path} className="grid gap-3 rounded-xl border border-border bg-background/40 p-4 lg:grid-cols-[1fr_auto] lg:items-center">
                <div className="min-w-0 space-y-2">
                  <div className="flex min-w-0 items-center gap-2">
                    <span title={folder.last_error ?? (folder.enabled ? "Active" : "Disabled")} className={cn("size-2.5 shrink-0 rounded-full", errored ? "bg-destructive" : folder.enabled ? "bg-emerald-400" : "bg-muted-foreground")} />
                    <p className="truncate font-medium text-foreground">{folder.path}</p>
                  </div>
                  <div className="flex flex-wrap gap-3 text-xs text-muted-foreground">
                    <span>{folder.file_count} PDF{folder.file_count === 1 ? "" : "s"}</span>
                    <label className="inline-flex items-center gap-2"><input type="checkbox" checked={folder.enabled} onChange={(event) => onToggle(folder, event.target.checked)} />Enabled</label>
                    <label className="inline-flex items-center gap-2"><input type="checkbox" checked={folder.recursive} onChange={(event) => onRecursiveChange(folder, event.target.checked)} />Recursive</label>
                    {folder.last_error ? <span className="text-destructive">{folder.last_error}</span> : null}
                  </div>
                </div>
                <div className="flex flex-wrap gap-2 lg:justify-end">
                  <Button type="button" variant="outline" onClick={() => onScan(folder)}><RefreshCw className="size-4" />Scan now</Button>
                  <Button type="button" variant="destructive" onClick={() => onRemove(folder)}><Trash2 className="size-4" />Remove</Button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {message ? <p className="text-xs text-muted-foreground">{message}</p> : null}
    </SettingsCard>
  );
}

function SettingsCard({ title, eyebrow, children }: { title: string; eyebrow: string; children: ReactNode }) {
  return (
    <section className="space-y-5 rounded-xl border border-border bg-card/70 p-5">
      <div>
        <p className="font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">{eyebrow}</p>
        <h2 className="mt-1 text-xl font-semibold tracking-[-0.04em]">{title}</h2>
      </div>
      {children}
    </section>
  );
}

function Stub({ title, text }: { title: string; text: string }) {
  return (
    <SettingsCard title={title} eyebrow="Settings">
      <div className="rounded-lg border border-dashed border-border bg-background/35 p-6 text-sm text-muted-foreground">{text}</div>
    </SettingsCard>
  );
}

function isProvider(value: string | null): value is Provider {
  return value === "none" || value === "openrouter" || value === "ollama";
}

