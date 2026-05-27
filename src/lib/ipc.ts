import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type JobProgressEvent = {
  type: "job.progress";
  job_id: number;
  document_id: number;
  filename: string;
  stage: string;
  progress_pct: number;
  message: string;
  page_number?: number | null;
  page_count?: number;
};

export type PageRecord = {
  id: number;
  page_number: number;
  text: string;
  ocr_status: string;
  mean_confidence: number | null;
  width_px: number | null;
  height_px: number | null;
  dpi: number | null;
  rotation: number;
};

export type DocumentRecord = {
  id: number;
  sha256: string;
  original_path: string;
  output_path: string | null;
  display_name: string | null;
  page_count: number;
  ocr_engine: string | null;
  status: string;
  error_message: string | null;
  ingested_at: number;
  updated_at: number;
  pages: PageRecord[];
};

export type JobFailedEvent = {
  type: "job.failed";
  job_id: number;
  document_id: number;
  error: string;
};

export type DocumentUpdatedEvent = {
  type: "document.updated";
  document_id: number;
  status: string;
};

export type WatcherErrorEvent = {
  type: "watcher.error";
  folder: string;
  error: string;
};

export type JobProgressUpdate = {
  job_id: number;
  document_id: number;
  filename: string;
  stage: string;
  progress_pct: number;
  message: string;
  page_number?: number | null;
  page_count: number;
  eta_seconds?: number | null;
};

export type JobProgressBatchEvent = {
  type: "job.progress.batch";
  updates: JobProgressUpdate[];
  ts: number;
};

export type JobLifecycleEvent = {
  type: "job.lifecycle";
  job_id: number;
  document_id?: number | null;
  status: JobStatus;
  message?: string | null;
  ts: number;
};

export type JobStatus = "queued" | "running" | "paused" | "done" | "error" | "cancelled";
export type JobKind = "ingest" | "reocr" | "rename" | "index" | "embed";
export type JobOrigin = "manual" | "watch";

export type JobSummary = {
  id: number;
  document_id?: number | null;
  filename: string;
  original_path?: string | null;
  source: JobOrigin;
  kind: JobKind;
  status: JobStatus;
  stage: string;
  progress_pct: number;
  error_message?: string | null;
  created_at: number;
  started_at?: number | null;
  finished_at?: number | null;
  page_count: number;
  engine?: string | null;
};

export type JobFilter = {
  status?: JobStatus[] | null;
  kind?: JobKind | null;
  since?: number | null;
  limit?: number;
};

export type FolderConfig = {
  path: string;
  enabled: boolean;
  recursive: boolean;
  file_count: number;
  last_error?: string | null;
};

export type DocumentNamingReadyEvent = {
  type: "document.naming_ready";
  document_id: number;
  proposed_name: string;
};

export type NamingProposal = {
  display_name: string;
  summary: string;
  provider: string;
  model: string;
  tokens_used?: number | null;
};

export type DocumentRow = {
  id: number;
  display_name: string;
  original_name: string;
  original_path: string;
  output_path: string | null;
  page_count: number;
  ingested_at: number;
  updated_at: number;
  ocr_engine: string | null;
  ai_provider: string | null;
  ai_summary: string | null;
  status: string;
  size_bytes?: number | null;
};

export type DocumentPagePreview = {
  page_number: number;
  text: string;
  ocr_status: string;
  mean_confidence?: number | null;
};

export type DocumentDetail = {
  document: DocumentRow;
  pages: DocumentPagePreview[];
};

export type PendingRenameRow = {
  document_id: number;
  original_name: string;
  current_name: string;
  output_path: string | null;
  proposed_name: string;
  summary?: string | null;
  provider: string;
  created_at: number;
  reviewed: number;
};

export type SearchSort = "relevance" | "newestFirst" | "oldestFirst";

export type SearchQuery = {
  q: string;
  limit?: number;
  offset?: number;
  dateFrom?: number | null;
  dateTo?: number | null;
  engine?: "tesseract" | "rapidocr" | null;
  sort?: SearchSort;
};

