import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, DownloadCloud, HardDrive, Loader2, RotateCcw, Trash2, Zap } from "lucide-react";
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
    const confirmed = window.confirm(
      `Remove ${engine.name}? This reclaims about ${engine.size_mb} MB and you can reinstall it later.`,
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
