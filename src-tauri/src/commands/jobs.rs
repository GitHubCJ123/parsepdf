use tauri::State;

use crate::jobs::{JobFilter, JobManager, JobSummary};

#[tauri::command]
pub async fn jobs_list(
    manager: State<'_, JobManager>,
    filter: JobFilter,
) -> Result<Vec<JobSummary>, String> {
    manager
        .list_filtered(filter)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_cancel(manager: State<'_, JobManager>, job_id: i64) -> Result<(), String> {
    manager
        .cancel(job_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_cancel_all(manager: State<'_, JobManager>) -> Result<u32, String> {
    manager
        .cancel_all()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_pause_all(manager: State<'_, JobManager>) -> Result<u32, String> {
    manager.pause_all().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_resume_all(manager: State<'_, JobManager>) -> Result<u32, String> {
    manager
        .resume_all()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_retry(manager: State<'_, JobManager>, job_id: i64) -> Result<(), String> {
    manager
        .retry(job_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn jobs_clear_completed(manager: State<'_, JobManager>) -> Result<u32, String> {
    manager
        .clear_completed()
        .await
        .map_err(|error| error.to_string())
}
