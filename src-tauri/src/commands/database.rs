use crate::db;

#[tauri::command]
pub async fn initialize_database() -> Result<db::AppDatabase, String> {
    tokio::task::spawn_blocking(|| db::prepare_database().map_err(|error| error.to_string()))
        .await
        .map_err(|error| error.to_string())?
}
