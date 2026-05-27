use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AppEventPayload {
    #[serde(rename = "job.progress")]
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
    #[serde(rename = "job.failed")]
    JobFailed {
        job_id: i64,
        document_id: i64,
        error: String,
    },
    #[serde(rename = "document.updated")]
    DocumentUpdated { document_id: i64, status: String },
    #[serde(rename = "document.naming_ready")]
    DocumentNamingReady {
        document_id: i64,
        proposed_name: String,
    },
    #[serde(rename = "watcher.error")]
    WatcherError { folder: String, error: String },
}
