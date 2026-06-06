import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { confirm } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, DownloadCloud, Gauge, HardDrive, Info, Languages, Loader2, PenLine, RotateCcw, ScanText, Sparkles, Trash2, WifiOff, Zap, type LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  installOcrEngine,
  listOcrEngines,
  listenEngineInstallProgress,
  removeOcrEngine,
  setDefaultOcrEngine,
  type EngineInfo,
  type EngineInstallProgressEvent,
} from "@/lib/ipc";

const OCR_ENGINES_QUERY_KEY = ["ocr-engines"] as const;

export type EngineComparison = {
  tagline: string;
  whenToChoose: string;
  speed: string;
  accuracy: string;
  languages: string;
  handwriting: string;
  offline: string;
  download: string;
};

// Detailed, FRONTEND-owned comparison copy keyed by engine id. The backend only
// supplies the short `description`; this complements it without touching the
// EngineInfo contract. Unknown ids fall back to FALLBACK_COMPARISON.
const ENGINE_COMPARISON: Record<string, EngineComparison> = {
  tesseract: {
    tagline: "Bundled default · fast and fully local",
    whenToChoose: "Everyday English/Latin-script PDFs, digital-born documents, and large batches where speed matters most.",
    speed: "Fast — light CPU inference, ideal for big queues.",
    accuracy: "Excellent on clean printed text; degrades on noisy scans.",
    languages: "100+ Latin and European scripts. Basic CJK only.",
    handwriting: "Not suited to handwriting or heavily degraded pages.",
    offline: "100% offline — ships inside the app.",
    download: "Bundled, no extra download.",
  },
  rapidocr: {
    tagline: "Opt-in download · higher accuracy",
    whenToChoose: "Photographed or low-quality scans, dense layouts, and CJK / mixed-script documents where Tesseract falls short.",
    speed: "Slower per page — heavier ONNX models need more compute.",
    accuracy: "Stronger on messy, rotated, or low-contrast scans.",
    languages: "Robust CJK (Chinese, Japanese, Korean) plus Latin via PP-OCRv5.",
    handwriting: "Far better on degraded print and some handwriting.",
    offline: "Fully offline after a one-time model download.",
    download: "One-time PP-OCRv5 model download.",
  },
};

const FALLBACK_COMPARISON: EngineComparison = {
  tagline: "OCR engine",
  whenToChoose: "Pick the engine that best matches your documents, then set it as default.",
  speed: "—",
  accuracy: "—",
  languages: "—",
  handwriting: "—",
  offline: "Runs locally on your machine.",
  download: "See size below.",
};

export function getEngineComparison(id: string): EngineComparison {
  return ENGINE_COMPARISON[id] ?? FALLBACK_COMPARISON;
}

type ProgressState = Record<string, EngineInstallProgressEvent>;

export function useOcrEngines() {
  return useQuery({
    queryKey: OCR_ENGINES_QUERY_KEY,
    queryFn: listOcrEngines,
    refetchOnWindowFocus: false,
  });
}

