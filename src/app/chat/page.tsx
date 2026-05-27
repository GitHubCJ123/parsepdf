import { Link } from "@tanstack/react-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import { format } from "date-fns";
import { BookOpen, ChevronLeft, FileText, Loader2, MessageSquareText, PanelLeftClose, PanelLeftOpen, Search, Send, Sparkles, X } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { CitationPill, type CitationSource } from "@/components/citation-pill";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { getSetting } from "@/lib/db";
import { chatGetThread, chatListThreads, chatSend, chatStatus, libraryGet, libraryList, listenChatMessageEnd, listenChatMessageStart, listenChatMessageToken, type ChatCitation, type ChatMessage, type ChatStatus, type ChatThread, type DocumentDetail, type DocumentRow } from "@/lib/ipc";
import { cn } from "@/lib/utils";

const PROVIDERS = ["ollama", "openrouter"];

export function ChatPage() {
  const [threads, setThreads] = useState<ChatThread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [documents, setDocuments] = useState<DocumentRow[]>([]);
  const [status, setStatus] = useState<ChatStatus | null>(null);
  const [input, setInput] = useState("");
  const [provider, setProvider] = useState("ollama");
  const [documentFilter, setDocumentFilter] = useState("all");
  const [sending, setSending] = useState(false);
  const [searchingId, setSearchingId] = useState<number | null>(null);
  const [streamingId, setStreamingId] = useState<number | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [preview, setPreview] = useState<CitationSource | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function refreshShell() {
    const [nextStatus, nextThreads, nextDocuments, configuredProvider] = await Promise.all([
      chatStatus().catch(() => null),
      chatListThreads().catch(() => []),
      libraryList(undefined, 200, 0).catch(() => []),
      getSetting("ai.default_provider").catch(() => null),
    ]);
    if (nextStatus) setStatus(nextStatus);
    setThreads(nextThreads);
    setDocuments(nextDocuments);
    const nextProvider = configuredProvider && configuredProvider !== "none" ? configuredProvider : nextStatus?.activeProvider ?? provider;
    if (PROVIDERS.includes(nextProvider)) setProvider(nextProvider);
    if (activeThreadId == null && nextThreads.length > 0) {
      await loadThread(nextThreads[0].id);
    }
  }

  async function loadThread(threadId: number) {
    const detail = await chatGetThread(threadId);
    setActiveThreadId(detail.thread.id);
    setMessages(detail.messages);
    setError(null);
  }

  useEffect(() => {
    void refreshShell();
    const unlisteners = [
      listenChatMessageStart((payload) => {
        setActiveThreadId(payload.thread_id);
        setSearchingId(payload.id);
        setStreamingId(null);
        setMessages((current) => upsertMessage(current, assistantPlaceholder(payload.id, payload.thread_id, provider)));
      }),
      listenChatMessageToken((payload) => {
        setSearchingId((current) => (current === payload.id ? null : current));
        setStreamingId(payload.id);
        setMessages((current) =>
          current.map((message) =>
            message.id === payload.id ? { ...message, content: `${message.content}${payload.delta}` } : message,
          ),
        );
      }),
      listenChatMessageEnd((payload) => {
        setSearchingId(null);
        setStreamingId(null);
        setMessages((current) =>
          current.map((message) =>
            message.id === payload.id
              ? {
                  ...message,
                  content: payload.content,
                  citations: payload.citations,
                  retrieval_ms: payload.retrieval_ms,
                  generation_ms: payload.generation_ms,
                }
              : message,
          ),
        );
        void chatListThreads().then(setThreads).catch(() => undefined);
      }),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const activeCitedMessage = useMemo(
    () => [...messages].reverse().find((message) => message.role === "assistant" && message.citations.length > 0) ?? null,
    [messages],
  );

  const sendDisabled = sending || !input.trim() || !status || status.documents === 0 || status.embeddingState === "error";

  async function handleSend() {
    const text = input.trim();
    if (!text || sendDisabled) return;
    setInput("");
    setSending(true);
    setError(null);
    const tempId = -Date.now();
    setMessages((current) => [
      ...current,
      {
        id: tempId,
        thread_id: activeThreadId ?? 0,
        role: "user",
        content: text,
        citations: [],
        provider: null,
        tokens_in: null,
        tokens_out: null,
        retrieval_ms: null,
        generation_ms: null,
        created_at: Math.floor(Date.now() / 1000),
      },
    ]);
    const docFilter = documentFilter === "all" ? null : { documentIds: [Number(documentFilter)] };
    try {
      await chatSend(activeThreadId, text, provider, docFilter);
      await refreshShell();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSending(false);
    }
  }

  if (status?.documents === 0) {
    return <NoDocuments />;
  }

  return (
    <div className="mx-auto flex h-[calc(100vh-7rem)] max-w-[1600px] overflow-hidden rounded-xl border border-border bg-card/60 shadow-2xl shadow-black/25">
      <aside className={cn("flex shrink-0 flex-col border-r border-border bg-background/45 transition-all", sidebarOpen ? "w-72" : "w-12")}>
        <div className="flex h-12 items-center justify-between border-b border-border px-3">
          {sidebarOpen ? <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">Threads</span> : null}
          <Button type="button" size="icon-sm" variant="ghost" onClick={() => setSidebarOpen((value) => !value)} aria-label="Toggle thread sidebar">
            {sidebarOpen ? <PanelLeftClose className="size-4" /> : <PanelLeftOpen className="size-4" />}
          </Button>
        </div>
        {sidebarOpen ? (
          <div className="min-h-0 flex-1 overflow-auto p-2">
            <Button type="button" variant="outline" className="mb-2 w-full justify-start" onClick={() => { setActiveThreadId(null); setMessages([]); }}>
              <Sparkles className="size-4" /> New thread
            </Button>
            {threads.length === 0 ? (
              <p className="px-2 py-8 text-center text-xs text-muted-foreground">Ask a question to start a thread.</p>
            ) : (
              <div className="space-y-1">
                {threads.map((thread) => (
                  <button
                    key={thread.id}
                    type="button"
                    onClick={() => void loadThread(thread.id)}
                    className={cn("w-full rounded-lg border px-3 py-2 text-left transition-colors", activeThreadId === thread.id ? "border-foreground/20 bg-secondary/70" : "border-transparent hover:bg-secondary/45")}
                  >
                    <div className="truncate text-sm font-medium tracking-[-0.03em]">{thread.title}</div>
                    <div className="mt-1 truncate text-xs text-muted-foreground">{thread.preview ?? formatDate(thread.updated_at)}</div>
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : null}
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <div className="flex items-center gap-2 text-sm">
            <span className="grid size-7 place-items-center rounded-md border border-border bg-secondary/50"><MessageSquareText className="size-4" /></span>
            <div>
              <div className="font-medium tracking-[-0.03em]">Document chat</div>
              <div className="text-xs text-muted-foreground">{statusLine(status, searchingId, streamingId)}</div>
            </div>
          </div>
          <div className="hidden items-center gap-2 text-xs text-muted-foreground md:flex">
            <Search className="size-3.5" /> Hybrid FTS + local vectors
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-auto px-4 py-5">
          {messages.length === 0 ? (
            <ChatEmptyState />
          ) : (
            <div className="mx-auto flex max-w-3xl flex-col gap-4">
              {messages.map((message) => (
                <MessageBubble key={message.id} message={message} active={streamingId === message.id} searching={searchingId === message.id} onOpenCitation={setPreview} />
              ))}
            </div>
          )}
        </div>

        {error ? <div className="border-t border-destructive/20 bg-destructive/10 px-4 py-2 text-sm text-destructive">{error}</div> : null}
        <footer className="shrink-0 border-t border-border bg-background/65 p-4">
          <div className="mx-auto max-w-3xl rounded-xl border border-border bg-card/85 p-2 shadow-xl shadow-black/20">
            <Textarea
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void handleSend();
                }
              }}
              placeholder="Ask anything about your library…"
              className="max-h-40 min-h-20 resize-none border-0 bg-transparent shadow-none focus-visible:ring-0"
              disabled={status?.embeddingState === "error"}
            />
            <div className="mt-2 flex flex-wrap items-center justify-between gap-2 border-t border-border/70 pt-2">
              <div className="flex flex-wrap items-center gap-2">
                <SelectLabel label="Provider">
                  <select value={provider} onChange={(event) => setProvider(event.target.value)} className="h-7 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-ring">
                    {PROVIDERS.map((item) => <option key={item} value={item}>{item}</option>)}
                  </select>
                </SelectLabel>
                <SelectLabel label="Filter library">
                  <select value={documentFilter} onChange={(event) => setDocumentFilter(event.target.value)} className="h-7 max-w-56 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-ring">
                    <option value="all">All documents</option>
                    {documents.map((document) => <option key={document.id} value={document.id}>{document.display_name}</option>)}
                  </select>
                </SelectLabel>
              </div>
              <Button type="button" onClick={() => void handleSend()} disabled={sendDisabled}>
                {sending ? <Loader2 className="size-4 animate-spin" /> : <Send className="size-4" />} Send
              </Button>
            </div>
          </div>
        </footer>
      </section>

      {activeCitedMessage ? <SourceRail message={activeCitedMessage} onOpen={setPreview} /> : null}
      {preview ? <CitationPreviewDrawer citation={preview} onClose={() => setPreview(null)} /> : null}
    </div>
  );
}

function NoDocuments() {
  return (
    <section className="grid min-h-[calc(100vh-7rem)] place-items-center">
      <div className="max-w-md rounded-xl border border-border bg-card/75 p-8 text-center shadow-2xl shadow-black/20">
        <div className="mx-auto grid size-11 place-items-center rounded-lg border border-border bg-secondary/50"><BookOpen className="size-5" /></div>
        <h1 className="mt-5 text-lg font-semibold tracking-[-0.04em]">Process some PDFs first</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">Document-aware chat needs OCR text and local embeddings before it can answer with citations.</p>
        <Button asChild className="mt-5"><Link to="/inbox">Open inbox</Link></Button>
      </div>
    </section>
  );
}

function ChatEmptyState() {
  return (
    <div className="mx-auto grid h-full max-w-2xl place-items-center text-center">
      <div>
        <div className="mx-auto grid size-12 place-items-center rounded-xl border border-border bg-secondary/50"><Sparkles className="size-5" /></div>
        <h1 className="mt-5 text-2xl font-semibold tracking-[-0.06em]">Ask anything about your library</h1>
        <p className="mx-auto mt-3 max-w-lg text-sm leading-6 text-muted-foreground">Try: “Find the invoice from Acme”, “Summarize my March meeting notes”, or “What were the budget changes?”</p>
      </div>
    </div>
  );
}

function MessageBubble({ message, active, searching, onOpenCitation }: { message: ChatMessage; active: boolean; searching: boolean; onOpenCitation: (citation: CitationSource) => void }) {
  const isUser = message.role === "user";
  return (
    <article className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div className={cn("max-w-[86%] rounded-xl border px-3 py-2", isUser ? "border-foreground/10 bg-foreground text-background" : "border-border bg-background/75 text-foreground")}>
        {searching ? <PhasePill label="Searching library…" /> : null}
        <div className="whitespace-pre-wrap text-sm leading-6">
          {isUser ? message.content : <AssistantContent content={message.content} citations={message.citations} onOpen={onOpenCitation} />}
          {active ? <span className="ml-1 inline-block size-1.5 animate-pulse rounded-full bg-emerald-200" /> : null}
        </div>
        {!isUser && (message.retrieval_ms != null || message.generation_ms != null) ? (
          <div className="mt-2 flex flex-wrap gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
            {message.retrieval_ms != null ? <span>retrieval {message.retrieval_ms} ms</span> : null}
            {message.generation_ms != null ? <span>generation {message.generation_ms} ms</span> : null}
          </div>
        ) : null}
      </div>
    </article>
  );
}

function AssistantContent({ content, citations, onOpen }: { content: string; citations: ChatCitation[]; onOpen: (citation: CitationSource) => void }) {
  if (!content) return <span className="text-muted-foreground">Waiting for model…</span>;
  const citationMap = new Map(citations.map((citation) => [citation.index, citation]));
  const parts: ReactNode[] = [];
  const regex = /\[(\d+|\?)\]/g;
  let last = 0;
  for (const match of content.matchAll(regex)) {
    const index = match.index ?? 0;
    if (index > last) parts.push(content.slice(last, index));
    const label = match[1];
    const numeric = Number(label);
    const citation = Number.isFinite(numeric) ? citationMap.get(numeric) : null;
    parts.push(citation ? <CitationPill key={`${index}-${label}`} citation={citation} onOpen={onOpen} /> : <sup key={`${index}-${label}`} className="mx-0.5 text-muted-foreground">[?]</sup>);
    last = index + match[0].length;
  }
  if (last < content.length) parts.push(content.slice(last));
  return <>{parts}</>;
}

function SourceRail({ message, onOpen }: { message: ChatMessage; onOpen: (citation: CitationSource) => void }) {
  return (
    <aside className="hidden w-80 shrink-0 flex-col border-l border-border bg-background/45 xl:flex">
      <div className="border-b border-border p-4">
        <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">Sources</p>
        <h2 className="mt-1 text-sm font-medium">{message.citations.length} cited excerpt{message.citations.length === 1 ? "" : "s"}</h2>
      </div>
      <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3">
        {message.citations.map((citation) => (
          <button key={`${citation.index}-${citation.chunk_id}`} type="button" onClick={() => onOpen(citation)} className="w-full rounded-lg border border-border bg-card/70 p-3 text-left transition-colors hover:border-emerald-200/40 hover:bg-secondary/50">
            <div className="flex items-start gap-2">
              <span className="grid size-8 shrink-0 place-items-center rounded-md border border-border bg-background"><FileText className="size-4" /></span>
              <div className="min-w-0">
                <div className="truncate text-sm font-medium">{citation.document_name}</div>
                <div className="mt-0.5 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Citation [{citation.index}] · p.{citation.page_number}</div>
              </div>
            </div>
            <p className="mt-3 line-clamp-5 text-xs leading-5 text-muted-foreground">{citation.excerpt}</p>
          </button>
        ))}
      </div>
    </aside>
  );
}

function CitationPreviewDrawer({ citation, onClose }: { citation: CitationSource; onClose: () => void }) {
  const [detail, setDetail] = useState<DocumentDetail | null>(null);
  useEffect(() => {
    let cancelled = false;
    void libraryGet(citation.document_id).then((next) => { if (!cancelled) setDetail(next); });
    return () => { cancelled = true; };
  }, [citation.document_id]);
  const pdfUrl = detail?.document.output_path ? convertFileSrc(detail.document.output_path) : null;
  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-full max-w-3xl flex-col border-l border-border bg-background/95 shadow-2xl shadow-black/50 backdrop-blur-xl">
      <div className="flex items-start justify-between gap-4 border-b border-border p-4">
        <div className="min-w-0">
          <button type="button" onClick={onClose} className="mb-2 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"><ChevronLeft className="size-3" />Back to chat</button>
          <h2 className="truncate text-xl font-semibold tracking-[-0.04em]">{citation.document_name}</h2>
          <p className="mt-1 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">Citation [{citation.index}] · page {citation.page_number}</p>
        </div>
        <Button type="button" size="icon-sm" variant="ghost" onClick={onClose} aria-label="Close source preview"><X className="size-4" /></Button>
      </div>
      <div className="border-b border-border bg-emerald-300/5 p-4">
        <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-emerald-100/80">Highlighted source text</p>
        <mark className="mt-2 block rounded-md border border-emerald-300/20 bg-emerald-300/10 p-3 text-sm leading-6 text-foreground">{citation.excerpt}</mark>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-black/25 p-4">
        {pdfUrl ? <object data={`${pdfUrl}#page=${citation.page_number}`} type="application/pdf" className="h-[72vh] w-full rounded-lg border border-border bg-background"><embed src={`${pdfUrl}#page=${citation.page_number}`} type="application/pdf" /></object> : <div className="grid h-full place-items-center text-sm text-muted-foreground">Loading preview…</div>}
      </div>
    </div>
  );
}

function SelectLabel({ label, children }: { label: string; children: ReactNode }) {
  return <label className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground"><span>{label}</span>{children}</label>;
}

function PhasePill({ label }: { label: string }) {
  return <div className="mb-2 inline-flex items-center gap-2 rounded-full border border-emerald-300/25 bg-emerald-300/10 px-2 py-1 text-xs text-emerald-100"><Loader2 className="size-3 animate-spin" />{label}</div>;
}

function statusLine(status: ChatStatus | null, searchingId: number | null, streamingId: number | null) {
  if (searchingId != null) return "Searching library…";
  if (streamingId != null) return "Streaming grounded answer…";
  if (!status) return "Loading chat runtime…";
  if (status.embeddingState === "initializing") return "Initializing embeddings…";
  if (status.embeddingState === "error") return status.embeddingError ?? "Embeddings unavailable";
  if (status.chunks === 0) return "Waiting for document embeddings…";
  return `${status.chunks.toLocaleString()} chunks ready`;
}

function upsertMessage(messages: ChatMessage[], message: ChatMessage) {
  return messages.some((current) => current.id === message.id) ? messages.map((current) => current.id === message.id ? message : current) : [...messages, message];
}

function assistantPlaceholder(id: number, threadId: number, provider: string): ChatMessage {
  return { id, thread_id: threadId, role: "assistant", content: "", citations: [], provider, tokens_in: null, tokens_out: null, retrieval_ms: null, generation_ms: null, created_at: Math.floor(Date.now() / 1000) };
}

function formatDate(value: number) {
  return format(new Date(value * 1000), "MMM d, yyyy");
}