export type SearchHit = {
  document_id: number;
  display_name: string;
  page_number: number;
  page_id: number;
  snippet_html: string;
  bm25_score: number;
  document_ingested_at: number;
  ocr_engine: string | null;
};

export type SearchResult = {
  hits: SearchHit[];
  total_hits: number;
  took_ms: number;
  query_warnings: string[];
};

export type RebuildReport = {
  documents: number;
  pages: number;
  took_ms: number;
};

export type ChatCitation = {
  index: number;
  chunk_id: number;
  page_id: number;
  document_id: number;
  page_number: number;
  document_name: string;
  excerpt: string;
};

export type ChatThread = {
  id: number;
  title: string;
  created_at: number;
  updated_at: number;
  preview: string | null;
};

export type ChatMessage = {
  id: number;
  thread_id: number;
  role: "user" | "assistant" | "system";
  content: string;
  citations: ChatCitation[];
  provider: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
  retrieval_ms: number | null;
  generation_ms: number | null;
  created_at: number;
};

export type ChatThreadDetail = {
  thread: ChatThread;
  messages: ChatMessage[];
};

export type ChatStatus = {
  documents: number;
  chunks: number;
  embeddingState: string;
  embeddingError: string | null;
  activeProvider: string | null;
};

export type DocFilter = {
  documentIds?: number[] | null;
  dateFrom?: number | null;
  dateTo?: number | null;
};

export type ChatMessageStartEvent = {
  id: number;
  thread_id: number;
};

export type ChatMessageTokenEvent = {
  id: number;
  delta: string;
};

export type ChatMessageEndEvent = {
  id: number;
  thread_id: number;
  content: string;
  citations: ChatCitation[];
  retrieval_ms: number;
  generation_ms: number;
};

export type EngineInstallProgressEvent = {
  engine_id: string;
  phase: string;
  bytes_done: number;
  bytes_total: number;
  current_file?: string | null;
};

export type EngineInfo = {
  id: string;
  name: string;
  description: string;
  status: "installed" | "available" | "installing" | "error";
  size_mb: number;
  is_default: boolean;
  error?: string | null;
};

export type AppEvent =
  | JobProgressEvent
  | JobProgressBatchEvent
  | JobLifecycleEvent
  | JobFailedEvent
  | DocumentUpdatedEvent
  | DocumentNamingReadyEvent
  | WatcherErrorEvent;

export type DatabaseInfo = {
  path: string;
  url: string;
  sqlite_vec_loaded: boolean;
};

export function initializeDatabase() {
  return invoke<DatabaseInfo>("initialize_database");
}

export function processPdf(inputPath: string, engineId?: string) {
  return invoke<JobSummary>("process_pdf", { inputPath, engineId });
}

export function listOcrEngines() {
  return invoke<EngineInfo[]>("ocr_list_engines");
}

export function installOcrEngine(engineId: string) {
  return invoke<void>("ocr_install_engine", { engineId });
}

export function removeOcrEngine(engineId: string) {
  return invoke<void>("ocr_remove_engine", { engineId });
}

export function setDefaultOcrEngine(engineId: string) {
  return invoke<void>("ocr_set_default", { engineId });
}

