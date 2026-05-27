import { type ReactNode, useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import { format } from "date-fns";
import { Copy, ExternalLink, Library, Loader2, Search, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { libraryDelete, libraryGet, libraryList, libraryOpenExternal, libraryPendingRenames, type DocumentDetail, type DocumentRow, type PendingRenameRow } from "@/lib/ipc";
import { cn } from "@/lib/utils";

export function LibraryPage() {
  const [documents, setDocuments] = useState<DocumentRow[]>([]);
  const [pendingRenames, setPendingRenames] = useState<PendingRenameRow[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [previewPage, setPreviewPage] = useState(1);
  const [detail, setDetail] = useState<DocumentDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const [rows, pending] = await Promise.all([libraryList(undefined, 300, 0), libraryPendingRenames()]);
      setDocuments(rows);
      setPendingRenames(pending);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const params = new URLSearchParams(window.location.search);
    const documentId = parsePositiveInt(params.get("doc"));
    const page = parsePositiveInt(params.get("page"));
    if (documentId != null) {
      setSelectedId(documentId);
      setPreviewPage(page ?? 1);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function loadDetail() {
      if (selectedId == null) {
        setDetail(null);
        return;
      }
      setDetailLoading(true);
      try {
        const next = await libraryGet(selectedId);
        if (!cancelled) setDetail(next);
      } finally {
        if (!cancelled) setDetailLoading(false);
      }
    }
    void loadDetail();
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const filteredDocuments = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return documents;
    return documents.filter((document) =>
      [document.display_name, document.original_name, document.ocr_engine, document.ai_provider]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  }, [documents, query]);

  async function deleteSelected(documentId: number) {
    await libraryDelete(documentId, false);
    setSelectedId(null);
    await refresh();
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-5">
      <header className="overflow-hidden rounded-xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.24em] text-muted-foreground">Processed archive</p>
            <h1 className="mt-2 text-3xl font-semibold tracking-[-0.055em]">Library</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
              Browse searchable PDFs, inspect OCR metadata, and preview the rendered file without leaving the app.
            </p>
          </div>
          <div className="relative w-full max-w-sm">
            <Search className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
            <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter by name, engine, provider…" className="pl-8" />
          </div>
        </div>
      </header>

      {pendingRenames.length > 0 && (
        <Link to="/library/review-renames" className="flex items-center justify-between rounded-xl border border-amber-400/25 bg-amber-400/10 px-4 py-3 text-sm text-amber-100 transition-colors hover:bg-amber-400/15">
          <span>{pendingRenames.length} document{pendingRenames.length === 1 ? "" : "s"} ready for naming.</span>
          <span className="font-medium">Review →</span>
        </Link>
      )}

      <section className="overflow-hidden rounded-xl border border-border bg-card/65">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h2 className="text-sm font-medium">Processed PDFs</h2>
            <p className="text-xs text-muted-foreground">{filteredDocuments.length} visible · click a row to preview</p>
          </div>
          <Button type="button" variant="outline" onClick={() => void refresh()} disabled={loading}>
            {loading ? <Loader2 className="size-4 animate-spin" /> : null}
            Refresh
          </Button>
        </div>

        {loading ? (
          <div className="grid place-items-center px-4 py-16 text-sm text-muted-foreground">
            <Loader2 className="mb-3 size-5 animate-spin" />
            Loading library…
          </div>
        ) : filteredDocuments.length === 0 ? (
          <div className="grid place-items-center px-4 py-16 text-center">
            <div className="grid size-12 place-items-center rounded-xl border border-border bg-secondary/70">
              <Library className="size-5" />
            </div>
            <h3 className="mt-4 text-lg font-semibold tracking-[-0.04em]">No documents yet.</h3>
            <p className="mt-2 text-sm text-muted-foreground">Process a PDF to get started.</p>
            <Button asChild className="mt-4">
              <Link to="/inbox">Process PDF</Link>
            </Button>
          </div>
        ) : (
          <div className="overflow-auto">
            <table className="w-full min-w-[860px] text-left text-sm">
              <thead className="bg-background/40 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                <tr>
                  <Th>Name</Th>
                  <Th>Original</Th>
                  <Th>Pages</Th>
                  <Th>Date</Th>
                  <Th>Engine</Th>
                  <Th>AI Provider</Th>
                  <Th>Size</Th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {filteredDocuments.map((document) => (
                  <tr key={document.id} onClick={() => { setPreviewPage(1); setSelectedId(document.id); }} className="cursor-pointer transition-colors hover:bg-secondary/45">
                    <Td className="max-w-[20rem]"><div className="truncate font-medium">{document.display_name}</div></Td>
                    <Td className="max-w-[16rem]"><div className="truncate text-muted-foreground">{document.original_name}</div></Td>
                    <Td>{document.page_count}</Td>
                    <Td>{formatDate(document.ingested_at)}</Td>
                    <Td><Badge>{document.ocr_engine ?? "—"}</Badge></Td>
                    <Td><Badge muted>{document.ai_provider ?? "none"}</Badge></Td>
                    <Td>{formatBytes(document.size_bytes)}</Td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <PreviewDrawer detail={detail} loading={detailLoading} currentPage={previewPage} onClose={() => { setSelectedId(null); setPreviewPage(1); }} onDelete={deleteSelected} />
    </div>
  );
}

function PreviewDrawer({ detail, loading, currentPage, onClose, onDelete }: { detail: DocumentDetail | null; loading: boolean; currentPage: number; onClose: () => void; onDelete: (documentId: number) => Promise<void> }) {
  const [page, setPage] = useState(currentPage);
  useEffect(() => {
    const maxPage = Math.max(1, detail?.document.page_count ?? currentPage);
    setPage(Math.min(maxPage, Math.max(1, currentPage)));
  }, [detail?.document.id, detail?.document.page_count, currentPage]);

  if (!detail && !loading) return null;

  const document = detail?.document;
  const pdfUrl = document?.output_path ? convertFileSrc(document.output_path) : null;

  return (
    <div className="fixed inset-y-0 right-0 z-40 flex w-full max-w-3xl flex-col border-l border-border bg-background/95 shadow-2xl shadow-black/40 backdrop-blur-xl">
      <div className="flex items-start justify-between gap-4 border-b border-border p-4">
        <div className="min-w-0">
          <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">Preview drawer</p>
          <h2 className="mt-1 truncate text-xl font-semibold tracking-[-0.04em]">{document?.display_name ?? "Loading…"}</h2>
          {document?.ai_summary && <p className="mt-2 line-clamp-2 text-sm leading-6 text-muted-foreground">{document.ai_summary}</p>}
        </div>
        <Button type="button" size="icon-sm" variant="ghost" onClick={onClose} aria-label="Close preview"><X className="size-4" /></Button>
      </div>

      {loading || !document ? (
        <div className="grid flex-1 place-items-center text-muted-foreground"><Loader2 className="size-5 animate-spin" /></div>
      ) : (
        <>
          <div className="grid gap-3 border-b border-border p-4 text-xs text-muted-foreground sm:grid-cols-2">
            <Meta label="Original" value={document.original_name} />
            <Meta label="Pages" value={String(document.page_count)} />
            <Meta label="OCR" value={document.ocr_engine ?? "—"} />
            <Meta label="AI" value={document.ai_provider ?? "none"} />
            <Meta label="Ingested" value={formatDate(document.ingested_at)} />
            <Meta label="Size" value={formatBytes(document.size_bytes)} />
            <Meta label="Output" value={document.output_path ?? "—"} wide />
            <Meta label="Original path" value={document.original_path} wide />
          </div>

          <div className="min-h-0 flex-1 overflow-auto bg-black/25 p-4">
            {pdfUrl ? (
              <div className="mx-auto max-w-2xl">
                <div className="mb-3 flex items-center justify-between rounded-lg border border-border bg-background/75 px-3 py-2 text-sm">
                  <span>Page {page} of {Math.max(1, document.page_count)}</span>
                  <div className="flex items-center gap-2">
                    <Button type="button" size="sm" variant="outline" disabled={page <= 1} onClick={() => setPage((value) => Math.max(1, value - 1))}>Prev</Button>
                    <Button type="button" size="sm" variant="outline" disabled={page >= Math.max(1, document.page_count)} onClick={() => setPage((value) => Math.min(Math.max(1, document.page_count), value + 1))}>Next</Button>
                  </div>
                </div>
                <PreviewFallback url={`${pdfUrl}#page=${page}`} />
              </div>
            ) : (
              <div className="grid h-full place-items-center text-sm text-muted-foreground">No output PDF path recorded.</div>
            )}
          </div>

          <footer className="flex flex-wrap items-center justify-between gap-2 border-t border-border p-4">
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="outline" onClick={() => void libraryOpenExternal(document.id)}><ExternalLink className="size-4" />Open externally</Button>
              <Button type="button" variant="outline" onClick={() => void navigator.clipboard.writeText(document.output_path ?? "")}><Copy className="size-4" />Copy path</Button>
            </div>
            <Button type="button" variant="destructive" onClick={() => void onDelete(document.id)}><Trash2 className="size-4" />Delete</Button>
          </footer>
        </>
      )}
    </div>
  );
}

function PreviewFallback({ url }: { url: string }) {
  return <object data={url} type="application/pdf" className="h-[72vh] w-full rounded-lg border border-border bg-background"><embed src={url} type="application/pdf" /></object>;
}

function Meta({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return <div className={cn("min-w-0", wide && "sm:col-span-2")}><div className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/80">{label}</div><div className="mt-1 truncate text-foreground/85">{value}</div></div>;
}

function Th({ children }: { children: ReactNode }) {
  return <th className="px-4 py-3 font-medium">{children}</th>;
}

function Td({ children, className }: { children: ReactNode; className?: string }) {
  return <td className={cn("px-4 py-3 align-middle", className)}>{children}</td>;
}

function Badge({ children, muted = false }: { children: ReactNode; muted?: boolean }) {
  return <span className={cn("inline-flex rounded border px-1.5 py-0.5 font-mono text-[11px]", muted ? "border-border text-muted-foreground" : "border-foreground/15 text-foreground")}>{children}</span>;
}

function formatDate(value: number) {
  return format(new Date(value * 1000), "MMM d, yyyy");
}

function parsePositiveInt(value: string | null) {
  if (!value) return null;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function formatBytes(value?: number | null) {
  if (!value) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

