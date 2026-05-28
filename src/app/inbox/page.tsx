import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { Activity, CheckCircle2, Clock3, FileText, Inbox as InboxIcon, Loader2, Pause, Play, RotateCcw, Trash2, UploadCloud, XCircle } from "lucide-react";
import { EmptyState } from "@/components/empty-state";
import { useOcrEngines } from "@/components/engine-selector";
import { ErrorBanner } from "@/components/error-banner";
import { QualityUpgradePrompt } from "@/components/quality-upgrade-prompt";
import { Button } from "@/components/ui/button";
import { notifySuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";
import {
  jobsCancel,
  jobsCancelAll,
  jobsClearCompleted,
  jobsList,
  jobsPauseAll,
  jobsResumeAll,
  jobsRetry,
  listenAppEvent,
  processPdf,
  setDefaultOcrEngine,
  type EngineInfo,
  type JobLifecycleEvent,
  type JobProgressBatchEvent,
  type JobProgressUpdate,
  type JobStatus,
  type JobSummary,
  type WatcherErrorEvent,
} from "@/lib/ipc";

type QueueJob = JobSummary & {
  progress_pct: number;
  message?: string | null;
  current_page?: number | null;
  updated_at_ms?: number;
};

type FilterTab = "all" | "running" | "failed" | "done";

type BannerState = {
  id: string;
  severity: "warning" | "error";
  title: string;
  message: string;
  details?: string;
};

const ITEM_HEIGHT = 76;
const LIST_HEIGHT = 460;

export function InboxPage() {
  const navigate = useNavigate();
  const [jobs, setJobs] = useState<QueueJob[]>([]);
  const [filter, setFilter] = useState<FilterTab>("all");
  const [isDragging, setIsDragging] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [banners, setBanners] = useState<BannerState[]>([]);
  const [queueMessage, setQueueMessage] = useState("");
  const { data: engines = [] } = useOcrEngines();
  const defaultEngineId = engines.find((engine) => engine.is_default)?.id ?? "tesseract";
  const [selectedEngineId, setSelectedEngineId] = useState(defaultEngineId);
  const refreshInFlight = useRef(false);

  const refreshJobs = useCallback(async () => {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    try {
      const rows = await jobsList({ limit: 750 });
      setJobs((current) => mergeJobRows(current, rows));
    } finally {
      refreshInFlight.current = false;
    }
  }, []);

  useEffect(() => {
    setSelectedEngineId(defaultEngineId);
  }, [defaultEngineId]);

  useEffect(() => {
    void refreshJobs();
    const id = window.setInterval(() => void refreshJobs(), 2500);
    return () => window.clearInterval(id);
  }, [refreshJobs]);

  useEffect(() => {
    let disposed = false;
    const progress = listenAppEvent("job.progress.batch", (payload: JobProgressBatchEvent) => {
      if (!disposed) setJobs((current) => applyProgressBatch(current, payload.updates));
    });
    const lifecycle = listenAppEvent("job.lifecycle", (payload: JobLifecycleEvent) => {
      if (!disposed) {
        setJobs((current) => patchLifecycle(current, payload));
        if (payload.status === "error") void refreshJobs();
      }
    });
    const watcher = listenAppEvent("watcher.error", (payload: WatcherErrorEvent) => {
      if (!disposed) addBanner({ id: `watcher-${Date.now()}`, severity: "error", title: "Watched folder access lost", message: payload.folder, details: payload.error });
    });
    return () => {
      disposed = true;
      void progress.then((unlisten) => unlisten());
      void lifecycle.then((unlisten) => unlisten());
      void watcher.then((unlisten) => unlisten());
    };
  }, [refreshJobs]);

  function addBanner(banner: BannerState) {
    setBanners((current) => [banner, ...current.filter((item) => item.title !== banner.title)].slice(0, 4));
  }

  const rapidOcrInstalled = engines.some((engine) => engine.id === "rapidocr" && engine.status === "installed");
  const lowConfidenceCount = 0;
  const scanLikeQueueCount = jobs.filter((job) => job.status === "running" && !job.page_count).length;
  const counts = useMemo(() => summarizeJobs(jobs), [jobs]);
  const failedJobs = useMemo(() => jobs.filter((job) => job.status === "error"), [jobs]);
  const filteredJobs = useMemo(() => filterJobs(jobs, filter), [jobs, filter]);
  const currentJob = useMemo(() => jobs.filter((job) => job.status === "running").sort((a, b) => (b.updated_at_ms ?? 0) - (a.updated_at_ms ?? 0))[0], [jobs]);
  const overallProgress = useMemo(() => {
    const active = jobs.filter((job) => job.status === "queued" || job.status === "running" || job.status === "paused");
    if (active.length === 0) return 0;
    return active.reduce((sum, job) => sum + clampProgress(job.progress_pct), 0) / active.length;
  }, [jobs]);

  const visibleStart = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - 4);
  const visibleEnd = Math.min(filteredJobs.length, visibleStart + Math.ceil(LIST_HEIGHT / ITEM_HEIGHT) + 8);
  const visibleJobs = filteredJobs.slice(visibleStart, visibleEnd);

  const processPath = useCallback(async (path: string) => {
    const engineId = selectedEngineId;
    try {
      const queued = await processPdf(path, engineId);
      setJobs((current) => mergeJobRows(current, [queued]));
      setQueueMessage("Queued for OCR.");
      notifySuccess("Queued for OCR.");
    } catch (error) {
      addBanner({ id: `manual-${Date.now()}`, severity: "error", title: "PDF could not be queued", message: basename(path), details: error instanceof Error ? error.message : String(error) });
    }
  }, [selectedEngineId]);

  useEffect(() => {
    const unlistenPromise = getCurrentWebviewWindow().onDragDropEvent((event) => {
      const payload = event.payload as { type: string; paths?: string[] };
      if (payload.type === "enter" || payload.type === "over") setIsDragging(true);
      if (payload.type === "leave") setIsDragging(false);
      if (payload.type === "drop") {
        setIsDragging(false);
        for (const path of payload.paths ?? []) {
          if (path.toLowerCase().endsWith(".pdf")) void processPath(path);
        }
      }
    });
    return () => void unlistenPromise.then((unlisten) => unlisten());
  }, [processPath]);

  async function choosePdf() {
    const selected = await open({ directory: false, multiple: true, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    const paths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    await Promise.allSettled(paths.map((path) => processPath(path)));
  }

  async function pauseAll() {
    await jobsPauseAll();
    await refreshJobs();
  }

  async function resumeAll() {
    await jobsResumeAll();
    await refreshJobs();
  }

  async function retryFailed() {
    await Promise.allSettled(failedJobs.map((job) => jobsRetry(job.id)));
    await refreshJobs();
  }

  async function cancelAll() {
    await jobsCancelAll();
    await refreshJobs();
  }

  async function clearCompleted() {
    await jobsClearCompleted();
    await refreshJobs();
  }

  const persistentIssue = issueBannerForJobs(jobs);

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-5">
      <section className={cn("relative overflow-hidden rounded-2xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20", isDragging && "border-foreground/50 bg-secondary/60")}>
        <div className="pointer-events-none absolute inset-0 opacity-80 [background:radial-gradient(circle_at_12%_18%,oklch(0.86_0.02_260/0.16),transparent_18rem),linear-gradient(120deg,transparent_0_44%,oklch(1_0_0/0.06)_44%_45%,transparent_45%)]" />
        <div className="relative grid gap-6 lg:grid-cols-[1.3fr_0.7fr] lg:items-center">
          <div className="space-y-4">
            <div className="inline-flex items-center gap-2 rounded-full border border-border bg-background/60 px-3 py-1 font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">
              <span className="size-1.5 rounded-full bg-foreground" />
              OCR intake bay
            </div>
            <div>
              <h1 className="max-w-3xl text-4xl font-semibold tracking-[-0.06em] text-foreground">Queue a shelf of PDFs without babysitting every file.</h1>
              <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">Drop batches here, pick files, or let Settings watch a folder. Progress arrives in throttled batches so 100+ documents stay smooth.</p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button type="button" size="lg" onClick={() => void choosePdf()}>
                <UploadCloud className="size-4" />
                Choose files
              </Button>
              <EnginePicker engines={engines} selectedEngineId={selectedEngineId} onChange={setSelectedEngineId} />
              {queueMessage ? <span className="rounded-lg border border-border bg-background/50 px-3 py-2 text-xs text-muted-foreground">{queueMessage}</span> : null}
            </div>
          </div>
          <div className={cn("grid min-h-48 place-items-center rounded-xl border border-dashed border-border bg-background/35 p-6 text-center", isDragging && "border-foreground/60 bg-background/70")}>
            <div className="space-y-3">
              <div className="mx-auto grid size-12 place-items-center rounded-xl border border-border bg-secondary/70"><FileText className="size-5" /></div>
              <p className="font-medium text-foreground">Drop multi-file PDF batches here</p>
              <p className="text-xs text-muted-foreground">Watched folders auto-ingest in the background.</p>
            </div>
          </div>
        </div>
      </section>

      <QualityUpgradePrompt lowConfidenceCount={lowConfidenceCount} scanLikeQueueCount={scanLikeQueueCount} rapidOcrInstalled={rapidOcrInstalled} onUseOnce={() => rapidOcrInstalled && setSelectedEngineId("rapidocr")} onSetDefault={() => { if (rapidOcrInstalled) { setSelectedEngineId("rapidocr"); void setDefaultOcrEngine("rapidocr"); } }} />

      {persistentIssue ? <InboxIssueBanner issue={persistentIssue} retryFailed={retryFailed} openSettings={() => void navigate({ to: "/settings" })} /> : null}
      {banners.map((banner) => (
        <ErrorBanner key={banner.id} severity={banner.severity} title={banner.title} message={banner.message} details={banner.details} dismissable onDismiss={() => setBanners((current) => current.filter((item) => item.id !== banner.id))} actions={[{ label: "Open settings", onClick: () => void navigate({ to: "/settings" }) }]} />
      ))}

      <section className="grid gap-4 lg:grid-cols-[1.05fr_0.95fr]">
        <div className="rounded-2xl border border-border bg-card/70 p-5" aria-live="polite">
          <div className="flex items-end justify-between gap-4">
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">Queue aggregate</p>
              <div className="mt-2 flex items-baseline gap-2"><span className="text-5xl font-semibold tracking-[-0.08em]">{counts.active}</span><span className="text-sm text-muted-foreground">active</span></div>
            </div>
            <Activity className="size-8 text-muted-foreground" />
          </div>
          <div className="mt-5 grid grid-cols-5 gap-2 text-center text-xs">
            <Metric label="queued" value={counts.queued} />
            <Metric label="running" value={counts.running} />
            <Metric label="paused" value={counts.paused} />
            <Metric label="failed" value={counts.failed} tone="bad" />
            <Metric label="done" value={counts.done} tone="good" />
          </div>
          <div className="mt-5 h-2 overflow-hidden rounded-full bg-secondary"><div className="h-full rounded-full bg-foreground transition-all duration-500" style={{ width: `${overallProgress}%` }} /></div>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button type="button" variant="outline" onClick={() => void pauseAll()}><Pause className="size-4" />Pause all</Button>
            <Button type="button" variant="outline" onClick={() => void resumeAll()}><Play className="size-4" />Resume all</Button>
            <Button type="button" variant="outline" onClick={() => void retryFailed()} disabled={failedJobs.length === 0}><RotateCcw className="size-4" />Retry failed ({failedJobs.length})</Button>
            <Button type="button" variant="destructive" onClick={() => void cancelAll()} disabled={counts.active === 0}>Cancel all</Button>
            <Button type="button" variant="ghost" onClick={() => void clearCompleted()}><Trash2 className="size-4" />Clear completed</Button>
          </div>
        </div>

        <div className="rounded-2xl border border-border bg-card/70 p-5">
          <p className="font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">Current document</p>
          {currentJob ? (
            <div className="mt-4 space-y-3">
              <div className="flex items-start justify-between gap-4"><div className="min-w-0"><h2 className="truncate text-xl font-semibold tracking-[-0.04em]">{currentJob.filename}</h2><p className="mt-1 text-sm text-muted-foreground">{formatStage(currentJob.stage)} · {currentJob.message ?? "Processing"}</p></div><StatusBadge status={currentJob.status} /></div>
              <div className="h-2 overflow-hidden rounded-full bg-secondary"><div className="h-full rounded-full bg-foreground transition-all duration-500" style={{ width: `${clampProgress(currentJob.progress_pct)}%` }} /></div>
              <div className="grid grid-cols-3 gap-2 text-xs text-muted-foreground"><span>Page {currentJob.current_page ?? "—"} / {currentJob.page_count || "—"}</span><span>{Math.round(clampProgress(currentJob.progress_pct))}%</span><span className="flex items-center gap-1"><Clock3 className="size-3" />ETA learning</span></div>
            </div>
          ) : (
            <div className="mt-4 rounded-xl border border-dashed border-border bg-background/35 p-6 text-sm text-muted-foreground">Nothing running right now.</div>
          )}
        </div>
      </section>

      <section className="overflow-hidden rounded-2xl border border-border bg-card/70">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div><h2 className="text-sm font-medium text-foreground">Job list</h2><p className="text-xs text-muted-foreground">Virtualized queue view for large batches.</p></div>
          <div className="flex rounded-lg border border-border bg-background/45 p-1">
            {(["all", "running", "failed", "done"] as const).map((tab) => <button key={tab} type="button" onClick={() => setFilter(tab)} className={cn("rounded-md px-3 py-1.5 text-xs capitalize text-muted-foreground", filter === tab && "bg-secondary text-foreground")}>{tab}</button>)}
          </div>
        </div>
        {filteredJobs.length === 0 ? (
          jobs.length === 0 ? (
            <EmptyState icon={InboxIcon} title="Nothing to process" description="Drag a PDF here, pick a file, or watch a folder." actionLabel="Choose file" onAction={() => void choosePdf()} />
          ) : (
            <div className="px-4 py-12 text-center text-sm text-muted-foreground">No jobs match this filter.</div>
          )
        ) : (
          <div className="overflow-auto" style={{ height: LIST_HEIGHT }} onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}>
            <div style={{ height: filteredJobs.length * ITEM_HEIGHT, position: "relative" }}>
              <div style={{ transform: `translateY(${visibleStart * ITEM_HEIGHT}px)` }}>
                {visibleJobs.map((job) => <JobRow key={job.id} job={job} onCancel={() => void jobsCancel(job.id).then(refreshJobs)} onRetry={() => void jobsRetry(job.id).then(refreshJobs)} />)}
              </div>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

function InboxIssueBanner({ issue, retryFailed, openSettings }: { issue: BannerState; retryFailed: () => Promise<void>; openSettings: () => void }) {
  const actions = issue.title.includes("rate")
    ? [{ label: "Retry", onClick: () => void retryFailed() }]
    : issue.title.includes("Ollama")
      ? [{ label: "Open Ollama", onClick: () => window.open("http://localhost:11434", "_blank") }, { label: "Open settings", onClick: openSettings }]
      : [{ label: "Open settings", onClick: openSettings }];
  return <ErrorBanner severity={issue.severity} title={issue.title} message={issue.message} details={issue.details} actions={actions} />;
}

function JobRow({ job, onCancel, onRetry }: { job: QueueJob; onCancel: () => void; onRetry: () => void }) {
  return (
    <div tabIndex={0} className="grid min-h-[76px] grid-cols-[minmax(0,1.4fr)_0.6fr_0.7fr_0.8fr_0.6fr_1fr_auto] items-center gap-3 border-b border-border px-4 py-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
      <div className="min-w-0"><p className="truncate font-medium text-foreground">{job.filename}</p><p className="text-xs text-muted-foreground">{job.source}</p></div>
      <StatusBadge status={job.status} />
      <span className="truncate text-xs text-muted-foreground">{formatStage(job.stage)}</span>
      <div className="min-w-24"><div className="h-1.5 overflow-hidden rounded-full bg-secondary"><div className="h-full rounded-full bg-foreground" style={{ width: `${clampProgress(job.progress_pct)}%` }} /></div><p className="mt-1 font-mono text-[10px] text-muted-foreground">{Math.round(clampProgress(job.progress_pct))}%</p></div>
      <span className="text-xs text-muted-foreground">{job.page_count || "—"} pg</span>
      <div className="min-w-0 text-xs text-destructive">{job.error_message ? <details><summary className="cursor-pointer truncate">{job.error_message}</summary><pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap text-[10px]">{job.error_message}</pre></details> : <span className="text-muted-foreground">{job.message ?? "—"}</span>}</div>
      <div className="flex gap-1">{job.status === "running" || job.status === "queued" || job.status === "paused" ? <Button type="button" size="sm" variant="ghost" onClick={onCancel}>Cancel</Button> : null}{job.status === "error" || job.status === "cancelled" ? <Button type="button" size="sm" variant="outline" onClick={onRetry}>Retry</Button> : null}</div>
    </div>
  );
}

function Metric({ label, value, tone }: { label: string; value: number; tone?: "good" | "bad" }) {
  return <div className="rounded-lg border border-border bg-background/45 px-2 py-3"><div className={cn("text-lg font-semibold", tone === "good" && "text-emerald-300", tone === "bad" && "text-destructive")}>{value}</div><div className="font-mono uppercase tracking-[0.16em] text-muted-foreground">{label}</div></div>;
}

function StatusBadge({ status }: { status: JobStatus }) {
  const Icon = status === "done" ? CheckCircle2 : status === "error" || status === "cancelled" ? XCircle : status === "running" ? Loader2 : Clock3;
  return <span className={cn("inline-flex w-fit items-center gap-1 rounded-full border px-2 py-1 font-mono text-[10px] uppercase tracking-[0.16em]", status === "done" && "border-emerald-400/30 text-emerald-300", (status === "error" || status === "cancelled") && "border-destructive/40 text-destructive", status === "running" && "border-blue-300/30 text-blue-200", (status === "queued" || status === "paused") && "border-border text-muted-foreground")}><Icon className={cn("size-3", status === "running" && "animate-spin")} />{status}</span>;
}

function EnginePicker({ engines, selectedEngineId, onChange }: { engines: EngineInfo[]; selectedEngineId: string; onChange: (engineId: string) => void }) {
  const rapidOcrInstalled = engines.some((engine) => engine.id === "rapidocr" && engine.status === "installed");
  return <div className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-background/50 px-2 py-1.5"><select value={selectedEngineId} onChange={(event) => onChange(event.target.value)} className="rounded-md border border-border bg-card px-2 py-1.5 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"><option value="tesseract">Process with Tesseract</option><option value="rapidocr" disabled={!rapidOcrInstalled}>{rapidOcrInstalled ? "Process with RapidOCR" : "RapidOCR — install first"}</option></select>{!rapidOcrInstalled ? <Link to="/settings" className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline">Settings → OCR</Link> : null}</div>;
}

function mergeJobRows(current: QueueJob[], rows: JobSummary[]) {
  const map = new Map(current.map((job) => [job.id, job]));
  for (const row of rows) map.set(row.id, { ...map.get(row.id), ...row, progress_pct: Math.max(map.get(row.id)?.progress_pct ?? 0, row.progress_pct ?? 0) });
  return [...map.values()].sort((a, b) => b.created_at - a.created_at || b.id - a.id);
}

function applyProgressBatch(current: QueueJob[], updates: JobProgressUpdate[]) {
  const map = new Map(current.map((job) => [job.id, job]));
  for (const update of updates) {
    const existing = map.get(update.job_id);
    map.set(update.job_id, { ...(existing ?? blankJob(update)), id: update.job_id, document_id: update.document_id, filename: update.filename, status: update.stage === "done" || update.stage === "partial_success" || update.stage === "naming" ? "done" : "running", stage: update.stage, progress_pct: clampProgress(update.progress_pct), message: update.message, current_page: update.page_number, page_count: update.page_count, updated_at_ms: Date.now() });
  }
  return [...map.values()].sort((a, b) => b.created_at - a.created_at || b.id - a.id);
}

function patchLifecycle(current: QueueJob[], payload: JobLifecycleEvent) {
  return current.map((job) => job.id === payload.job_id ? { ...job, status: payload.status, message: payload.message ?? job.message, progress_pct: payload.status === "done" || payload.status === "error" || payload.status === "cancelled" ? 100 : job.progress_pct, updated_at_ms: Date.now() } : job);
}

function blankJob(update: JobProgressUpdate): QueueJob {
  return { id: update.job_id, document_id: update.document_id, filename: update.filename, original_path: null, source: "manual", kind: "ingest", status: "running", stage: update.stage, progress_pct: update.progress_pct, created_at: Math.floor(Date.now() / 1000), page_count: update.page_count };
}

function summarizeJobs(jobs: QueueJob[]) {
  const queued = jobs.filter((job) => job.status === "queued").length;
  const running = jobs.filter((job) => job.status === "running").length;
  const paused = jobs.filter((job) => job.status === "paused").length;
  const failed = jobs.filter((job) => job.status === "error").length;
  const done = jobs.filter((job) => job.status === "done").length;
  return { queued, running, paused, failed, done, active: queued + running + paused };
}

function filterJobs(jobs: QueueJob[], filter: FilterTab) {
  if (filter === "running") return jobs.filter((job) => job.status === "running" || job.status === "queued" || job.status === "paused");
  if (filter === "failed") return jobs.filter((job) => job.status === "error");
  if (filter === "done") return jobs.filter((job) => job.status === "done" || job.status === "cancelled");
  return jobs;
}

function issueBannerForJobs(jobs: QueueJob[]): BannerState | null {
  const errorText = jobs.map((job) => job.error_message ?? "").join("\n").toLowerCase();
  if (errorText.includes("rate limit") || errorText.includes("429")) return { id: "rate-limit", severity: "warning", title: "AI provider rate-limited", message: "OCR finished, but AI naming needs a retry after the provider cools down.", details: errorText };
  if (errorText.includes("ollama") && (errorText.includes("not reachable") || errorText.includes("connection"))) return { id: "ollama", severity: "warning", title: "Ollama is not running", message: "Start Ollama or switch the AI provider in Settings.", details: errorText };
  if (errorText.includes("rapidocr") && errorText.includes("missing")) return { id: "rapidocr", severity: "error", title: "RapidOCR models are missing", message: "Install or repair RapidOCR models in Settings before retrying.", details: errorText };
  return null;
}

function basename(path: string) { return path.split(/[\\/]/).filter(Boolean).pop() ?? path; }
function formatStage(stage: string) { return stage.replace(/_/g, " "); }
function clampProgress(progress: number | null | undefined) { return Math.max(0, Math.min(100, progress ?? 0)); }