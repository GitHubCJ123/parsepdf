use crate::{
    jobs::{JobManager, JobSummary},
    watcher::{IngestJob, JobOrigin},
};
use tauri::State;
use tracing::info;

#[tauri::command]
pub async fn process_pdf(
    manager: State<'_, JobManager>,
    input_path: String,
    engine_id: Option<String>,
    display_name_override: Option<String>,
) -> Result<JobSummary, String> {
    info!(
        input_path = %input_path,
        engine_id = engine_id.as_deref().unwrap_or("(default)"),
        "[command] process_pdf invoked"
    );
    let result = manager
        .enqueue_ingest(IngestJob {
            source_path: input_path.clone().into(),
            origin: JobOrigin::Manual,
            engine: engine_id,
            display_name: display_name_override,
        })
        .await
        .map_err(|error| error.to_string());
    match &result {
        Ok(summary) => info!(
            job_id = summary.id,
            document_id = ?summary.document_id,
            input_path = %input_path,
            "[command] process_pdf returned"
        ),
        Err(error) => info!(
            input_path = %input_path,
            error = %error,
            "[command] process_pdf failed"
        ),
    }
    result
}
