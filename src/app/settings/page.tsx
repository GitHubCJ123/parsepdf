import { type ReactNode, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { CheckCircle2, ChevronDown, Copy, DownloadCloud, FolderOpen, Info, Layers, Loader2, PlugZap, RefreshCw, RotateCw, ShieldAlert, ShieldCheck, SlidersHorizontal, Sparkles, Trash2 } from "lucide-react";
import { AboutDialog } from "@/components/about-dialog";
import { EngineSelector } from "@/components/engine-selector";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { aiHealthCheck, aiListModels, debugDumpState, debugResetLibrary, listOcrEngines, secretsDelete, secretsGet, secretsSet, setDefaultOcrEngine, type DebugStateDump, type EngineInfo } from "@/lib/ipc";
import { getSetting, setSetting } from "@/lib/db";
import { notifySuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";

const DEFAULT_OLLAMA_URL = "http://localhost:11434";

type Provider = "none" | "ollama";
type Status = "idle" | "testing" | "connected" | "not-configured" | "error";

export function SettingsPage() {
  const { engines, refresh: refreshEngines, loading: enginesLoading } = useOcrEngines();
  const [activeSection, setActiveSection] = useState("ocr");
  const [outputDir, setOutputDir] = useState("");
  const [provider, setProvider] = useState<Provider>("none");
  const [ollamaBaseUrl, setOllamaBaseUrl] = useState(DEFAULT_OLLAMA_URL);
  const [ollamaModel, setOllamaModel] = useState("llama3.1");
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaStatus, setOllamaStatus] = useState<Status>("idle");
  const [ollamaMessage, setOllamaMessage] = useState("");
  const [settingsMessage, setSettingsMessage] = useState("");
  const [aboutOpen, setAboutOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const [storedOutput, storedProvider, ooModel, ollamaUrl] = await Promise.all([
        getSetting("output_dir"),
        getSetting("ai.default_provider"),
        getSetting("ollama.model"),
        secretsGet("ollama.base_url").catch(() => null),
      ]);
      if (cancelled) return;
      setOutputDir(storedOutput ?? "%USERPROFILE%\\Documents\\PDF-Parser\\Processed");
      const validProvider = isProvider(storedProvider) ? storedProvider : "none";
      setProvider(validProvider);
      // Migrate away from the now-removed OpenRouter provider so chat falls back
      // to a supported backend instead of a dead cloud config.
      if (storedProvider != null && storedProvider !== validProvider) {
        await setSetting("ai.default_provider", validProvider);
      }
      setOllamaModel(ooModel ?? "llama3.1");
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

  async function chooseOutputDir() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setOutputDir(selected);
      await setSetting("output_dir", selected);
      setSettingsMessage("Output folder saved.");
      notifySuccess("Output folder saved.");
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

  // Auto-discover Ollama models whenever the AI tab is opened or the base URL changes.
  // Failures are silent (Ollama may simply not be running); the user can still type a model name.
  useEffect(() => {
    if (activeSection !== "ai") return;
    let cancelled = false;
    (async () => {
      try {
        const models = await aiListModels("ollama");
        if (cancelled) return;
        setOllamaModels(models);
        if (models.length > 0 && !models.includes(ollamaModel)) {
          setOllamaModel(models[0]);
          await setSetting("ollama.model", models[0]);
        }
      } catch {
        // Ignored: silent failure (e.g. Ollama not running). User still has manual input.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeSection, ollamaBaseUrl]);

  const sections = useMemo(
    () => [
      ["ocr", "OCR"],
      ["ai", "AI providers"],
      ["library", "Library"],
      ["updates", "Updates"],
      ["about", "About"],
      ["diagnostics", "Diagnostics"],
    ] as const,
    [],
  );

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

        {activeSection === "ocr" && (
          <SettingsCard title="OCR" eyebrow="Local text layer">
            <div className="grid gap-4 md:grid-cols-2">
              <label className="space-y-2">
                <span className="flex items-center gap-1.5 text-sm font-medium">
                  OCR engine
                  <span
                    tabIndex={0}
                    role="note"
                    aria-label="Tesseract is the bundled, fast default and is best for clean Latin-script text. RapidOCR (PP-OCRv5) is an opt-in download with higher accuracy on low-quality scans and strong CJK support."
                    title="Tesseract: fast, bundled, best for clean Latin-script text. RapidOCR (PP-OCRv5): opt-in download, higher accuracy on low-quality scans and CJK."
                    className="inline-flex cursor-help items-center rounded text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <Info className="size-3.5" />
                  </span>
                </span>
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
                <p className="text-xs text-muted-foreground">Tesseract is fast and bundled; install RapidOCR for higher accuracy on tough or non-Latin scans. See the comparison below.</p>
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
          <SettingsCard title="AI providers" eyebrow="Chat & naming">
            <div className="flex items-start gap-3 rounded-xl border border-border bg-background/45 p-4">
              <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-secondary/50 text-foreground">
                <Sparkles className="size-4" />
              </span>
              <p className="text-sm leading-6 text-muted-foreground">
                Ollama runs entirely on your machine — free, offline, and private. Point PDF-Parser at your local Ollama server and pick a model to power Chat over your library. Toggle it off to keep the app fully OCR-only.
              </p>
            </div>

            <ProviderCard
              title="Ollama"
              status={ollamaStatus}
              message={ollamaMessage}
              enabled={provider === "ollama"}
              onToggle={(next) => void persistProvider(next ? "ollama" : "none")}
            >
              <label className="block space-y-2">
                <span className="text-sm font-medium">Base URL</span>
                <Input className="h-10" value={ollamaBaseUrl} onChange={(event) => setOllamaBaseUrl(event.target.value)} placeholder={DEFAULT_OLLAMA_URL} />
              </label>
              <label className="block space-y-2">
                <span className="text-sm font-medium">Model</span>
                {ollamaModels.length > 0 ? (
                  <SettingSelect widthClass="w-full" value={ollamaModels.includes(ollamaModel) ? ollamaModel : ollamaModels[0]} onChange={setOllamaModel}>
                    {ollamaModels.map((model) => <option key={model} value={model} className="bg-popover text-popover-foreground">{model}</option>)}
                  </SettingSelect>
                ) : (
                  <>
                    <Input className="h-10" list="ollama-models" value={ollamaModel} onChange={(event) => setOllamaModel(event.target.value)} placeholder="No installed models detected" />
                    <datalist id="ollama-models">
                      {[...new Set(["llama3.1", ...ollamaModels])].map((model) => <option key={model} value={model} />)}
                    </datalist>
                    <p className="text-xs leading-5 text-muted-foreground">Run an Ollama model first (e.g. <code className="rounded bg-secondary px-1 py-0.5 font-mono">ollama pull llama3.1</code>) then test the connection.</p>
                  </>
                )}
              </label>
              <div className="flex flex-wrap gap-2 pt-1">
                <Button type="button" variant="outline" onClick={() => void saveOllama()}>Save</Button>
                <Button type="button" onClick={() => void testOllama()} disabled={ollamaStatus === "testing"}>
                  {ollamaStatus === "testing" ? <Loader2 className="size-4 animate-spin" /> : <PlugZap className="size-4" />}
                  Test connection
                </Button>
              </div>
            </ProviderCard>
          </SettingsCard>
        )}

        {activeSection === "library" && <LibrarySettings />}
        {activeSection === "updates" && <UpdatesSettings />}
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
        {activeSection === "diagnostics" && <DiagnosticsCard />}
        <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />
      </main>
    </div>
  );
}

function LibrarySettings() {
  const [confirmDelete, setConfirmDelete] = useState(true);
  const [pageSize, setPageSize] = useState("300");
  const [duplicateHandling, setDuplicateHandling] = useState("confirm");
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const [confirmValue, sizeValue, dupValue] = await Promise.all([
        getSetting("library.confirm_delete"),
        getSetting("library.page_size"),
        getSetting("library.duplicate_handling"),
      ]);
      if (cancelled) return;
      // Confirmation defaults ON so a brand-new install can't delete without asking.
      setConfirmDelete(confirmValue == null ? true : confirmValue === "1");
      setPageSize(sizeValue ?? "300");
      setDuplicateHandling(dupValue === "skip-silent" ? "skip-silent" : "confirm");
      setLoaded(true);
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function persistConfirmDelete(value: boolean) {
    setConfirmDelete(value);
    await setSetting("library.confirm_delete", value ? "1" : "0");
    notifySuccess("Library settings saved.");
  }

  async function persistPageSize(value: string) {
    setPageSize(value);
    await setSetting("library.page_size", value);
    notifySuccess("Library settings saved.");
  }

  async function persistDuplicateHandling(value: string) {
    setDuplicateHandling(value);
    await setSetting("library.duplicate_handling", value);
    notifySuccess("Library settings saved.");
  }

  return (
    <SettingsCard title="Library defaults" eyebrow="Archive behaviour">
      <p className="text-sm leading-6 text-muted-foreground">
        Control how the Library handles duplicates, deletes documents, and how many it loads at once. Preview, open, and copy-path actions live in the Library panel itself.
      </p>

      <div className="divide-y divide-border/70 overflow-hidden rounded-xl border border-border bg-background/40">
        <SettingRow
          icon={Trash2}
          title="Confirm before deleting"
          description="Ask for confirmation before a document is removed. Deleting always removes the processed PDF from disk."
        >
          <Toggle checked={confirmDelete} disabled={!loaded} onChange={(value) => void persistConfirmDelete(value)} label="Confirm before deleting" />
        </SettingRow>

        <SettingRow
          icon={Copy}
          title="On duplicate upload"
          description={'"Ask me" shows a dialog (open existing or reprocess); "Skip silently" just opens the existing document. A different file with the same name is always saved as a new version.'}
        >
          <SettingSelect value={duplicateHandling} disabled={!loaded} onChange={(value) => void persistDuplicateHandling(value)}>
            <option value="confirm">Ask me</option>
            <option value="skip-silent">Skip silently</option>
          </SettingSelect>
        </SettingRow>

        <SettingRow
          icon={Layers}
          title="Documents to load"
          description="How many of the most recent documents to fetch when opening the Library."
        >
          <SettingSelect value={pageSize} disabled={!loaded} onChange={(value) => void persistPageSize(value)}>
            <option value="100">100</option>
            <option value="200">200</option>
            <option value="300">300</option>
            <option value="500">500</option>
          </SettingSelect>
        </SettingRow>
      </div>
    </SettingsCard>
  );
}

function SettingRow({ icon: Icon, title, description, children }: { icon: typeof Trash2; title: string; description: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-start gap-3">
        <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-secondary/50 text-muted-foreground">
          <Icon className="size-4" />
        </span>
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground">{title}</div>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
        </div>
      </div>
      <div className="shrink-0 self-start sm:self-center sm:pl-4">{children}</div>
    </div>
  );
}

function SettingSelect({ value, disabled, onChange, children, widthClass = "w-44" }: { value: string; disabled?: boolean; onChange: (value: string) => void; children: ReactNode; widthClass?: string }) {
  return (
    <div className={cn("relative", widthClass)}>
      <select
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        className="h-10 w-full appearance-none rounded-lg border border-input bg-background pl-3 pr-9 text-sm font-medium text-foreground outline-none transition-colors hover:border-foreground/30 focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 [color-scheme:dark]"
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
    </div>
  );
}

function Toggle({ checked, disabled, onChange, label }: { checked: boolean; disabled?: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
        checked ? "border-primary bg-primary" : "border-input bg-secondary",
      )}
    >
      <span
        className={cn(
          "inline-block size-4 rounded-full bg-background shadow-sm transition-transform",
          checked ? "translate-x-[1.375rem]" : "translate-x-1",
        )}
      />
    </button>
  );
}

function DiagnosticsCard() {
  const [dump, setDump] = useState<DebugStateDump | null>(null);
  const [loading, setLoading] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const state = await debugDumpState();
      setDump(state);
    } catch (error) {
      console.error("[diagnostics] dump failed", error);
    } finally {
      setLoading(false);
    }
  }

  async function reset() {
    setResetting(true);
    try {
      const deleted = await debugResetLibrary();
      notifySuccess(`Wiped ${deleted} document${deleted === 1 ? "" : "s"} and all related rows.`);
      setConfirmReset(false);
      await refresh();
    } catch (error) {
      console.error("[diagnostics] reset failed", error);
    } finally {
      setResetting(false);
    }
  }

  const docsById = new Map((dump?.documents ?? []).map((doc) => [doc.id, doc]));
  const grouped = new Map<number | null, DebugStateDump["jobs"]>();
  for (const job of dump?.jobs ?? []) {
    const key = job.document_id;
    const list = grouped.get(key) ?? [];
    list.push(job);
    grouped.set(key, list);
  }

  return (
    <SettingsCard title="Diagnostics" eyebrow="State and reset">
      <div className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" onClick={() => void refresh()} disabled={loading}>{loading ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}Dump current state</Button>
          {!confirmReset ? (
            <Button type="button" variant="destructive" onClick={() => setConfirmReset(true)} disabled={resetting}>Reset library</Button>
          ) : (
            <>
              <Button type="button" variant="destructive" onClick={() => void reset()} disabled={resetting}>{resetting ? <Loader2 className="size-4 animate-spin" /> : null}Confirm: delete all documents</Button>
              <Button type="button" variant="ghost" onClick={() => setConfirmReset(false)} disabled={resetting}>Cancel</Button>
            </>
          )}
        </div>
        {dump ? (
          <div className="space-y-3 text-sm">
            <div className="rounded-lg border border-border bg-background/40 p-3 font-mono text-xs text-muted-foreground">
              <div>db: {dump.db_path}</div>
              <div>{dump.documents_count} document{dump.documents_count === 1 ? "" : "s"} · {dump.jobs_count} job{dump.jobs_count === 1 ? "" : "s"}</div>
            </div>
            {[...grouped.entries()].map(([docId, jobList]) => {
              const doc = docId == null ? null : docsById.get(docId) ?? null;
              return (
                <div key={`doc-${docId ?? "none"}`} className="rounded-lg border border-border bg-background/30 p-3">
                  <div className="flex items-center justify-between">
                    <p className="font-mono text-xs text-foreground">{doc ? `doc#${doc.id} · ${doc.sha256_short}…` : "(no document)"}</p>
                    <p className="font-mono text-[11px] text-muted-foreground">{doc?.status ?? "—"} · {doc?.page_count ?? 0} pages</p>
                  </div>
                  {doc ? <p className="mt-1 truncate text-xs text-muted-foreground" title={doc.original_path}>orig: {doc.original_path}</p> : null}
                  {doc?.output_path ? <p className="truncate text-xs text-muted-foreground" title={doc.output_path}>out: {doc.output_path}</p> : null}
                  <ul className="mt-2 space-y-1">
                    {jobList.map((job) => (
                      <li key={job.id} className="flex items-center justify-between rounded border border-border/60 bg-card/40 px-2 py-1 font-mono text-[11px]">
                        <span>job#{job.id} · {job.status} · {job.origin}{job.engine ? ` · ${job.engine}` : ""}</span>
                        <span className="text-muted-foreground">{new Date(job.created_at * 1000).toLocaleTimeString()}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              );
            })}
            {dump.jobs.length === 0 && dump.documents.length === 0 ? <p className="text-muted-foreground">No documents or jobs yet.</p> : null}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">Click "Dump current state" to inspect the DB rows behind the upload queue. Use "Reset library" to wipe everything (documents, jobs, pages, chunks, embeddings) and start clean.</p>
        )}
      </div>
    </SettingsCard>
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

function ProviderCard({ title, status, message, children, cloud = false, enabled, onToggle }: { title: string; status: Status; message: string; children: ReactNode; cloud?: boolean; enabled?: boolean; onToggle?: (value: boolean) => void }) {
  const showToggle = typeof enabled === "boolean" && Boolean(onToggle);
  return (
    <section className={cn("rounded-xl border bg-background/40 p-5 transition-colors", enabled ? "border-primary/40" : "border-border")}>
      <div className="mb-5 flex items-start justify-between gap-4">
        <div>
          <h3 className="font-medium text-foreground">{title}</h3>
          <p className="mt-1 text-xs text-muted-foreground">{cloud ? "Cloud API · key stored securely" : "Local endpoint · offline capable"}</p>
        </div>
        <div className="flex items-center gap-3">
          <StatusBadge status={status} />
          {showToggle && <Toggle checked={enabled!} onChange={onToggle!} label={`Enable ${title}`} />}
        </div>
      </div>
      {showToggle && !enabled ? (
        <p className="rounded-lg border border-dashed border-border bg-background/40 p-3 text-xs text-muted-foreground">{title} is off. Enable it to configure a model and use Chat.</p>
      ) : (
        <div className="space-y-5">{children}</div>
      )}
      {message && <p className="mt-4 flex items-center gap-2 text-xs text-muted-foreground"><ShieldAlert className="size-3.5" />{message}</p>}
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

function UpdatesSettings() {
  const [version, setVersion] = useState("0.1.0");
  const [autoCheck, setAutoCheck] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [result, setResult] = useState<"idle" | "up-to-date" | "available" | "error">("idle");
  const [message, setMessage] = useState("");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const [ver, auto] = await Promise.all([
        getVersion().catch(() => "0.1.0"),
        getSetting("updater.auto_check"),
      ]);
      if (cancelled) return;
      setVersion(ver);
      setAutoCheck(auto !== "0");
      setLoaded(true);
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function persistAutoCheck(next: boolean) {
    setAutoCheck(next);
    await setSetting("updater.auto_check", next ? "1" : "0");
  }

  async function checkNow() {
    setChecking(true);
    setMessage("");
    try {
      const found = await check();
      if (found) {
        setUpdate(found);
        setResult("available");
        setMessage(`Version ${found.version} is available.`);
      } else {
        setUpdate(null);
        setResult("up-to-date");
        setMessage("You're on the latest version.");
      }
    } catch (error) {
      setResult("error");
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setChecking(false);
    }
  }

  async function installNow() {
    if (!update) return;
    setInstalling(true);
    setMessage("Downloading and installing…");
    try {
      await invoke("prepare_for_update");
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      setResult("error");
      setMessage(error instanceof Error ? error.message : String(error));
      setInstalling(false);
    }
  }

  return (
    <SettingsCard title="Updates" eyebrow="Stay current">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-background/45 p-4">
        <div className="flex items-center gap-3">
          <span className="grid size-10 place-items-center rounded-lg border border-border bg-secondary/50 text-foreground">
            <ShieldCheck className="size-5" />
          </span>
          <div>
            <div className="text-sm font-medium text-foreground">PDF-Parser</div>
            <p className="mt-0.5 font-mono text-xs text-muted-foreground">Version {version}</p>
          </div>
        </div>
        <Button type="button" onClick={() => void checkNow()} disabled={checking || installing}>
          {checking ? <Loader2 className="size-4 animate-spin" /> : <RotateCw className="size-4" />}
          Check for updates
        </Button>
      </div>

      {result !== "idle" && (
        <div
          className={cn(
            "flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4 text-sm",
            result === "available" && "border-primary/40 bg-primary/5 text-foreground",
            result === "up-to-date" && "border-emerald-400/30 bg-emerald-400/5 text-foreground",
            result === "error" && "border-destructive/40 bg-destructive/5 text-destructive",
          )}
        >
          <div className="flex items-center gap-2">
            {result === "available" && <DownloadCloud className="size-4 text-primary" />}
            {result === "up-to-date" && <CheckCircle2 className="size-4 text-emerald-400" />}
            {result === "error" && <ShieldAlert className="size-4" />}
            <span className={result === "error" ? undefined : "text-muted-foreground"}>{message}</span>
          </div>
          {result === "available" && (
            <Button type="button" onClick={() => void installNow()} disabled={installing}>
              {installing ? <Loader2 className="size-4 animate-spin" /> : <DownloadCloud className="size-4" />}
              {installing ? "Installing…" : "Install and restart"}
            </Button>
          )}
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-border">
        <SettingRow icon={RefreshCw} title="Automatic update checks" description="Check for new signed releases on launch and every 6 hours while the app is open.">
          <Toggle checked={autoCheck} disabled={!loaded} onChange={(next) => void persistAutoCheck(next)} label="Automatic update checks" />
        </SettingRow>
      </div>

      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5" />
        Updates are downloaded from signed GitHub release artifacts and verified before they install.
      </p>
    </SettingsCard>
  );
}

function isProvider(value: string | null): value is Provider {
  return value === "none" || value === "ollama";
}

