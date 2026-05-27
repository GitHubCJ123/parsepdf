use tauri::State;

use crate::{
    search::{SearchQuery, SearchResult},
    state::AppState,
};

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: SearchQuery,
) -> Result<SearchResult, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || crate::search::search_db(&db_path, query))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
