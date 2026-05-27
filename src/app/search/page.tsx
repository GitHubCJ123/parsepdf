import { type KeyboardEvent, useEffect, useMemo, useState } from "react";
import { format } from "date-fns";
import { Bookmark, ChevronDown, ChevronRight, DatabaseZap, Loader2, Search, SlidersHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { execute, selectRows } from "@/lib/db";
import { searchDocuments, searchRebuildIndex, type SearchHit, type SearchQuery, type SearchResult, type SearchSort } from "@/lib/ipc";
import { cn } from "@/lib/utils";

type OcrEngine = "tesseract" | "rapidocr";

type SavedSearchRow = {
  id: number;
  name: string;
  query: string;
  created_at: number;
};

type SearchGroup = {
  document_id: number;
  display_name: string;
  document_ingested_at: number;
  ocr_engine: string | null;
  hits: SearchHit[];
};

const ENGINES: OcrEngine[] = ["tesseract", "rapidocr"];

export function SearchPage() {
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [selectedEngines, setSelectedEngines] = useState<OcrEngine[]>([]);
  const [sort, setSort] = useState<SearchSort>("relevance");
  const [result, setResult] = useState<SearchResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedSearches, setSavedSearches] = useState<SavedSearchRow[]>([]);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<number, boolean>>({});
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [searchNonce, setSearchNonce] = useState(0);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildMessage, setRebuildMessage] = useState<string | null>(null);
  const [savedOpen, setSavedOpen] = useState(true);

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebouncedQuery(query), 200);
    return () => window.clearTimeout(timeout);
  }, [query]);

  useEffect(() => {
    void refreshSavedSearches();
  }, []);

  useEffect(() => {
    let cancelled = false;
    const trimmed = debouncedQuery.trim();
    if (!trimmed) {
      setResult(null);
      setError(null);
      setLoading(false);
      setSelectedIndex(-1);
      return;
    }

    async function runSearch() {
      setLoading(true);
      setError(null);
      try {
        const next = await searchDocuments(buildSearchQuery(trimmed, dateFrom, dateTo, selectedEngines, sort));
        if (!cancelled) {
          setResult(next);
          setSelectedIndex(next.hits.length > 0 ? 0 : -1);
        }
      } catch (searchError) {
        if (!cancelled) {
          setResult(null);
          setError(String(searchError));
          setSelectedIndex(-1);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void runSearch();
    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, dateFrom, dateTo, selectedEngines, sort, searchNonce]);

  const groups = useMemo(() => groupHits(result?.hits ?? []), [result]);
  const flatHits = result?.hits ?? [];

  function toggleEngine(engine: OcrEngine) {
    setSelectedEngines((current) => (current.includes(engine) ? current.filter((value) => value !== engine) : [...current, engine]));
  }

  function handleResultsKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (flatHits.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((current) => Math.min(flatHits.length - 1, current < 0 ? 0 : current + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex((current) => Math.max(0, current < 0 ? flatHits.length - 1 : current - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      openHit(flatHits[Math.max(0, selectedIndex)]);
    }
  }

  async function refreshSavedSearches() {
    const rows = await selectRows<SavedSearchRow>("SELECT id, name, query, created_at FROM saved_searches ORDER BY created_at DESC LIMIT 25");
    setSavedSearches(rows);
  }

  async function saveCurrentSearch() {
    const trimmed = query.trim();
    if (!trimmed) return;
    const name = window.prompt("Save search as", trimmed);
    if (!name?.trim()) return;
    await execute("INSERT INTO saved_searches(name, query, created_at) VALUES(?1, ?2, ?3)", [
      name.trim(),
      JSON.stringify(buildSearchQuery(trimmed, dateFrom, dateTo, selectedEngines, sort)),
      Math.floor(Date.now() / 1000),
    ]);
    await refreshSavedSearches();
  }

  function applySavedSearch(row: SavedSearchRow) {
    try {
      const saved = JSON.parse(row.query) as Partial<SearchQuery>;
      setQuery(saved.q ?? "");
      setDateFrom(formatDateInput(saved.dateFrom));
      setDateTo(formatDateInput(saved.dateTo));
      setSelectedEngines(isOcrEngine(saved.engine) ? [saved.engine] : []);
      setSort(normalizeSort(saved.sort));
    } catch {
      setError("Saved search is no longer readable.");
    }
  }

  async function rebuildIndex() {
    setRebuilding(true);
    setRebuildMessage(null);
    try {
      const report = await searchRebuildIndex();
      setRebuildMessage(`Rebuilt ${report.pages} pages across ${report.documents} documents in ${report.took_ms} ms.`);
      setSearchNonce((value) => value + 1);
    } catch (rebuildError) {
      setRebuildMessage(`Rebuild failed: ${String(rebuildError)}`);
    } finally {
      setRebuilding(false);
    }
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-5">
      <section className="sticky top-0 z-20 rounded-xl border border-border bg-background/95 p-4 shadow-2xl shadow-black/25 backdrop-blur-xl">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" />
            <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search OCR text by name, date, invoice number, or phrase…" className="h-10 pl-9 text-sm" autoFocus />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" variant="outline" onClick={() => setSearchNonce((value) => value + 1)} disabled={!query.trim() || loading}>
              {loading ? <Loader2 className="size-4 animate-spin" /> : <Search className="size-4" />}
              Search
            </Button>
            <Button type="button" variant="outline" onClick={() => void saveCurrentSearch()} disabled={!query.trim()}>
              <Bookmark className="size-4" />
              Save search
            </Button>
          </div>
        </div>
      </section>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_18rem]">
        <main className="min-w-0 rounded-xl border border-border bg-card/60" tabIndex={0} onKeyDown={handleResultsKeyDown}>
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <h1 className="text-sm font-medium">Full-text search</h1>
              <p className="text-xs text-muted-foreground">
                {result ? `${result.total_hits} result${result.total_hits === 1 ? "" : "s"} in ${result.took_ms} ms` : "Search across all OCR'd documents."}
              </p>
            </div>
            {loading && <Loader2 className="size-4 animate-spin text-muted-foreground" />}
          </div>

          {result?.query_warnings.length ? (
            <div className="border-b border-amber-400/20 bg-amber-400/10 px-4 py-2 text-xs text-amber-100">{result.query_warnings.join(" · ")}</div>
          ) : null}

          {error ? (
            <div className="grid place-items-center px-4 py-16 text-center">
              <div className="max-w-md rounded-xl border border-destructive/30 bg-destructive/10 p-5">
                <h2 className="text-sm font-medium text-destructive">Search failed</h2>
                <p className="mt-2 text-sm text-muted-foreground">{error}</p>
                <Button type="button" variant="outline" className="mt-4" onClick={() => setSearchNonce((value) => value + 1)}>Retry</Button>
              </div>
            </div>
          ) : !query.trim() ? (
            <EmptySearchState title="Search across all OCR'd documents." description="Try a name, date, invoice number, or keyword. Quoted phrases are preserved." />
          ) : !loading && result && result.hits.length === 0 ? (
            <EmptySearchState title="No matches." description="Try fewer or different words, clear filters, or rebuild the search index." />
          ) : (
            <div className="divide-y divide-border">
              {groups.map((group) => {
                const collapsed = collapsedGroups[group.document_id] ?? false;
                return (
                  <section key={group.document_id}>
                    <button type="button" onClick={() => setCollapsedGroups((current) => ({ ...current, [group.document_id]: !collapsed }))} className="flex w-full items-center gap-3 bg-background/35 px-4 py-3 text-left transition-colors hover:bg-secondary/40">
                      {collapsed ? <ChevronRight className="size-4 text-muted-foreground" /> : <ChevronDown className="size-4 text-muted-foreground" />}
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium">{group.display_name}</div>
                        <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                          <span>{formatEpoch(group.document_ingested_at)}</span>
                          <span>•</span>
                          <span>{group.ocr_engine ?? "unknown engine"}</span>
                          <span>•</span>
                          <span>{group.hits.length} page hit{group.hits.length === 1 ? "" : "s"}</span>
                        </div>
                      </div>
                    </button>
                    {!collapsed && (
                      <div className="divide-y divide-border/70">
                        {group.hits.map((hit) => {
                          const absoluteIndex = flatHits.findIndex((item) => item.page_id === hit.page_id);
                          return <HitRow key={hit.page_id} hit={hit} selected={absoluteIndex === selectedIndex} onOpen={() => openHit(hit)} />;
                        })}
                      </div>
                    )}
                  </section>
                );
              })}
            </div>
          )}
        </main>

        <aside className="flex flex-col gap-4">
          <section className="rounded-xl border border-border bg-card/60 p-4">
            <div className="mb-4 flex items-center gap-2">
              <SlidersHorizontal className="size-4 text-muted-foreground" />
              <h2 className="text-sm font-medium">Filters</h2>
            </div>
            <div className="space-y-4">
              <label className="block text-xs font-medium text-muted-foreground">
                From
                <Input type="date" value={dateFrom} onChange={(event) => setDateFrom(event.target.value)} className="mt-1" />
              </label>
              <label className="block text-xs font-medium text-muted-foreground">
                To
                <Input type="date" value={dateTo} onChange={(event) => setDateTo(event.target.value)} className="mt-1" />
              </label>
              <div>
                <div className="text-xs font-medium text-muted-foreground">OCR engine</div>
                <div className="mt-2 space-y-2">
                  {ENGINES.map((engine) => (
                    <label key={engine} className="flex items-center gap-2 text-sm text-foreground/85">
                      <input type="checkbox" checked={selectedEngines.includes(engine)} onChange={() => toggleEngine(engine)} className="size-3.5 accent-primary" />
                      <span className="capitalize">{engine}</span>
                    </label>
                  ))}
                </div>
                <p className="mt-2 text-[11px] leading-4 text-muted-foreground">Select one engine to filter. Selecting both searches all engines.</p>
              </div>
              <label className="block text-xs font-medium text-muted-foreground">
                Sort
                <select value={sort} onChange={(event) => setSort(event.target.value as SearchSort)} className="mt-1 h-8 w-full rounded-lg border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50">
                  <option value="relevance">Relevance</option>
                  <option value="newestFirst">Newest first</option>
                  <option value="oldestFirst">Oldest first</option>
                </select>
              </label>
              <Button type="button" variant="outline" className="w-full" onClick={() => { setDateFrom(""); setDateTo(""); setSelectedEngines([]); setSort("relevance"); }}>
                Clear filters
              </Button>
            </div>
          </section>

          <section className="rounded-xl border border-border bg-card/60 p-4">
            <button type="button" onClick={() => setSavedOpen((value) => !value)} className="mb-3 flex w-full items-center gap-2 text-left">
              {savedOpen ? <ChevronDown className="size-4 text-muted-foreground" /> : <ChevronRight className="size-4 text-muted-foreground" />}
              <Bookmark className="size-4 text-muted-foreground" />
              <h2 className="text-sm font-medium">Saved searches</h2>
            </button>
            {savedOpen && (savedSearches.length === 0 ? (
              <p className="text-xs leading-5 text-muted-foreground">Saved searches will appear here for quick reuse.</p>
            ) : (
              <div className="space-y-2">
                {savedSearches.map((saved) => (
                  <button key={saved.id} type="button" onClick={() => applySavedSearch(saved)} className="w-full rounded-lg border border-border bg-background/35 px-3 py-2 text-left transition-colors hover:bg-secondary/45">
                    <div className="truncate text-sm font-medium">{saved.name}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{formatEpoch(saved.created_at)}</div>
                  </button>
                ))}
              </div>
            ))}
          </section>

          <section className="rounded-xl border border-border bg-card/60 p-4">
            <div className="mb-3 flex items-center gap-2">
              <DatabaseZap className="size-4 text-muted-foreground" />
              <h2 className="text-sm font-medium">Index maintenance</h2>
            </div>
            <p className="text-xs leading-5 text-muted-foreground">Rebuild is safe anytime and re-syncs FTS5 from stored page text.</p>
            <Button type="button" variant="outline" className="mt-3 w-full" onClick={() => void rebuildIndex()} disabled={rebuilding}>
              {rebuilding ? <Loader2 className="size-4 animate-spin" /> : null}
              Rebuild index
            </Button>
            {rebuildMessage && <p className="mt-2 text-xs leading-5 text-muted-foreground">{rebuildMessage}</p>}
          </section>
        </aside>
      </div>
    </div>
  );
}

function HitRow({ hit, selected, onOpen }: { hit: SearchHit; selected: boolean; onOpen: () => void }) {
  return (
    <button type="button" onClick={onOpen} className={cn("flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-secondary/35", selected && "bg-secondary/45")}>
      <span className="mt-0.5 rounded-md border border-border bg-background px-2 py-1 font-mono text-[11px] text-muted-foreground">p.{hit.page_number}</span>
      <span className="min-w-0 flex-1 text-sm leading-6 text-foreground/85 [&_mark]:rounded [&_mark]:bg-amber-400/25 [&_mark]:px-0.5 [&_mark]:text-amber-100" dangerouslySetInnerHTML={{ __html: sanitizeSnippetHtml(hit.snippet_html) }} />
    </button>
  );
}

function EmptySearchState({ title, description }: { title: string; description: string }) {
  return (
    <div className="grid place-items-center px-4 py-16 text-center">
      <div className="max-w-md rounded-xl border border-border bg-background/35 p-6">
        <div className="mx-auto grid size-10 place-items-center rounded-lg border border-border bg-secondary/50 text-muted-foreground">
          <Search className="size-4" />
        </div>
        <h2 className="mt-4 text-base font-semibold tracking-[-0.04em]">{title}</h2>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{description}</p>
      </div>
    </div>
  );
}

function buildSearchQuery(q: string, dateFrom: string, dateTo: string, engines: OcrEngine[], sort: SearchSort): SearchQuery {
  return {
    q,
    limit: 100,
    offset: 0,
    dateFrom: dateToEpoch(dateFrom, "start"),
    dateTo: dateToEpoch(dateTo, "end"),
    engine: engines.length === 1 ? engines[0] : null,
    sort,
  };
}

function groupHits(hits: SearchHit[]): SearchGroup[] {
  const groups = new Map<number, SearchGroup>();
  for (const hit of hits) {
    const existing = groups.get(hit.document_id);
    if (existing) {
      existing.hits.push(hit);
    } else {
      groups.set(hit.document_id, {
        document_id: hit.document_id,
        display_name: hit.display_name,
        document_ingested_at: hit.document_ingested_at,
        ocr_engine: hit.ocr_engine,
        hits: [hit],
      });
    }
  }
  return Array.from(groups.values());
}

function openHit(hit: SearchHit) {
  const params = new URLSearchParams({ doc: String(hit.document_id), page: String(hit.page_number) });
  window.location.assign(`/library?${params.toString()}`);
}

function dateToEpoch(value: string, boundary: "start" | "end") {
  if (!value) return null;
  const suffix = boundary === "start" ? "T00:00:00" : "T23:59:59";
  return Math.floor(new Date(`${value}${suffix}`).getTime() / 1000);
}

function formatDateInput(value?: number | null) {
  if (!value) return "";
  return new Date(value * 1000).toISOString().slice(0, 10);
}

function formatEpoch(value: number) {
  return format(new Date(value * 1000), "MMM d, yyyy");
}

function isOcrEngine(value: unknown): value is OcrEngine {
  return value === "tesseract" || value === "rapidocr";
}

function normalizeSort(value: unknown): SearchSort {
  return value === "newestFirst" || value === "oldestFirst" ? value : "relevance";
}

function sanitizeSnippetHtml(html: string) {
  const template = document.createElement("template");
  template.innerHTML = html;
  return Array.from(template.content.childNodes).map(renderSafeNode).join("");
}

function renderSafeNode(node: ChildNode): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return escapeHtml(node.textContent ?? "");
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    return "";
  }
  const element = node as Element;
  const children = Array.from(element.childNodes).map(renderSafeNode).join("");
  return element.tagName.toLowerCase() === "mark" ? `<mark>${children}</mark>` : children;
}

function escapeHtml(value: string) {
  return value.split("&").join("&amp;").split("<").join("&lt;").split(">").join("&gt;").split('"').join("&quot;").split("'").join("&#39;");
}