export function listenEngineInstallProgress(
  handler: (payload: EngineInstallProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<EngineInstallProgressEvent>("engine.install.progress", (event) => {
    handler(event.payload);
  });
}

export function listenAppEvent<TType extends AppEvent["type"]>(
  type: TType,
  handler: (payload: Extract<AppEvent, { type: TType }>) => void,
): Promise<UnlistenFn> {
  return listen<Extract<AppEvent, { type: TType }>>(type, (event) => {
    handler(event.payload);
  });
}

export function aiHealthCheck(provider: string) {
  return invoke<boolean>("ai_health_check", { provider });
}

export function aiListModels(provider: string) {
  return invoke<string[]>("ai_list_models", { provider });
}

export function aiProposeNames(documentIds: number[]) {
  return invoke<NamingProposal[]>("ai_propose_names", { documentIds });
}

export function aiApplyRename(documentId: number, newName: string) {
  return invoke<void>("ai_apply_rename", { documentId, newName });
}

export function secretsSet(key: string, value: string) {
  return invoke<void>("secrets_set", { key, value });
}

export function secretsGet(key: string) {
  return invoke<string | null>("secrets_get", { key });
}

export function secretsDelete(key: string) {
  return invoke<void>("secrets_delete", { key });
}

export function libraryList(query?: string, limit = 200, offset = 0) {
  return invoke<DocumentRow[]>("library_list", { query: query || null, limit, offset });
}

export function libraryGet(documentId: number) {
  return invoke<DocumentDetail>("library_get", { documentId });
}

export function libraryDelete(documentId: number, force = false) {
  return invoke<void>("library_delete", { documentId, force });
}

export function libraryPendingRenames() {
  return invoke<PendingRenameRow[]>("library_pending_renames");
}

export function librarySkipRename(documentId: number) {
  return invoke<void>("library_skip_rename", { documentId });
}

export function libraryOpenExternal(documentId: number) {
  return invoke<void>("library_open_external", { documentId });
}

export function searchDocuments(query: SearchQuery) {
  return invoke<SearchResult>("search", { query });
}

export function searchRebuildIndex() {
  return invoke<RebuildReport>("search_rebuild_index");
}

export function jobsList(filter: JobFilter = { limit: 500 }) {
  return invoke<JobSummary[]>("jobs_list", { filter });
}

export function jobsCancel(jobId: number) {
  return invoke<void>("jobs_cancel", { jobId });
}

export function jobsCancelAll() {
  return invoke<number>("jobs_cancel_all");
}

export function jobsPauseAll() {
  return invoke<number>("jobs_pause_all");
}

export function jobsResumeAll() {
  return invoke<number>("jobs_resume_all");
}

export function jobsRetry(jobId: number) {
  return invoke<void>("jobs_retry", { jobId });
}

export function jobsClearCompleted() {
  return invoke<number>("jobs_clear_completed");
}

export function watcherAddFolder(path: string, recursive: boolean) {
  return invoke<FolderConfig>("watcher_add_folder", { path, recursive });
}

export function watcherRemoveFolder(path: string) {
  return invoke<void>("watcher_remove_folder", { path });
}

export function watcherListFolders() {
  return invoke<FolderConfig[]>("watcher_list_folders");
}

export function watcherSetEnabled(path: string, enabled: boolean) {
  return invoke<void>("watcher_set_enabled", { path, enabled });
}

export function watcherScanNow(path: string) {
  return invoke<number>("watcher_scan_now", { path });
}

export function chatStatus() {
  return invoke<ChatStatus>("chat_status");
}

export function chatListThreads() {
  return invoke<ChatThread[]>("chat_list_threads");
}

export function chatGetThread(threadId: number) {
  return invoke<ChatThreadDetail>("chat_get_thread", { threadId });
}

export function chatSend(threadId: number | null, message: string, providerId: string, docFilter?: DocFilter | null) {
  return invoke<number>("chat_send", { threadId, message, providerId, docFilter: docFilter ?? null });
}

export function listenChatMessageStart(handler: (payload: ChatMessageStartEvent) => void): Promise<UnlistenFn> {
  return listen<ChatMessageStartEvent>("chat.message.start", (event) => handler(event.payload));
}

export function listenChatMessageToken(handler: (payload: ChatMessageTokenEvent) => void): Promise<UnlistenFn> {
  return listen<ChatMessageTokenEvent>("chat.message.token", (event) => handler(event.payload));
}

export function listenChatMessageEnd(handler: (payload: ChatMessageEndEvent) => void): Promise<UnlistenFn> {
  return listen<ChatMessageEndEvent>("chat.message.end", (event) => handler(event.payload));
}
