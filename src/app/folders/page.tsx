import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import { Activity, ArrowRight, CheckCircle2, Clock3, FolderPlus, Info, Loader2, RefreshCw, Save, Timer, Trash2, UploadCloud, XCircle } from "lucide-react";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getSetting, setSetting } from "@/lib/db";
import { notifySuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";
import {
  jobsList,
  listenAppEvent,
  watcherAddFolder,
  watcherListFolders,
  watcherRemoveFolder,
  watcherScanNow,
  watcherSetEnabled,
  type FolderConfig,
  type JobLifecycleEvent,
  type JobProgressBatchEvent,
  type JobStatus,
  type JobSummary,
} from "@/lib/ipc";

// Shared with the Rust watcher, which reads this same key to schedule sweeps.
const RESCAN_INTERVAL_KEY = "watcher.rescan_interval_secs";
const DEFAULT_RESCAN_SECS = 300;
const MIN_RESCAN_SECS = 30;
const RESCAN_PRESETS: ReadonlyArray<{ label: string; value: number }> = [
  { label: "1 min", value: 60 },
  { label: "5 min", value: 300 },
  { label: "15 min", value: 900 },
  { label: "30 min", value: 1800 },
  { label: "1 hour", value: 3600 },
];

export function FoldersPage() {
  const [folders, setFolders] = useState<FolderConfig[]>([]);
  const [foldersMessage, setFoldersMessage] = useState("");

  const refreshFolders = useCallback(async () => {
    try {
      setFolders(await watcherListFolders());
    } catch (error) {
      setFoldersMessage(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    void refreshFolders();
    // The watcher updates file counts in the background, so refresh on focus.
    const onFocus = () => void refreshFolders();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refreshFolders]);

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

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-5">
      <FoldersHero onAdd={() => void chooseWatchedFolder()} />

      <WatchedActivityPanel />

      <RescanIntervalControl />

      <WatchedFoldersPanel
        folders={folders}
        message={foldersMessage}
        onAdd={() => void chooseWatchedFolder()}
        onRefresh={() => void refreshFolders()}
        onToggle={(folder, enabled) => void toggleWatchedFolder(folder, enabled)}
        onRecursiveChange={(folder, recursive) => void setFolderRecursive(folder, recursive)}
        onScan={(folder) => void scanWatchedFolder(folder)}
        onRemove={(folder) => void removeWatchedFolder(folder)}
      />
    </div>
  );
}

function FoldersHero({ onAdd }: { onAdd: () => void }) {
  return (
    <section className="relative overflow-hidden rounded-2xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20">
      <div className="pointer-events-none absolute inset-0 opacity-80 [background:radial-gradient(circle_at_12%_18%,oklch(0.86_0.05_150/0.18),transparent_18rem),linear-gradient(120deg,transparent_0_44%,oklch(1_0_0/0.06)_44%_45%,transparent_45%)]" />
      <div className="relative grid gap-6 lg:grid-cols-[1.3fr_0.7fr] lg:items-center">
        <div className="space-y-4">
          <div className="inline-flex items-center gap-2 rounded-full border border-border bg-background/60 px-3 py-1 font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">
            <span className="size-1.5 rounded-full bg-emerald-400" />
            Automatic intake
          </div>
          <div>
            <h1 className="max-w-3xl text-4xl font-semibold tracking-[-0.06em] text-foreground">Drop PDFs into a folder. They process themselves.</h1>
            <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
              Watched folders are the primary way to use PDF-Parser. Point it at a folder and every new PDF is detected, OCR&apos;d, named, and added to your Library automatically — no clicks per file. Got a one-off document? Use <Link to="/upload" className="font-medium text-foreground underline-offset-4 hover:underline">manual upload</Link> instead.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" size="lg" onClick={onAdd}>
              <FolderPlus className="size-4" />
              Watch a folder
            </Button>
            <Link
              to="/upload"
              className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background/50 px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              <UploadCloud className="size-4" /> Upload manually
            </Link>
          </div>
          <p className="flex items-center gap-1.5 text-xs leading-5 text-muted-foreground">
            <Info className="size-3.5 shrink-0" />
            Folders are watched only while PDF-Parser is open. Background watching (when the app is closed) is coming soon.
          </p>
        </div>
        <ol className="grid gap-3 rounded-xl border border-dashed border-border bg-background/35 p-5">
          <HeroStep n={1} title="Watch a folder" body="Pick any folder — a scanner&apos;s output, a synced drive, or a downloads folder." />
          <HeroStep n={2} title="Drop in PDFs" body="New files are debounced and checked for stable writes before queuing." />
          <HeroStep n={3} title="Find them in Library" body="OCR and AI naming run automatically in the background." />
        </ol>
      </div>
    </section>
  );
}

function HeroStep({ n, title, body }: { n: number; title: string; body: string }) {
  return (
    <li className="flex items-start gap-3">
      <span className="grid size-6 shrink-0 place-items-center rounded-full border border-border bg-background/60 font-mono text-xs text-foreground">{n}</span>
      <div className="min-w-0">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{body}</p>
      </div>
    </li>
  );
}

function RescanIntervalControl() {
  const [seconds, setSeconds] = useState(DEFAULT_RESCAN_SECS);
  const [inputValue, setInputValue] = useState(String(DEFAULT_RESCAN_SECS));
  const [loaded, setLoaded] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const raw = await getSetting(RESCAN_INTERVAL_KEY).catch(() => null);
      if (cancelled) return;
      const parsed = raw == null ? DEFAULT_RESCAN_SECS : Number.parseInt(raw, 10);
      const value = Number.isFinite(parsed) && parsed >= MIN_RESCAN_SECS ? parsed : DEFAULT_RESCAN_SECS;
      setSeconds(value);
      setInputValue(String(value));
      setLoaded(true);
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function persist(value: number) {
    const clamped = Math.max(MIN_RESCAN_SECS, Math.round(value));
    setSeconds(clamped);
    setInputValue(String(clamped));
    setError("");
    await setSetting(RESCAN_INTERVAL_KEY, String(clamped));
    setMessage(`Saved — folders rescanned every ${formatInterval(clamped)}.`);
    notifySuccess("Rescan interval saved.");
  }

  function applyCustom() {
    const parsed = Number.parseInt(inputValue, 10);
    if (!Number.isFinite(parsed)) {
      setError("Enter a whole number of seconds.");
      return;
    }
    if (parsed < MIN_RESCAN_SECS) {
      setError(`Minimum is ${MIN_RESCAN_SECS} seconds.`);
      return;
    }
    void persist(parsed);
  }

  const isCustom = loaded && !RESCAN_PRESETS.some((preset) => preset.value === seconds);

  return (
    <section className="rounded-2xl border border-border bg-card/70 p-5">
      <div className="flex items-start gap-3">
        <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-secondary/50 text-muted-foreground">
          <Timer className="size-4" />
        </span>
        <div className="min-w-0 flex-1 space-y-4">
          <div>
            <h2 className="text-sm font-medium text-foreground">Rescan interval</h2>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              How often enabled folders are swept for files the live watcher might miss (after sleep, on network drives, or during bulk copies). Currently every <span className="font-medium text-foreground">{formatInterval(seconds)}</span>.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            {RESCAN_PRESETS.map((preset) => (
              <button
                key={preset.value}
                type="button"
                disabled={!loaded}
                onClick={() => void persist(preset.value)}
                aria-pressed={seconds === preset.value}
                className={cn(
                  "rounded-lg border px-3 py-1.5 text-sm transition-colors disabled:opacity-50",
                  seconds === preset.value
                    ? "border-foreground/40 bg-secondary text-foreground"
                    : "border-border bg-background/45 text-muted-foreground hover:text-foreground",
                )}
              >
                {preset.label}
              </button>
            ))}
          </div>

          <div className="flex flex-wrap items-end gap-3">
            <label className="space-y-1.5">
              <span className="block text-xs font-medium text-muted-foreground">Custom (seconds, min {MIN_RESCAN_SECS})</span>
              <div className="flex gap-2">
                <Input
                  type="number"
                  min={MIN_RESCAN_SECS}
                  step={10}
                  value={inputValue}
                  disabled={!loaded}
                  onChange={(event) => setInputValue(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      applyCustom();
                    }
                  }}
                  className="w-32"
                  aria-label="Custom rescan interval in seconds"
                />
                <Button type="button" variant="outline" onClick={applyCustom} disabled={!loaded}>
                  <Save className="size-4" /> Save
                </Button>
              </div>
            </label>
            {isCustom ? <span className="pb-2 text-xs text-muted-foreground">Using a custom interval.</span> : null}
          </div>

          {error ? (
            <p className="text-xs text-destructive">{error}</p>
          ) : message ? (
            <p className="text-xs text-muted-foreground">{message}</p>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function WatchedFoldersPanel({
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
}): ReactNode {
  return (
    <section className="space-y-5 rounded-2xl border border-border bg-card/70 p-5">
      <div>
        <p className="font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">Automatic intake</p>
        <h2 className="mt-1 text-xl font-semibold tracking-[-0.04em]">Watched folders</h2>
      </div>

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
    </section>
  );
}

function WatchedActivityPanel() {
  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [loaded, setLoaded] = useState(false);
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      const rows = await jobsList({ limit: 100 });
      setJobs(
        rows
          .filter((row) => row.source === "watch")
          .sort((a, b) => b.created_at - a.created_at || b.id - a.id),
      );
      setLoaded(true);
    } finally {
      inFlight.current = false;
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 2500);
    return () => window.clearInterval(id);
  }, [refresh]);

  // Patch live progress without a full refetch, and refetch on lifecycle changes
  // (queued/started/done/error) so status stays authoritative.
  useEffect(() => {
    let disposed = false;
    const progress = listenAppEvent("job:progress:batch", (payload: JobProgressBatchEvent) => {
      if (disposed) return;
      setJobs((current) => {
        if (current.length === 0) return current;
        const byId = new Map(current.map((job) => [job.id, job]));
        let changed = false;
        for (const update of payload.updates) {
          const existing = byId.get(update.job_id);
          if (existing) {
            byId.set(update.job_id, {
              ...existing,
              progress_pct: update.progress_pct,
              stage: update.stage,
              status: update.stage === "done" ? "done" : existing.status === "queued" ? "running" : existing.status,
              page_count: update.page_count || existing.page_count,
            });
            changed = true;
          }
        }
        return changed ? [...byId.values()] : current;
      });
    });
    const lifecycle = listenAppEvent("job:lifecycle", (_payload: JobLifecycleEvent) => {
      if (!disposed) void refresh();
    });
    return () => {
      disposed = true;
      void progress.then((unlisten) => unlisten());
      void lifecycle.then((unlisten) => unlisten());
    };
  }, [refresh]);

  const activeCount = jobs.filter((job) => job.status === "queued" || job.status === "running" || job.status === "paused").length;
  const recent = jobs.slice(0, 6);

  return (
    <section className="space-y-4 rounded-2xl border border-border bg-card/70 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-secondary/50 text-muted-foreground">
            <Activity className="size-4" />
          </span>
          <div>
            <h2 className="text-sm font-medium text-foreground">Processing activity</h2>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              {activeCount > 0
                ? `${activeCount} file${activeCount === 1 ? "" : "s"} from watched folders processing now.`
                : "Files picked up from watched folders appear here and in the full queue as they process."}
            </p>
          </div>
        </div>
        <Link
          to="/upload"
          className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background/50 px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          Full queue <ArrowRight className="size-3.5" />
        </Link>
      </div>

      {recent.length === 0 ? (
        <p className="rounded-lg border border-dashed border-border bg-background/35 px-4 py-6 text-center text-xs text-muted-foreground">
          {loaded ? "No recent watched-folder activity yet. Drop a PDF into a watched folder, or use Scan now below." : "Loading…"}
        </p>
      ) : (
        <ul className="space-y-2">
          {recent.map((job) => (
            <li key={job.id} className="flex items-center gap-3 rounded-lg border border-border bg-background/40 px-3 py-2">
              <ActivityStatusIcon status={job.status} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm text-foreground">{job.filename}</p>
                <p className="truncate text-xs text-muted-foreground" title={job.original_path ?? undefined}>
                  {job.original_path ? folderName(job.original_path) : "watched folder"} · {job.stage}
                </p>
              </div>
              <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
                {job.status === "running" || job.status === "queued" ? `${Math.round(clampPct(job.progress_pct))}%` : job.status}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function ActivityStatusIcon({ status }: { status: JobStatus }) {
  if (status === "done") return <CheckCircle2 className="size-4 shrink-0 text-emerald-300" />;
  if (status === "error" || status === "cancelled") return <XCircle className="size-4 shrink-0 text-destructive" />;
  if (status === "running") return <Loader2 className="size-4 shrink-0 animate-spin text-blue-200" />;
  return <Clock3 className="size-4 shrink-0 text-muted-foreground" />;
}

function folderName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : path;
}

function clampPct(value: number) {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
}

function formatInterval(totalSeconds: number) {
  if (totalSeconds % 3600 === 0) {
    const hours = totalSeconds / 3600;
    return `${hours} hour${hours === 1 ? "" : "s"}`;
  }
  if (totalSeconds % 60 === 0) {
    const minutes = totalSeconds / 60;
    return `${minutes} min`;
  }
  return `${totalSeconds} sec`;
}
