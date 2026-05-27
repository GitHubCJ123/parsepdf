import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type JobProgressEvent = {
  type: "job.progress";
  job_id: number;
  document_id: number;
  stage: string;
  progress_pct: number;
  message: string;
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

export function listenAppEvent<TType extends AppEvent["type"]>(
  type: TType,
  handler: (payload: Extract<AppEvent, { type: TType }>) => void,
): Promise<UnlistenFn> {
  return listen<Extract<AppEvent, { type: TType }>>(type, (event) => {
    handler(event.payload);
  });
}
