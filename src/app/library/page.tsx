
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useState } from "react";
import { format } from "date-fns";
import { Library as LibraryIcon, Loader2, Search } from "lucide-react";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { EmptyState } from "@/components/empty-state";
import { PdfPreviewDrawer } from "@/components/pdf-preview-drawer";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { libraryDelete, libraryGet, libraryList, type DocumentDetail, type DocumentRow } from "@/lib/ipc";
import { getSetting } from "@/lib/db";
import { cn } from "@/lib/utils";

export function LibraryPage() {
  const [documents, setDocuments] = useState<DocumentRow[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [previewPage, setPreviewPage] = useState(1);
  const [detail, setDetail] = useState<DocumentDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<number | null>(null);

  async function refresh() {
    setLoading(true);
    try {
      const limit = parseLoadLimit(await getSetting("library.page_size"));
      const rows = await libraryList(undefined, limit, 0);
      setDocuments(rows);
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
    return documents.filter((document) => [document.display_name, document.original_name, document.ocr_engine, document.ai_provider].filter(Boolean).join(" ").toLowerCase().includes(needle));
  }, [documents, query]);

  async function deleteSelected(documentId: number) {
    const confirmSetting = await getSetting("library.confirm_delete");
    const shouldConfirm = confirmSetting == null ? true : confirmSetting === "1";
    if (shouldConfirm) {
      // Open the in-app styled confirm modal. `window.confirm` is suppressed in
      // the WebView2 runtime, and the native OS dialog looks out of place.
      setPendingDelete(documentId);
      return;
    }
    await performDelete(documentId);
  }

  async function performDelete(documentId: number) {
    // Always remove the processed PDF from disk along with the DB record.
    await libraryDelete(documentId, true);
    setSelectedId(null);
    await refresh();
  }

  function openDocument(documentId: number, page = 1) {
    setPreviewPage(page);
    setSelectedId(documentId);
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-5">
      <header className="overflow-hidden rounded-xl border border-border bg-card/70 p-6 shadow-2xl shadow-black/20">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.24em] text-muted-foreground">Processed archive</p>
            <h1 className="mt-2 text-3xl font-semibold tracking-[-0.055em]">Library</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">Browse searchable PDFs, inspect OCR metadata, and preview the rendered file without leaving the app.</p>
          </div>
          <div className="relative w-full max-w-sm">
            <Search className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
            <Input data-global-search="true" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter by name, engine, or provider" className="pl-8" />
          </div>
        </div>
      </header>

      <section className="overflow-hidden rounded-xl border border-border bg-card/65">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h2 className="text-sm font-medium">Processed PDFs</h2>
            <p className="text-xs text-muted-foreground">{filteredDocuments.length} visible · click a row to preview</p>
          </div>
          <Button type="button" variant="outline" onClick={() => void refresh()} disabled={loading}>{loading ? <Loader2 className="size-4 animate-spin" /> : null}Refresh</Button>
        </div>

        {loading ? (
          <div className="grid place-items-center px-4 py-16 text-sm text-muted-foreground"><Loader2 className="mb-3 size-5 animate-spin" />Loading library…</div>
        ) : documents.length === 0 ? (
          <EmptyState icon={LibraryIcon} title="Your library is empty" description="Process a PDF to start building your searchable archive." actionLabel="Go to Inbox" onAction={() => window.location.assign("/inbox")} />
        ) : filteredDocuments.length === 0 ? (
          <EmptyState icon={Search} title="No matches" description="Try fewer or different words." />
        ) : (
          <div className="overflow-auto">
            <table className="w-full min-w-[860px] text-left text-sm">
              <thead className="bg-background/40 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                <tr><Th>Name</Th><Th>Original</Th><Th>Pages</Th><Th>Date</Th><Th>Engine</Th><Th>AI provider</Th><Th>Size</Th></tr>
              </thead>
              <tbody className="divide-y divide-border">
                {filteredDocuments.map((document) => (
                  <tr key={document.id} tabIndex={0} role="button" onClick={() => openDocument(document.id)} onKeyDown={(event: KeyboardEvent<HTMLTableRowElement>) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); openDocument(document.id); } }} className="cursor-pointer transition-colors hover:bg-secondary/45 focus-visible:bg-secondary/45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
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

      <PdfPreviewDrawer
        detail={detail}
        loading={detailLoading}
        initialPage={previewPage}
        onClose={() => { setSelectedId(null); setPreviewPage(1); }}
        onDelete={deleteSelected}
        onReprocessed={() => { setSelectedId(null); setPreviewPage(1); void refresh(); }}
      />

      <ConfirmDialog
        open={pendingDelete != null}
        onOpenChange={(open) => { if (!open) setPendingDelete(null); }}
        destructive
        title="Delete document"
        description="This permanently deletes the document and removes its searchable PDF from disk. This cannot be undone."
        confirmLabel="Delete permanently"
        onConfirm={async () => {
          if (pendingDelete != null) await performDelete(pendingDelete);
        }}
      />
    </div>
  );
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

function parseLoadLimit(value: string | null) {
  // Backend clamps to 1..500; default to 300 when unset or invalid.
  return parsePositiveInt(value) ?? 300;
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