export function EngineSelector() {
  const queryClient = useQueryClient();
  const { data: engines = [], isLoading, error } = useOcrEngines();
  const [progress, setProgress] = useState<ProgressState>({});
  const [busyEngine, setBusyEngine] = useState<string | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlistenPromise = listenEngineInstallProgress((payload) => {
      if (disposed) {
        return;
      }
      setProgress((current) => ({ ...current, [payload.engine_id]: payload }));
      if (payload.phase === "complete") {
        void queryClient.invalidateQueries({ queryKey: OCR_ENGINES_QUERY_KEY });
      }
    });
    return () => {
      disposed = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [queryClient]);

  async function install(engineId: string) {
    setBusyEngine(engineId);
    setInstallError(null);
    try {
      await installOcrEngine(engineId);
    } catch (error) {
      setInstallError(error instanceof Error ? error.message : String(error));
    } finally {
      setProgress((current) => {
        const next = { ...current };
        delete next[engineId];
        return next;
      });
      await queryClient.invalidateQueries({ queryKey: OCR_ENGINES_QUERY_KEY });
      setBusyEngine(null);
    }
  }

  async function remove(engine: EngineInfo) {
    const confirmed = await confirm(
      `Remove ${engine.name}? This reclaims about ${engine.size_mb} MB and you can reinstall it later.`,
      { title: `Remove ${engine.name}`, kind: "warning" },
    );
    if (!confirmed) {
      return;
    }
    setBusyEngine(engine.id);
    try {
      await removeOcrEngine(engine.id);
      setProgress((current) => {
        const next = { ...current };
        delete next[engine.id];
        return next;
      });
      await queryClient.invalidateQueries({ queryKey: OCR_ENGINES_QUERY_KEY });
    } finally {
      setBusyEngine(null);
    }
  }

  async function setDefault(engineId: string) {
    setBusyEngine(engineId);
    try {
      await setDefaultOcrEngine(engineId);
      await queryClient.invalidateQueries({ queryKey: OCR_ENGINES_QUERY_KEY });
    } finally {
      setBusyEngine(null);
    }
  }

  const totalDownloadMb = useMemo(
    () => engines.filter((engine) => engine.status !== "installed").reduce((sum, engine) => sum + engine.size_mb, 0),
    [engines],
  );

  if (isLoading) {
    return (
      <section className="rounded-2xl border border-border bg-card/70 p-5" aria-busy="true">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" /> Loading OCR engines…
        </div>
      </section>
    );
  }

  if (error) {
    return (
      <section className="rounded-2xl border border-destructive/30 bg-destructive/10 p-5 text-sm text-destructive">
        Unable to load OCR engines: {error instanceof Error ? error.message : String(error)}
      </section>
    );
  }

  return (
    <section className="relative overflow-hidden rounded-2xl border border-border bg-card/75 p-5 shadow-2xl shadow-black/20">
      <div className="pointer-events-none absolute inset-0 opacity-80 [background:radial-gradient(circle_at_12%_0%,oklch(0.88_0.03_120/0.16),transparent_22rem),linear-gradient(135deg,transparent_0_55%,oklch(1_0_0/0.06)_55%_56%,transparent_56%)]" />
      <div className="relative space-y-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="inline-flex items-center gap-2 rounded-full border border-border bg-background/50 px-3 py-1 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
              <Zap className="size-3" /> OCR engines
            </div>
            <h2 className="mt-3 text-2xl font-semibold tracking-[-0.05em] text-foreground">Choose speed or precision per document.</h2>
            <p className="mt-1 max-w-2xl text-sm leading-6 text-muted-foreground">
              Tesseract stays bundled and default. RapidOCR downloads verified PP-OCRv5 ONNX models into LocalAppData only when you opt in.
            </p>
          </div>
          {totalDownloadMb > 0 ? (
            <div className="rounded-xl border border-border bg-background/45 px-3 py-2 text-right font-mono text-xs text-muted-foreground">
              Optional payload
              <div className="text-foreground">{totalDownloadMb} MB</div>
            </div>
          ) : null}
        </div>

        {installError ? (
          <div
            role="alert"
            className="flex items-start justify-between gap-3 rounded-xl border border-destructive/35 bg-destructive/10 p-3 text-xs leading-5 text-destructive"
          >
            <span>Install failed: {installError}</span>
            <button
              type="button"
              onClick={() => setInstallError(null)}
              className="shrink-0 rounded-md border border-destructive/30 px-2 py-0.5 font-mono uppercase tracking-[0.18em] text-destructive/90 hover:bg-destructive/15"
            >
              Dismiss
            </button>
          </div>
        ) : null}

        <div className="grid gap-3 lg:grid-cols-2">
          {engines.map((engine) => (
            <EngineCard
              key={engine.id}
              engine={engine}
              progress={progress[engine.id]}
              busy={busyEngine === engine.id}
              onInstall={() => void install(engine.id)}
              onRemove={() => void remove(engine)}
              onSetDefault={() => void setDefault(engine.id)}
            />
          ))}
        </div>

        <EngineComparisonTable engines={engines} />
      </div>
    </section>
  );
}

type EngineCardProps = {
  engine: EngineInfo;
  progress?: EngineInstallProgressEvent;
  busy: boolean;
  onInstall: () => void;
  onRemove: () => void;
  onSetDefault: () => void;
};

function EngineCard({ engine, progress, busy, onInstall, onRemove, onSetDefault }: EngineCardProps) {
  const installing = engine.status === "installing" || progress?.phase === "downloading" || busy;
  const progressPct = progress && progress.bytes_total > 0 ? Math.round((progress.bytes_done / progress.bytes_total) * 100) : 0;

  return (
    <article
      className={cn(
        "flex min-h-64 flex-col justify-between rounded-xl border bg-background/55 p-4 transition-colors",
        engine.is_default ? "border-foreground/35" : "border-border",
        engine.status === "error" && "border-destructive/35 bg-destructive/5",
      )}
    >
      <div className="space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold tracking-[-0.03em] text-foreground">{engine.name}</h3>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">{engine.description}</p>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              <span className="font-medium text-foreground">When to choose:</span> {getEngineComparison(engine.id).whenToChoose}
            </p>
          </div>
          <StatusPill engine={engine} installing={installing} />
        </div>

        <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div className="rounded-lg border border-border bg-card/50 p-2">
            <div className="font-mono uppercase tracking-[0.18em]">Size</div>
            <div className="mt-1 text-sm text-foreground">{engine.size_mb} MB</div>
          </div>
          <div className="rounded-lg border border-border bg-card/50 p-2">
            <div className="font-mono uppercase tracking-[0.18em]">Estimate</div>
            <div className="mt-1 text-sm text-foreground">{engine.status === "installed" ? "Ready" : "1–3 min"}</div>
          </div>
        </div>

        {installing ? (
          <div className="space-y-2" aria-live="polite">
            <div className="flex justify-between font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
              <span>{progress?.phase ?? "working"}</span>
              <span>{progressPct}%</span>
            </div>
            <div className="h-2 overflow-hidden rounded-full bg-secondary">
              <div className="h-full rounded-full bg-foreground transition-all duration-500" style={{ width: `${progressPct}%` }} />
            </div>
            <p className="truncate text-xs text-muted-foreground">{progress?.current_file ?? "Preparing download"}</p>
          </div>
        ) : null}

        {engine.status === "error" && engine.error ? (
          <p className="rounded-lg border border-destructive/30 bg-destructive/10 p-2 text-xs leading-5 text-destructive">
            {engine.error}
          </p>
        ) : null}
      </div>

      <div className="mt-5 flex flex-wrap items-center gap-2">
        {engine.status === "installed" ? (
          <>
            <label className="inline-flex items-center gap-2 rounded-lg border border-border bg-card/50 px-2.5 py-2 text-sm text-foreground">
              <input type="radio" name="default-ocr-engine" checked={engine.is_default} onChange={onSetDefault} />
              Set as default
            </label>
            {engine.id !== "tesseract" ? (
              <Button type="button" variant="outline" size="sm" onClick={onRemove} disabled={busy}>
                <Trash2 className="size-3.5" /> Remove
              </Button>
            ) : null}
          </>
        ) : engine.status === "available" || engine.status === "error" ? (
          <Button type="button" size="sm" onClick={onInstall} disabled={busy}>
            {engine.status === "error" ? <RotateCcw className="size-3.5" /> : <DownloadCloud className="size-3.5" />}
            {engine.status === "error" ? "Retry" : `Install ${engine.size_mb} MB`}
          </Button>
        ) : (
          <Button type="button" variant="outline" size="sm" disabled>
            <Loader2 className="size-3.5 animate-spin" /> Cancel
          </Button>
        )}
      </div>
    </article>
  );
}

