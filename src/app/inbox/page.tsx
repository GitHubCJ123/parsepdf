import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FileText, Loader2, UploadCloud, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { listenAppEvent, processPdf, type JobProgressEvent } from "@/lib/ipc";

type JobRow = {
  id: string;
  jobId?: number;
  documentId?: number;
  filename: string;
  stage: string;
  message: string;
  progress: number;
  status: "running" | "done" | "error";
  error?: string;
  outputPath?: string | null;
  pageCount?: number;
};

export function InboxPage() {
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [isDragging, setIsDragging] = useState(false);

  const runningCount = useMemo(
    () => jobs.filter((job) => job.status === "running").length,
    [jobs],
  );

  const handleProgress = useCallback((payload: JobProgressEvent) => {
    setJobs((current) => {
      const matchIndex = current.findIndex(
        (job) =>
          job.jobId === payload.job_id ||
          (!job.jobId &&
            job.status === "running" &&
            job.filename.toLowerCase() === payload.filename.toLowerCase()),
      );
      const nextJob: JobRow = {
        ...(matchIndex >= 0
          ? current[matchIndex]
          : {
              id: `job-${payload.job_id}`,
              filename: payload.filename,
              status: "running" as const,
              stage: payload.stage,
              message: payload.message,
              progress: 0,
            }),
        jobId: payload.job_id,
        documentId: payload.document_id,
        filename: payload.filename,
        stage: payload.stage,
        message: payload.message,
        progress: Math.max(0, Math.min(100, payload.progress_pct)),
        pageCount: payload.page_count,
        status: payload.stage === "done" || payload.stage === "partial_success" ? "done" : "running",
      };

      if (matchIndex >= 0) {
        const updated = [...current];
        updated[matchIndex] = nextJob;
        return updated;
      }
      return [nextJob, ...current];
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlistenPromise = listenAppEvent("job.progress", (payload) => {
      if (!disposed) {
        handleProgress(payload);
      }
    });
    return () => {
      disposed = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [handleProgress]);

  const processPath = useCallback(async (path: string) => {
    const filename = basename(path);
    const localId = `local-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    setJobs((current) => [
      {
        id: localId,
        filename,
        stage: "queued",
        message: "Queued for OCR",
        progress: 0,
        status: "running",
      },
      ...current,
    ]);

    try {
      const record = await processPdf(path);
      setJobs((current) =>
        current.map((job) =>
          job.id === localId || job.documentId === record.id
            ? {
                ...job,
                documentId: record.id,
                filename,
                stage: record.status,
                message: record.status === "partial_success" ? "Finished with page-level OCR errors" : "Searchable PDF written",
                progress: 100,
                status: record.status === "error" ? "error" : "done",
                outputPath: record.output_path,
                pageCount: record.page_count,
              }
            : job,
        ),
      );
    } catch (error) {
      setJobs((current) =>
        current.map((job) =>
          job.id === localId
            ? {
                ...job,
                stage: "error",
                message: "Processing failed",
                progress: 100,
                status: "error",
                error: error instanceof Error ? error.message : String(error),
              }
            : job,
        ),
      );
    }
  }, []);

  useEffect(() => {
    const unlistenPromise = getCurrentWebviewWindow().onDragDropEvent((event) => {
      const payload = event.payload as { type: string; paths?: string[] };
      if (payload.type === "enter" || payload.type === "over") {
        setIsDragging(true);
      }
      if (payload.type === "leave") {
        setIsDragging(false);
      }
      if (payload.type === "drop") {
        setIsDragging(false);
        for (const path of payload.paths ?? []) {
          if (path.toLowerCase().endsWith(".pdf")) {
            void processPath(path);
          }
        }
      }
    });

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [processPath]);

  async function choosePdf() {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof selected === "string") {
        await processPath(selected);
      }
    } catch (error) {
      console.error("Unable to process PDF", error);
    }
  }

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6">
      <section
        className={cn(
          "relative overflow-hidden rounded-xl border border-border bg-card/70 p-8 shadow-2xl shadow-black/20 transition-colors",
          isDragging && "border-foreground/45 bg-secondary/70",
        )}
      >
        <div className="pointer-events-none absolute inset-0 opacity-70 [background:radial-gradient(circle_at_18%_18%,oklch(0.9_0_0/0.13),transparent_18rem),linear-gradient(115deg,transparent_0_45%,oklch(1_0_0/0.06)_45%_46%,transparent_46%)]" />
        <div className="relative grid gap-8 lg:grid-cols-[1.2fr_0.8fr] lg:items-center">
          <div className="space-y-5">
            <div className="inline-flex items-center gap-2 rounded-full border border-border bg-background/60 px-3 py-1 font-mono text-xs uppercase tracking-[0.22em] text-muted-foreground">
              <span className="size-1.5 rounded-full bg-foreground" />
              OCR intake bay
            </div>
            <div className="space-y-3">
              <h1 className="max-w-2xl text-4xl font-semibold tracking-[-0.06em] text-foreground">
                Pick a PDF. Preserve the original. Add a hidden searchable text layer.
              </h1>
              <p className="max-w-xl text-sm leading-6 text-muted-foreground">
                Phase 1 runs local Tesseract OCR, writes the processed PDF to your configured output folder, and indexes every page in SQLite.
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button type="button" size="lg" onClick={() => void choosePdf()}>
                <UploadCloud className="size-4" />
                Choose PDF
              </Button>
              <div className="rounded-lg border border-border bg-background/50 px-3 py-2 font-mono text-xs text-muted-foreground">
                {runningCount > 0 ? `${runningCount} active job${runningCount === 1 ? "" : "s"}` : "Ready"}
              </div>
            </div>
          </div>

          <div
            className={cn(
              "grid min-h-52 place-items-center rounded-xl border border-dashed border-border bg-background/35 p-6 text-center transition-colors",
              isDragging && "border-foreground/60 bg-background/70",
            )}
          >
            <div className="space-y-3">
              <div className="mx-auto grid size-12 place-items-center rounded-xl border border-border bg-secondary/70">
                <FileText className="size-5 text-foreground" />
              </div>
              <div>
                <p className="font-medium text-foreground">Drop scanned or digital-born PDFs here</p>
                <p className="mt-1 text-xs text-muted-foreground">Native text pages skip OCR automatically.</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="rounded-xl border border-border bg-card/60">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h2 className="text-sm font-medium text-foreground">Processing queue</h2>
            <p className="text-xs text-muted-foreground">Live OCR, compose, and indexing progress.</p>
          </div>
        </div>

        {jobs.length === 0 ? (
          <div className="px-4 py-10 text-center text-sm text-muted-foreground">
            No jobs yet. Choose a PDF to start the Phase 1 pipeline.
          </div>
        ) : (
          <div className="divide-y divide-border">
            {jobs.map((job) => (
              <JobItem key={job.id} job={job} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function JobItem({ job }: { job: JobRow }) {
  const isDone = job.status === "done";
  const isError = job.status === "error";
  const Icon = isDone ? CheckCircle2 : isError ? XCircle : Loader2;

  return (
    <div className="grid gap-3 px-4 py-4 sm:grid-cols-[1fr_auto] sm:items-center">
      <div className="min-w-0 space-y-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className={cn("size-4 shrink-0", !isDone && !isError && "animate-spin", isDone && "text-emerald-400", isError && "text-destructive")} />
          <div className="truncate font-medium text-foreground">{job.filename}</div>
          <span className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
            {formatStage(job.stage)}
          </span>
        </div>
        <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
          <div className="h-full rounded-full bg-foreground transition-all duration-500" style={{ width: `${job.progress}%` }} />
        </div>
        <p className="text-xs text-muted-foreground">
          {job.error ?? job.message}
          {job.pageCount ? ` · ${job.pageCount} page${job.pageCount === 1 ? "" : "s"}` : null}
        </p>
      </div>
      <div className="font-mono text-xs text-muted-foreground sm:text-right">
        {Math.round(job.progress)}%
      </div>
    </div>
  );
}

function basename(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : path;
}

function formatStage(stage: string) {
  return stage.replace(/_/g, " ");
}
