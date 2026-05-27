use crate::db;

#[tauri::command]
pub fn initialize_database() -> Result<db::AppDatabase, String> {
    db::prepare_database().map_err(|error| error.to_string())
}
