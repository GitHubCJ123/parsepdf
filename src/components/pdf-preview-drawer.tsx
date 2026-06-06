
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Document, Page, pdfjs } from "react-pdf";
import "react-pdf/dist/Page/AnnotationLayer.css";
import "react-pdf/dist/Page/TextLayer.css";
import { ChevronDown, Copy, ExternalLink, Loader2, RotateCcw, Search, Trash2, X, ZoomIn, ZoomOut } from "lucide-react";
import { format } from "date-fns";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { type DocumentDetail, type EngineInfo, libraryForceReprocess, libraryOpenExternal, listOcrEngines, type SearchHit, searchDocument } from "@/lib/ipc";
import { notifyError, notifySuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";

pdfjs.GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).toString();

type PdfPreviewDrawerProps = {
  detail: DocumentDetail | null;
  loading: boolean;
  initialPage: number;
  onClose: () => void;
  onDelete?: (documentId: number) => Promise<void>;
  /** Provided by the Library: enables the "Reprocess with engine" control and
   * is called after a reprocess job is queued so the caller can refresh/close. */
  onReprocessed?: () => void;
  eyebrow?: string;
  citation?: ReactNode;
};

type ZoomMode = number | "fit";

export function PdfPreviewDrawer({ detail, loading, initialPage, onClose, onDelete, onReprocessed, eyebrow = "Preview drawer", citation }: PdfPreviewDrawerProps) {
  const document = detail?.document ?? null;
  const pageCount = Math.max(1, document?.page_count ?? 1);
  const [page, setPage] = useState(clampPage(initialPage, pageCount));
  const [pageInput, setPageInput] = useState(String(clampPage(initialPage, pageCount)));
  const [zoom, setZoom] = useState<ZoomMode>("fit");
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [containerWidth, setContainerWidth] = useState(760);
  const observerRef = useRef<ResizeObserver | null>(null);

  // Callback ref: the viewport only mounts after loading finishes, so a one-shot
  // mount effect would miss it and leave containerWidth stuck at its initial
  // value (which shrank the PDF). This attaches the observer whenever the node
  // appears and measures it immediately.
  const setViewportRef = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect();
    if (!node) return;
    setContainerWidth(node.clientWidth);
    const observer = new ResizeObserver(([entry]) => setContainerWidth(entry.contentRect.width));
    observer.observe(node);
    observerRef.current = observer;
  }, []);

  // Full ordered text of the rendered page (one entry per pdf.js text item),
  // captured via onGetTextSuccess. Needed for phrase highlighting: a phrase can
  // span several items, so we match against the joined text, then mark only the
  // matched slice inside each item.
  const [textItems, setTextItems] = useState<string[]>([]);

  useEffect(() => {
    const next = clampPage(initialPage, pageCount);
    setPage(next);
    setPageInput(String(next));
  }, [document?.id, initialPage, pageCount]);

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      const parsed = Number.parseInt(pageInput, 10);
      if (Number.isFinite(parsed)) setPage(clampPage(parsed, pageCount));
    }, 250);
    return () => window.clearTimeout(timeout);
  }, [pageInput, pageCount]);

  useEffect(() => {
    let cancelled = false;
    const trimmed = searchQuery.trim();
    if (!document || !trimmed) {
      setSearchHits([]);
      setSearchError(null);
      setSearching(false);
      return;
    }
    const timeout = window.setTimeout(async () => {
      setSearching(true);
      setSearchError(null);
      try {
        const hits = await searchDocument(document.id, trimmed);
        if (!cancelled) setSearchHits(hits);
      } catch (error) {
        if (!cancelled) {
          setSearchHits([]);
          setSearchError(String(error));
        }
      } finally {
        if (!cancelled) setSearching(false);
      }
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [document, searchQuery]);

  const pdfUrl = document?.output_path ? convertFileSrc(document.output_path) : null;
  const visibleHit = searchHits.find((hit) => hit.page_number === page);
  const terms = useMemo(() => searchTerms(searchQuery), [searchQuery]);
  const renderWidth = zoom === "fit" ? Math.max(320, Math.min(containerWidth - 32, 1600)) : undefined;

  // Per-item highlight ranges for the current phrase. Matches the phrase against
  // the joined page text (so it crosses item boundaries), then maps each match
  // back to the local character ranges inside each text item.
  const itemHighlights = useMemo(() => computeItemHighlights(textItems, terms), [textItems, terms]);

  // react-pdf calls this per text item; we return HTML with <mark> around only
  // the matched slice. The DOM-<mark> path is what reliably renders highlights in
  // this WebView, so phrase logic is precomputed and applied here per item.
  const renderItem = useCallback(
    ({ str, itemIndex }: { str: string; itemIndex: number }) => markItem(str, itemHighlights.get(itemIndex)),
    [itemHighlights],
  );

  async function openExternal() {
    if (!document) return;
    try {
      await libraryOpenExternal(document.id);
    } catch (error) {
      notifyError(`PDF could not be opened. ${String(error)}`);
    }
  }

  async function copyPath() {
    if (!document?.output_path) return;
    await navigator.clipboard.writeText(document.output_path);
    notifySuccess("Path copied.");
  }

  // Reprocess (Library only): load installed engines and re-run OCR with the
  // chosen one. Source is the original file (matched by SHA256), so the doc row
  // is reused rather than duplicated.
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [reprocessEngine, setReprocessEngine] = useState("");
  const [reprocessing, setReprocessing] = useState(false);
  const installedEngines = useMemo(() => engines.filter((engine) => engine.status === "installed"), [engines]);

  useEffect(() => {
    if (!onReprocessed) return;
    let cancelled = false;
    void listOcrEngines().then((list) => { if (!cancelled) setEngines(list); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [onReprocessed]);

  useEffect(() => {
    // Default the picker to the document's current engine.
    setReprocessEngine(document?.ocr_engine ?? "");
  }, [document?.id, document?.ocr_engine]);

  async function handleReprocess() {
    if (!document) return;
    const engineId = reprocessEngine || document.ocr_engine || installedEngines[0]?.id;
    setReprocessing(true);
    try {
      await libraryForceReprocess(document.id, engineId ?? undefined);
      const label = installedEngines.find((engine) => engine.id === engineId)?.name ?? engineId ?? "default engine";
      notifySuccess(`Reprocessing with ${label}…`);
      onReprocessed?.();
    } catch (error) {
      notifyError(`Reprocess failed. ${String(error)}`);
    } finally {
      setReprocessing(false);
    }
  }

  return (
    <Dialog open={Boolean(detail || loading)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent showCloseButton={false} className="top-0 left-auto right-0 bottom-0 flex h-dvh w-[80vw] max-w-none translate-x-0 translate-y-0 grid-rows-none flex-col gap-0 rounded-none border-y-0 border-r-0 border-l border-border bg-background/95 p-0 shadow-2xl shadow-black/50 backdrop-blur-xl sm:max-w-none">
        <div className="flex items-start justify-between gap-4 border-b border-border p-4">
          <div className="min-w-0">
            <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">{eyebrow}</p>
            <h2 className="mt-1 truncate text-xl font-semibold tracking-[-0.04em]">{document?.display_name ?? "Loading"}</h2>
            {document?.ai_summary ? <p className="mt-2 line-clamp-2 text-sm leading-6 text-muted-foreground">{document.ai_summary}</p> : null}
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

            {citation ? <div className="border-b border-border bg-emerald-300/5 p-4">{citation}</div> : null}

            <div className="space-y-3 border-b border-border p-3">
              <div className="flex flex-wrap items-center gap-2">
                <label className="flex items-center gap-2 text-sm text-muted-foreground">
                  <span>Page</span>
                  <Input type="number" min={1} max={pageCount} value={pageInput} onChange={(event) => setPageInput(event.target.value)} className="h-8 w-20" aria-label={`Page ${page} of ${pageCount}`} />
                  <span>of {pageCount}</span>
                </label>
                <div className="ml-auto flex flex-wrap items-center gap-1">
                  <Button type="button" size="sm" variant="outline" disabled={page <= 1} onClick={() => { const next = page - 1; setPage(next); setPageInput(String(next)); }}>Prev</Button>
                  <Button type="button" size="sm" variant="outline" disabled={page >= pageCount} onClick={() => { const next = page + 1; setPage(next); setPageInput(String(next)); }}>Next</Button>
                  <Button type="button" size="icon-sm" variant="outline" onClick={() => setZoom((value) => typeof value === "number" ? Math.max(0.5, value - 0.15) : 0.85)} aria-label="Zoom out"><ZoomOut className="size-4" /></Button>
                  <Button type="button" size="icon-sm" variant="outline" onClick={() => setZoom((value) => typeof value === "number" ? Math.min(2.5, value + 0.15) : 1.15)} aria-label="Zoom in"><ZoomIn className="size-4" /></Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => setZoom("fit")}>Fit</Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => setZoom(1)}>100%</Button>
                </div>
              </div>
              <div className="grid gap-2 md:grid-cols-[1fr_auto] md:items-start">
                <div className="relative">
                  <Search className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
                  <Input value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} placeholder="Search within document" className="pl-8" />
                </div>
                <div className="text-xs text-muted-foreground md:pt-2">
                  {searching ? "Searching" : searchQuery.trim() ? `${searchHits.length} match${searchHits.length === 1 ? "" : "es"}` : ""}
                </div>
              </div>
              {searchError ? <p className="text-xs text-destructive">Search failed. Try different words.</p> : null}
              {visibleHit ? <Snippet snippetHtml={visibleHit.snippet_html} terms={terms} /> : null}
              {searchHits.length > 0 ? (
                <div className="flex gap-1 overflow-x-auto pb-1">
                  {searchHits.slice(0, 24).map((hit) => (
                    <button key={hit.page_id} type="button" onClick={() => { setPage(hit.page_number); setPageInput(String(hit.page_number)); }} className={cn("rounded border px-2 py-1 font-mono text-[11px] text-muted-foreground", hit.page_number === page && "border-foreground/30 bg-secondary text-foreground")}>p.{hit.page_number}</button>
                  ))}
                </div>
              ) : null}
            </div>

            <div ref={setViewportRef} className="min-h-0 flex-1 overflow-auto bg-black/25 p-4">
              {pdfUrl ? (
                <div className="mx-auto w-fit max-w-full">
                  <Document file={pdfUrl} loading={<PreviewLoader />} error={<PreviewError />}>
                    <Page
                      pageNumber={page}
                      renderAnnotationLayer
                      renderTextLayer
                      width={renderWidth}
                      scale={zoom === "fit" ? undefined : zoom}
                      customTextRenderer={renderItem}
                      onGetTextSuccess={({ items }) => setTextItems(items.map((item) => ("str" in item ? item.str : "")))}
                      loading={<PreviewLoader />}
                    />
                  </Document>
                </div>
              ) : (
                <div className="grid h-full place-items-center text-sm text-muted-foreground">No output PDF path recorded.</div>
              )}
            </div>

            <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-border p-4">
              <div className="flex flex-wrap items-center gap-2">
                <Button type="button" variant="outline" onClick={() => void openExternal()} disabled={!document.output_path}><ExternalLink className="size-4" />Open externally</Button>
                <Button type="button" variant="outline" onClick={() => void copyPath()} disabled={!document.output_path}><Copy className="size-4" />Copy path</Button>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {onReprocessed && installedEngines.length > 0 ? (
                  <div className="flex items-center gap-2 rounded-xl border border-primary/30 bg-primary/5 p-1.5 pl-3 shadow-sm">
                    <RotateCcw className="size-4 text-foreground/80" />
                    <span className="text-xs font-semibold uppercase tracking-[0.16em] text-foreground/80">Re-OCR with</span>
                    <div className="relative">
                      <select
                        value={reprocessEngine || document.ocr_engine || installedEngines[0]?.id}
                        onChange={(event) => setReprocessEngine(event.target.value)}
                        disabled={reprocessing}
                        className="h-9 appearance-none rounded-lg border border-input bg-background pl-3 pr-8 text-sm font-medium text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring [color-scheme:dark]"
                      >
                        {installedEngines.map((engine) => (
                          <option key={engine.id} value={engine.id} className="bg-popover text-popover-foreground">{engine.name}</option>
                        ))}
                      </select>
                      <ChevronDown className="pointer-events-none absolute right-2 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                    </div>
                    <Button type="button" onClick={() => void handleReprocess()} disabled={reprocessing}>
                      {reprocessing ? <Loader2 className="size-4 animate-spin" /> : <RotateCcw className="size-4" />}
                      Reprocess
                    </Button>
                  </div>
                ) : null}
                {onDelete ? <Button type="button" variant="destructive" onClick={() => void onDelete(document.id)}><Trash2 className="size-4" />Delete</Button> : null}
              </div>
            </footer>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PreviewLoader() {
  return <div className="grid h-96 place-items-center text-muted-foreground"><Loader2 className="size-5 animate-spin" /></div>;
}

function PreviewError() {
  return <div className="grid h-96 place-items-center rounded-lg border border-border bg-background p-6 text-sm text-muted-foreground">Preview could not load. Open the PDF externally.</div>;
}

function Snippet({ snippetHtml, terms }: { snippetHtml: string; terms: string[] }) {
  // The backend snippet marks EVERY matched FTS token (incl. stop words like
  // "the"/"of"). Re-highlight from the plain text with the same stop-word-aware
  // terms the PDF uses, so the two stay consistent.
  const html = useMemo(() => {
    const template = document.createElement("template");
    template.innerHTML = snippetHtml;
    const text = template.content.textContent ?? "";
    return highlightText(text, terms);
  }, [snippetHtml, terms]);
  return <div className="rounded-lg border border-border bg-background/60 px-3 py-2 text-xs leading-5 text-muted-foreground [&_mark]:rounded [&_mark]:bg-amber-400/25 [&_mark]:px-0.5 [&_mark]:text-amber-100" dangerouslySetInnerHTML={{ __html: html }} />;
}

function Meta({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return <div className={cn("min-w-0", wide && "sm:col-span-2")}><div className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/80">{label}</div><div className="mt-1 truncate text-foreground/85">{value}</div></div>;
}

function clampPage(value: number, max: number) {
  return Math.max(1, Math.min(Math.max(1, max), value || 1));
}

function searchTerms(query: string) {
  // Ordered query tokens (NOT deduped — order matters for phrase matching).
  // Keep the punctuation people actually search for ( - / ( ) ) so things like
  // "(1754-1783)" or "and/or" can be matched; strip only stray noise.
  return query
    .toLowerCase()
    .split(/\s+/)
    .map((term) => term.replace(/[^\p{L}\p{N}_/()-]/gu, ""))
    .filter((term) => term.length > 0)
    .slice(0, 24);
}

function highlightText(text: string, terms: string[]) {
  // PHRASE match: the words must appear consecutively (separated only by
  // whitespace), so "the anatomy of a revolution" highlights ONLY where that
  // whole phrase occurs together — not every stray "of"/"a". A single-word
  // query still works (the phrase is just that one word).
  const pattern = buildPhraseRegex(terms, "\\s+");
  if (!pattern) return escapeHtml(text);
  let result = "";
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (match[0].length === 0) continue;
    result += escapeHtml(text.slice(last, index));
    result += `<mark>${escapeHtml(match[0])}</mark>`;
    last = index + match[0].length;
  }
  result += escapeHtml(text.slice(last));
  return result;
}

// Hyphen the user types should also match the unicode dash variants PDFs use
// for ranges (en/em dash, figure dash, minus) — e.g. typed "1754-1783" matches
// rendered "1754–1783".
const DASH_VARIANTS = "[-\\u2010\\u2011\\u2012\\u2013\\u2014\\u2015\\u2212]";

function termToRegex(term: string) {
  // Allow optional whitespace around a typed hyphen so "1754-1783" also matches a
  // rendered range that pdf.js split into separate spans ("1754 – 1783").
  return escapeRegex(term).replace(/-/g, `\\s*${DASH_VARIANTS}\\s*`);
}

// Build a phrase regex from ordered query words. Each word is matched literally
// (special chars like ( ) / - are escaped/handled), words are joined by `gap`
// whitespace, and word-boundary lookarounds are only applied on an end that is
// an actual word char — so "(1754-1783)" still matches even though it starts and
// ends with punctuation.
function buildPhraseRegex(words: string[], gap: string): RegExp | null {
  const clean = words.filter((word) => word.length > 0);
  if (clean.length === 0) return null;
  const phrase = clean.map(termToRegex).join(gap);
  const isWordChar = (ch: string) => /[\p{L}\p{N}]/u.test(ch);
  const pre = isWordChar(clean[0][0]) ? "(?<![\\p{L}\\p{N}])" : "";
  const post = isWordChar(clean[clean.length - 1].slice(-1)) ? "(?![\\p{L}\\p{N}])" : "";
  try {
    return new RegExp(`${pre}(?:${phrase})${post}`, "giu");
  } catch {
    return null;
  }
}

function escapeRegex(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// Compute, for each pdf.js text item, the local character ranges that fall
// inside a phrase match. The page is split into many items (often one word
// each, with the spaces between them dropped), so we join the items with a
// single space, match the phrase against that joined text, then map each match
// back to per-item [start, end) slices.
function computeItemHighlights(items: string[], terms: string[]): Map<number, [number, number][]> {
  const result = new Map<number, [number, number][]>();
  const clean = terms.filter((term) => term.length > 0);
  if (items.length === 0 || clean.length === 0) return result;

  // Track where each item sits within the joined string (with a 1-char " "
  // separator between items, mirroring how the words actually read).
  const spans: { index: number; start: number; end: number }[] = [];
  let joined = "";
  items.forEach((str, index) => {
    if (joined.length > 0) joined += " ";
    const start = joined.length;
    joined += str;
    spans.push({ index, start, end: start + str.length });
  });

  const pattern = buildPhraseRegex(clean, "\\s*");
  if (!pattern) return result;

  for (const match of joined.matchAll(pattern)) {
    const mStart = match.index ?? 0;
    const mEnd = mStart + match[0].length;
    if (mEnd <= mStart) continue;
    for (const span of spans) {
      const from = Math.max(span.start, mStart);
      const to = Math.min(span.end, mEnd);
      if (to <= from) continue;
      const local: [number, number] = [from - span.start, to - span.start];
      const existing = result.get(span.index);
      if (existing) existing.push(local);
      else result.set(span.index, [local]);
    }
  }
  return result;
}

// Build the HTML react-pdf sets as a text item's innerHTML: the item text with
// <mark> wrapped around each highlighted slice. Everything is HTML-escaped.
function markItem(str: string, ranges?: [number, number][]) {
  if (!ranges || ranges.length === 0) return escapeHtml(str);
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  let result = "";
  let cursor = 0;
  for (const [start, end] of sorted) {
    const from = Math.max(cursor, start);
    if (from > cursor) result += escapeHtml(str.slice(cursor, from));
    if (end > from) result += `<mark>${escapeHtml(str.slice(from, end))}</mark>`;
    cursor = Math.max(cursor, end);
  }
  result += escapeHtml(str.slice(cursor));
  return result;
}

function escapeHtml(value: string) {
  return value.split("&").join("&amp;").split("<").join("&lt;").split(">").join("&gt;").split('"').join("&quot;").split("'").join("&#39;");
}

function formatDate(value: number) {
  return format(new Date(value * 1000), "MMM d, yyyy");
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
