use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct ProgressAggregator {
    inner: Arc<Mutex<AggregatorState>>,
    app_handle: AppHandle,
    flush_interval: Duration,
}

#[derive(Default)]
struct AggregatorState {
    updates: HashMap<i64, JobProgressUpdate>,
}

impl ProgressAggregator {
    pub fn new(app_handle: AppHandle) -> Self {
        let aggregator = Self {
            inner: Arc::new(Mutex::new(AggregatorState::default())),
            app_handle,
            flush_interval: Duration::from_millis(250),
        };
        aggregator.spawn_flush_loop();
        aggregator
    }

    pub fn notify_progress(&self, update: JobProgressUpdate) {
        self.inner
            .lock()
            .expect("progress aggregator lock poisoned")
            .updates
            .insert(update.job_id, update);
    }

    pub fn notify_lifecycle(&self, event: JobLifecycle) {
        let _ = self.app_handle.emit("job:lifecycle", event);
    }

    fn spawn_flush_loop(&self) {
        let aggregator = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(aggregator.flush_interval);
            loop {
                interval.tick().await;
                aggregator.flush();
            }
        });
    }

    fn flush(&self) {
        let updates = {
            let mut state = self
                .inner
                .lock()
                .expect("progress aggregator lock poisoned");
            if state.updates.is_empty() {
                return;
            }
            state
                .updates
                .drain()
                .map(|(_, update)| update)
                .collect::<Vec<_>>()
        };
        let _ = self.app_handle.emit(
            "job:progress:batch",
            JobProgressBatch {
                event_type: "job:progress:batch".to_string(),
                updates,
                ts: now_ts(),
            },
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressUpdate {
    pub job_id: i64,
    pub document_id: i64,
    pub filename: String,
    pub stage: String,
    pub progress_pct: f32,
    pub message: String,
    pub page_number: Option<i64>,
    pub page_count: i64,
    pub eta_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressBatch {
    #[serde(rename = "type")]
    pub event_type: String,
    pub updates: Vec<JobProgressUpdate>,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLifecycle {
    #[serde(rename = "type")]
    pub event_type: String,
    pub job_id: i64,
    pub document_id: Option<i64>,
    pub status: String,
    pub message: Option<String>,
    pub ts: i64,
}

impl JobLifecycle {
    pub fn new(
        job_id: i64,
        document_id: Option<i64>,
        status: &str,
        message: Option<String>,
    ) -> Self {
        Self {
            event_type: "job:lifecycle".to_string(),
            job_id,
            document_id,
            status: status.to_string(),
            message,
            ts: now_ts(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AppEventPayload {
    #[serde(rename = "job:progress")]
    JobProgress {
        job_id: i64,
        document_id: i64,
        filename: String,
        stage: String,
        progress_pct: f32,
        message: String,
        page_number: Option<i64>,
        page_count: i64,
    },
    #[serde(rename = "job:progress:batch")]
    JobProgressBatch {
        updates: Vec<JobProgressUpdate>,
        ts: i64,
    },
    #[serde(rename = "job:failed")]
    JobFailed {
        job_id: i64,
        document_id: i64,
        error: String,
    },
    #[serde(rename = "job:lifecycle")]
    JobLifecycle {
        job_id: i64,
        document_id: Option<i64>,
        status: String,
        message: Option<String>,
        ts: i64,
    },
    #[serde(rename = "document:updated")]
    DocumentUpdated { document_id: i64, status: String },
    #[serde(rename = "document:naming_ready")]
    DocumentNamingReady {
        document_id: i64,
        proposed_name: String,
    },
    #[serde(rename = "watcher:error")]
    WatcherError { folder: String, error: String },
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