function StatusPill({ engine, installing }: { engine: EngineInfo; installing: boolean }) {
  const label = installing ? "installing" : engine.status;
  const Icon = label === "installed" ? CheckCircle2 : label === "installing" ? Loader2 : HardDrive;
  return (
    <span className="inline-flex shrink-0 items-center gap-1 rounded-full border border-border bg-card/60 px-2 py-1 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
      <Icon className={cn("size-3", label === "installing" && "animate-spin")} />
      {label}
    </span>
  );
}

const COMPARISON_DIMENSIONS: ReadonlyArray<{ key: keyof EngineComparison; label: string; icon: LucideIcon }> = [
  { key: "speed", label: "Speed", icon: Gauge },
  { key: "accuracy", label: "Accuracy", icon: ScanText },
  { key: "languages", label: "Languages & scripts", icon: Languages },
  { key: "handwriting", label: "Handwriting & poor scans", icon: PenLine },
  { key: "offline", label: "Offline", icon: WifiOff },
  { key: "download", label: "Download size", icon: HardDrive },
];

export function EngineComparisonTable({ engines }: { engines: EngineInfo[] }) {
  const known = engines.filter((engine) => engine.id in ENGINE_COMPARISON);
  const list = (known.length > 0 ? known : engines)
    .slice()
    .sort((a, b) => Number(b.is_default) - Number(a.is_default));
  if (list.length === 0) {
    return null;
  }
  return (
    <section className="rounded-xl border border-border bg-background/45 p-4">
      <div className="flex items-center gap-2">
        <Info className="size-4 text-muted-foreground" />
        <h3 className="text-sm font-medium text-foreground">Which engine should I use?</h3>
      </div>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">
        Tesseract is the fast, bundled default. Install RapidOCR when you need higher accuracy on tough or non-Latin scans.
      </p>
      <div className="mt-4 grid gap-3 md:grid-cols-2">
        {list.map((engine) => {
          const comparison = getEngineComparison(engine.id);
          return (
            <article
              key={engine.id}
              className={cn("rounded-lg border bg-card/50 p-3", engine.is_default ? "border-foreground/30" : "border-border")}
            >
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-sm font-semibold text-foreground">{engine.name}</h4>
                {engine.is_default ? (
                  <span className="rounded-full border border-foreground/30 px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                    default
                  </span>
                ) : null}
              </div>
              <p className="mt-0.5 text-xs text-muted-foreground">{comparison.tagline}</p>
              <p className="mt-2 flex items-start gap-1.5 text-xs leading-5 text-foreground">
                <Sparkles className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
                <span>
                  <span className="font-medium">When to choose:</span> {comparison.whenToChoose}
                </span>
              </p>
              <dl className="mt-3 space-y-2">
                {COMPARISON_DIMENSIONS.map((dimension) => {
                  const Icon = dimension.icon;
                  const value =
                    dimension.key === "download" && engine.id !== "tesseract"
                      ? `${comparison.download} (~${engine.size_mb} MB)`
                      : comparison[dimension.key];
                  return (
                    <div key={dimension.key} className="grid grid-cols-[8rem_1fr] gap-2 text-xs">
                      <dt className="flex items-center gap-1.5 text-muted-foreground">
                        <Icon className="size-3.5 shrink-0" />
                        {dimension.label}
                      </dt>
                      <dd className="text-foreground/90">{value}</dd>
                    </div>
                  );
                })}
              </dl>
            </article>
          );
        })}
      </div>
    </section>
  );
}
