import { formatDistanceToNow } from "date-fns";
import { Brain, Check, ChevronDown, FileText, Files, Loader2, MessageCircle, MessageCircleOff, MessageSquareText, PanelLeftClose, PanelLeftOpen, Plus, Search, Send, Trash2 } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { CitationPill, type CitationSource } from "@/components/citation-pill";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { EmptyState } from "@/components/empty-state";
import { PdfPreviewDrawer } from "@/components/pdf-preview-drawer";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { getSetting, setSetting } from "@/lib/db";
import { aiListModels, chatDeleteThread, chatGetThread, chatListThreads, chatSend, chatStatus, libraryGet, libraryList, listenChatMessageEnd, listenChatMessageStart, listenChatMessageToken, type ChatCitation, type ChatMessage, type ChatStatus, type ChatThread, type DocumentDetail, type DocumentRow } from "@/lib/ipc";
import { cn } from "@/lib/utils";

export function ChatPage() {
  const [threads, setThreads] = useState<ChatThread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [documents, setDocuments] = useState<DocumentRow[]>([]);
  const [status, setStatus] = useState<ChatStatus | null>(null);
  const [input, setInput] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [scopeIds, setScopeIds] = useState<number[]>([]);
  const [thinkingEnabled, setThinkingEnabled] = useState(false);
  const [thinkingById, setThinkingById] = useState<Record<number, string>>({});
  const [sending, setSending] = useState(false);
  const [searchingId, setSearchingId] = useState<number | null>(null);
  const [streamingId, setStreamingId] = useState<number | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [preview, setPreview] = useState<CitationSource | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ChatThread | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Keep the provider id (with model override) fresh for the streaming event
  // listeners, which capture it in a closure that only runs once.
  const providerIdRef = useRef("ollama");
  providerIdRef.current = selectedModel ? `ollama:${selectedModel}` : "ollama";

  async function refreshShell() {
    const [nextStatus, nextThreads, nextDocuments, modelList, savedModel, savedThinking] = await Promise.all([
      chatStatus().catch(() => null),
      chatListThreads().catch(() => []),
      libraryList(undefined, 500, 0).catch(() => []),
      aiListModels("ollama").catch((): string[] => []),
      getSetting("ollama.model").catch(() => null),
      getSetting("chat.thinking_enabled").catch(() => null),
    ]);
    if (nextStatus) setStatus(nextStatus);
    setThreads(nextThreads);
    setDocuments(nextDocuments);
    setModels(modelList);
    setSelectedModel((current) => current || (savedModel && (modelList.length === 0 || modelList.includes(savedModel)) ? savedModel : modelList[0] ?? savedModel ?? ""));
    setThinkingEnabled(savedThinking === "1");
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

  function startNewThread() {
    setActiveThreadId(null);
    setMessages([]);
    setError(null);
  }

  async function changeModel(model: string) {
    setSelectedModel(model);
    // Persist so the choice is remembered and stays consistent with Settings.
    if (model) await setSetting("ollama.model", model).catch(() => undefined);
  }

  async function changeThinking(enabled: boolean) {
    setThinkingEnabled(enabled);
    await setSetting("chat.thinking_enabled", enabled ? "1" : "0").catch(() => undefined);
  }

  async function confirmDeleteThread(thread: ChatThread) {
    await chatDeleteThread(thread.id);
    if (activeThreadId === thread.id) startNewThread();
    const remaining = await chatListThreads().catch(() => []);
    setThreads(remaining);
  }

  useEffect(() => {
    void refreshShell();
    const unlisteners = [
      listenChatMessageStart((payload) => {
        setActiveThreadId(payload.thread_id);
        setSearchingId(payload.id);
        setStreamingId(null);
        setMessages((current) => upsertMessage(current, assistantPlaceholder(payload.id, payload.thread_id, providerIdRef.current)));
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
        if (payload.thinking) {
          setThinkingById((current) => ({ ...current, [payload.id]: payload.thinking as string }));
        }
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

  useEffect(() => {
    window.addEventListener("pdf-parser:new-chat", startNewThread);
    return () => window.removeEventListener("pdf-parser:new-chat", startNewThread);
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
    const docFilter = scopeIds.length > 0 ? { documentIds: scopeIds } : null;
    try {
      await chatSend(activeThreadId, text, providerIdRef.current, docFilter, thinkingEnabled);
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
    <div className="mx-auto flex h-[calc(100vh-7rem)] max-w-[1600px] overflow-hidden rounded-2xl border border-border bg-card/60 shadow-2xl shadow-black/25">
      <aside className={cn("flex shrink-0 flex-col border-r border-border bg-background/45 transition-all duration-200", sidebarOpen ? "w-72" : "w-12")}>
        <div className="flex h-12 items-center justify-between border-b border-border px-3">
          {sidebarOpen ? <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">Conversations</span> : null}
          <Button type="button" size="icon-sm" variant="ghost" onClick={() => setSidebarOpen((value) => !value)} aria-label={sidebarOpen ? "Collapse sidebar" : "Expand sidebar"}>
            {sidebarOpen ? <PanelLeftClose className="size-4" /> : <PanelLeftOpen className="size-4" />}
          </Button>
        </div>
        {sidebarOpen ? (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="p-2">
              <Button type="button" className="w-full justify-start" onClick={startNewThread}>
                <Plus className="size-4" /> New chat
              </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-auto px-2 pb-2">
              {threads.length === 0 ? (
                <p className="px-2 py-8 text-center text-xs text-muted-foreground">No conversations yet. Ask a question to start one.</p>
              ) : (
                <div className="space-y-1">
                  {threads.map((thread) => (
                    <ThreadRow
                      key={thread.id}
                      thread={thread}
                      active={activeThreadId === thread.id}
                      onOpen={() => void loadThread(thread.id)}
                      onDelete={() => setPendingDelete(thread)}
                    />
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : null}
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-border px-4">
          <div className="flex min-w-0 items-center gap-2 text-sm">
            <span className="grid size-7 shrink-0 place-items-center rounded-md border border-border bg-secondary/50"><MessageSquareText className="size-4" /></span>
            <div className="min-w-0">
              <div className="truncate font-medium tracking-[-0.03em]">Document chat</div>
              <div className="truncate text-xs text-muted-foreground">{statusLine(status, searchingId, streamingId)}</div>
            </div>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-auto px-4 py-5">
          {messages.length === 0 ? (
            <ChatEmptyState onPick={setInput} />
          ) : (
            <div className="mx-auto flex max-w-3xl flex-col gap-4">
              {messages.map((message) => (
                <MessageBubble key={message.id} message={message} thinking={thinkingById[message.id]} active={streamingId === message.id} searching={searchingId === message.id} onOpenCitation={setPreview} />
              ))}
            </div>
          )}
        </div>

        {error ? <div className="border-t border-destructive/20 bg-destructive/10 px-4 py-2 text-sm text-destructive">{error}</div> : null}
        <footer className="shrink-0 border-t border-border bg-background/65 p-4">
          <div className="mx-auto max-w-3xl space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <ModelPicker models={models} value={selectedModel} onChange={(model) => void changeModel(model)} />
              <ScopePicker documents={documents} selectedIds={scopeIds} onChange={setScopeIds} />
              <ThinkingToggle enabled={thinkingEnabled} onChange={(value) => void changeThinking(value)} />
              <span className="text-xs text-muted-foreground">{scopeIds.length > 0 ? "Answering from selected documents" : "Answering from your whole library"}</span>
            </div>
            <div className="rounded-xl border border-border bg-card/85 p-2 shadow-xl shadow-black/20">
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
              <div className="mt-2 flex items-center justify-between gap-2 border-t border-border/70 pt-2">
                <span className="px-1 text-[11px] text-muted-foreground">Enter to send · Shift+Enter for a new line</span>
                <Button type="button" onClick={() => void handleSend()} disabled={sendDisabled}>
                  {sending ? <Loader2 className="size-4 animate-spin" /> : <Send className="size-4" />} Send
                </Button>
              </div>
            </div>
          </div>
        </footer>
      </section>

      {activeCitedMessage ? <SourceRail message={activeCitedMessage} onOpen={setPreview} /> : null}
      {preview ? <CitationPreviewDrawer citation={preview} onClose={() => setPreview(null)} /> : null}
      <ConfirmDialog
        open={pendingDelete != null}
        onOpenChange={(open) => { if (!open) setPendingDelete(null); }}
        destructive
        title="Delete conversation"
        description={pendingDelete ? `"${pendingDelete.title}" and all of its messages will be permanently removed.` : ""}
        confirmLabel="Delete"
        onConfirm={async () => { if (pendingDelete) await confirmDeleteThread(pendingDelete); }}
      />
    </div>
  );
}

function NoDocuments() {
  return (
    <EmptyState
      icon={MessageCircleOff}
      title="Add documents first"
      description="Process a few PDFs to enable chat over your library."
      actionLabel="Go to Inbox"
      onAction={() => window.location.assign("/inbox")}
      className="min-h-[calc(100vh-7rem)]"
    />
  );
}

function ChatEmptyState({ onPick }: { onPick: (value: string) => void }) {
  const suggestions = ["Find the Acme invoice", "Summarize March notes", "Show budget changes"];
  return (
    <EmptyState icon={MessageCircle} title="Ask anything about your library" description="Hybrid search retrieves relevant pages and answers with citations." className="h-full min-h-full">
      <div className="flex flex-wrap justify-center gap-2">
        {suggestions.map((suggestion) => (
          <button
            key={suggestion}
            type="button"
            onClick={() => onPick(suggestion)}
            className="rounded-full border border-border bg-background/45 px-3 py-1 text-xs text-muted-foreground transition-colors hover:border-foreground/30 hover:text-foreground"
          >
            {suggestion}
          </button>
        ))}
      </div>
    </EmptyState>
  );
}

function ThreadRow({ thread, active, onOpen, onDelete }: { thread: ChatThread; active: boolean; onOpen: () => void; onDelete: () => void }) {
  return (
    <div
      className={cn(
        "group relative flex items-center rounded-lg border transition-colors",
        active ? "border-foreground/20 bg-secondary/70" : "border-transparent hover:bg-secondary/45",
      )}
    >
      <button type="button" onClick={onOpen} className="min-w-0 flex-1 px-3 py-2 text-left">
        <div className="truncate text-sm font-medium tracking-[-0.03em]">{thread.title}</div>
        <div className="mt-1 truncate text-xs text-muted-foreground">{thread.preview ?? relativeTime(thread.updated_at)}</div>
      </button>
      <button
        type="button"
        onClick={(event) => { event.stopPropagation(); onDelete(); }}
        aria-label="Delete conversation"
        className="mr-1 grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/15 hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Trash2 className="size-4" />
      </button>
    </div>
  );
}

function ModelPicker({ models, value, onChange }: { models: string[]; value: string; onChange: (model: string) => void }) {
  const options = value && !models.includes(value) ? [value, ...models] : models;
  if (options.length === 0) {
    return (
      <div className="flex items-center gap-1.5 rounded-md border border-border bg-background/60 px-2.5 py-1.5 text-xs text-muted-foreground" title="Start Ollama to choose a model">
        <span className="size-1.5 rounded-full bg-amber-400" />
        {value || "Ollama offline"}
      </div>
    );
  }
  return (
    <div className="relative flex h-8 items-center gap-1.5 rounded-md border border-border bg-background/60 pl-2.5 focus-within:border-ring">
      <span className="pointer-events-none text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Model</span>
      <select
        value={value || options[0]}
        onChange={(event) => onChange(event.target.value)}
        className="h-full appearance-none rounded-md border-0 bg-transparent py-0 pr-7 pl-0 text-xs font-medium outline-none"
      >
        {options.map((model) => <option key={model} value={model}>{model}</option>)}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2 size-3.5 text-muted-foreground" />
    </div>
  );
}

function ScopePicker({ documents, selectedIds, onChange }: { documents: DocumentRow[]; selectedIds: number[]; onChange: (ids: number[]) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const allSelected = selectedIds.length === 0;
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return documents;
    return documents.filter((document) => document.display_name.toLowerCase().includes(needle));
  }, [documents, query]);

  function toggle(id: number) {
    onChange(selectedIds.includes(id) ? selectedIds.filter((value) => value !== id) : [...selectedIds, id]);
  }

  const label = allSelected ? "All documents" : `${selectedIds.length} selected`;

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex h-8 items-center gap-2 rounded-md border border-border bg-background/60 px-2.5 text-xs font-medium outline-none transition-colors hover:border-foreground/30 focus:border-ring"
      >
        <Files className="size-3.5 text-muted-foreground" />
        {label}
        <ChevronDown className="size-3.5 text-muted-foreground" />
      </button>
      {open ? (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} aria-hidden />
          <div className="absolute bottom-full left-0 z-50 mb-2 w-80 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl shadow-black/40">
            <div className="border-b border-border p-2">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-muted-foreground" />
                <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Filter documents" className="h-8 pl-8 text-xs" />
              </div>
            </div>
            <button
              type="button"
              onClick={() => { onChange([]); }}
              className="flex w-full items-center justify-between gap-2 border-b border-border px-3 py-2 text-left text-sm transition-colors hover:bg-secondary/50"
            >
              <span className="font-medium">All documents</span>
              {allSelected ? <Check className="size-4 text-emerald-300" /> : null}
            </button>
            <div className="max-h-64 overflow-auto p-1">
              {filtered.length === 0 ? (
                <p className="px-3 py-6 text-center text-xs text-muted-foreground">No documents match.</p>
              ) : (
                filtered.map((document) => {
                  const checked = selectedIds.includes(document.id);
                  return (
                    <button
                      key={document.id}
                      type="button"
                      onClick={() => toggle(document.id)}
                      className="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm transition-colors hover:bg-secondary/50"
                    >
                      <span className={cn("grid size-4 shrink-0 place-items-center rounded border", checked ? "border-foreground bg-foreground text-background" : "border-border")}>
                        {checked ? <Check className="size-3" /> : null}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate">{document.display_name}</span>
                      </span>
                    </button>
                  );
                })
              )}
            </div>
            {selectedIds.length > 0 ? (
              <div className="flex items-center justify-between border-t border-border px-3 py-2 text-xs">
                <span className="text-muted-foreground">{selectedIds.length} selected</span>
                <button type="button" onClick={() => onChange([])} className="font-medium text-foreground hover:underline">Clear</button>
              </div>
            ) : null}
          </div>
        </>
      ) : null}
    </div>
  );
}

function MessageBubble({ message, thinking, active, searching, onOpenCitation }: { message: ChatMessage; thinking?: string; active: boolean; searching: boolean; onOpenCitation: (citation: CitationSource) => void }) {
  const isUser = message.role === "user";
  return (
    <article className={cn("flex", isUser ? "justify-end" : "justify-start")} aria-live={!isUser && active ? "polite" : undefined} aria-atomic={false}>
      <div className={cn("max-w-[86%] rounded-xl border px-3 py-2", isUser ? "border-foreground/10 bg-foreground text-background" : "border-border bg-background/75 text-foreground")}>
        {searching ? <PhasePill label="Searching library…" /> : null}
        {!isUser && thinking ? <ReasoningBlock thinking={thinking} /> : null}
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

function ReasoningBlock({ thinking }: { thinking: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-2 overflow-hidden rounded-lg border border-border/70 bg-secondary/40">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs font-medium text-muted-foreground transition-colors hover:text-foreground"
      >
        <Brain className="size-3.5" />
        <span>Reasoning</span>
        <ChevronDown className={cn("ml-auto size-3.5 transition-transform", open && "rotate-180")} />
      </button>
      {open ? (
        <div className="border-t border-border/60 px-3 py-2 text-xs leading-5 whitespace-pre-wrap text-muted-foreground">
          {thinking}
        </div>
      ) : null}
    </div>
  );
}

function ThinkingToggle({ enabled, onChange }: { enabled: boolean; onChange: (enabled: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={enabled}
      onClick={() => onChange(!enabled)}
      title="Ask reasoning models to think step by step before answering"
      className={cn(
        "flex h-8 items-center gap-2 rounded-md border px-2.5 text-xs font-medium outline-none transition-colors focus:border-ring",
        enabled ? "border-foreground/30 bg-secondary text-foreground" : "border-border bg-background/60 text-muted-foreground hover:border-foreground/30",
      )}
    >
      <Brain className="size-3.5" />
      Thinking
      <span className={cn("ml-0.5 h-2 w-2 rounded-full", enabled ? "bg-emerald-400" : "bg-muted-foreground/40")} />
    </button>
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
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void libraryGet(citation.document_id)
      .then((next) => { if (!cancelled) setDetail(next); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [citation.document_id]);

  return (
    <PdfPreviewDrawer
      detail={detail}
      loading={loading}
      initialPage={citation.page_number}
      onClose={onClose}
      eyebrow={`Citation [${citation.index}]`}
      citation={(
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-emerald-100/80">Highlighted source text</p>
          <mark className="mt-2 block rounded-md border border-emerald-300/20 bg-emerald-300/10 p-3 text-sm leading-6 text-foreground">{citation.excerpt}</mark>
        </div>
      )}
    />
  );
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

function relativeTime(value: number) {
  return formatDistanceToNow(new Date(value * 1000), { addSuffix: true });
}
