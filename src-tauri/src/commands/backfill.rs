use tauri::State;

use crate::{search::RebuildReport, state::AppState};

/// Rebuilds the FTS5 index from the pages content table and optimizes it.
/// Safe and idempotent; useful after older ingests or index corruption.
#[tauri::command]
pub async fn search_rebuild_index(state: State<'_, AppState>) -> Result<RebuildReport, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || crate::search::rebuild_index(&db_path))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
