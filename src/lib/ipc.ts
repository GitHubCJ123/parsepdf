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
  return invoke<DocumentRecord>("process_pdf", { inputPath, engineId });
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
