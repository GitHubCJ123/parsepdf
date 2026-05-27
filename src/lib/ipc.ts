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

export type AppEvent =
  | JobProgressEvent
  | JobFailedEvent
  | DocumentUpdatedEvent
  | WatcherErrorEvent;

export type DatabaseInfo = {
  path: string;
  url: string;
  sqlite_vec_loaded: boolean;
};

export function initializeDatabase() {
  return invoke<DatabaseInfo>("initialize_database");
}

export function processPdf(inputPath: string) {
  return invoke<DocumentRecord>("process_pdf", { inputPath });
}

export function listenAppEvent<TType extends AppEvent["type"]>(
  type: TType,
  handler: (payload: Extract<AppEvent, { type: TType }>) => void,
): Promise<UnlistenFn> {
  return listen<Extract<AppEvent, { type: TType }>>(type, (event) => {
    handler(event.payload);
  });
}
