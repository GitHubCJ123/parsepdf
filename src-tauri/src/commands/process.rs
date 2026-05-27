use crate::{
    jobs::{JobManager, JobSummary},
    watcher::{IngestJob, JobOrigin},
};
use tauri::State;

#[tauri::command]
pub async fn process_pdf(
    manager: State<'_, JobManager>,
    input_path: String,
    engine_id: Option<String>,
) -> Result<JobSummary, String> {
    manager
        .enqueue_ingest(IngestJob {
            source_path: input_path.into(),
            origin: JobOrigin::Manual,
            engine: engine_id,
        })
        .await
        .map_err(|error| error.to_string())
}
