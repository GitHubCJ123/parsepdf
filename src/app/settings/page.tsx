import { type ReactNode, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Eye, EyeOff, FolderOpen, Loader2, PlugZap, ShieldAlert, SlidersHorizontal } from "lucide-react";
import { EngineSelector } from "@/components/engine-selector";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { aiHealthCheck, aiListModels, listOcrEngines, secretsDelete, secretsGet, secretsSet, setDefaultOcrEngine, type EngineInfo } from "@/lib/ipc";
import { getSetting, setSetting } from "@/lib/db";
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
    }
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
      ["ai", "AI Providers"],
      ["folders", "Folders"],
      ["library", "Library"],
      ["appearance", "Appearance"],
      ["updates", "Updates"],
      ["about", "About"],
    ] as const,
    [],
  );

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
                <span className="text-sm font-medium">OCR Engine</span>
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
                <p className="text-xs text-muted-foreground">Phase 6 can add RapidOCR here without changing the layout.</p>
              </label>
              <label className="space-y-2">
                <span className="text-sm font-medium">Output folder</span>
                <div className="flex gap-2">
                  <Input value={outputDir} onChange={(event) => setOutputDir(event.target.value)} onBlur={() => void setSetting("output_dir", outputDir)} />
                  <Button type="button" variant="outline" onClick={() => void chooseOutputDir()}>
                    <FolderOpen className="size-4" />
                  </Button>
                </div>
              </label>
            </div>
            <EngineSelector />
            {settingsMessage && <p className="text-xs text-muted-foreground">{settingsMessage}</p>}
          </SettingsCard>
        )}

        {activeSection === "ai" && (
          <SettingsCard title="AI Providers" eyebrow="Review before rename">
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
                    <Button type="button" variant="outline" onClick={() => setShowOpenRouterKey((shown) => !shown)}>
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

        {activeSection === "folders" && <Stub title="Watched folders" text="Folder watcher wiring lands in Phase 4. This list stays empty in Phase 2." />}
        {activeSection === "library" && <Stub title="Library defaults" text="Phase 2 adds browsing, preview, review, delete, and copy-path actions in the Library panel." />}
        {activeSection === "appearance" && <Stub title="Appearance" text="Dark refined mode is locked for now. Theme options are reserved for polish." />}
        {activeSection === "updates" && <Stub title="Updates" text="The Phase 7 updater checks signed GitHub release artifacts and prepares installs on quit." />}
        {activeSection === "about" && <Stub title="About PDF-Parser" text="Local-first OCR, optional AI naming, SQLite search, and a Windows-native Tauri shell." />}
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
    <SettingsCard title={title} eyebrow="Phase 2 stub">
      <div className="rounded-lg border border-dashed border-border bg-background/35 p-6 text-sm text-muted-foreground">{text}</div>
    </SettingsCard>
  );
}

function isProvider(value: string | null): value is Provider {
  return value === "none" || value === "openrouter" || value === "ollama";
}

