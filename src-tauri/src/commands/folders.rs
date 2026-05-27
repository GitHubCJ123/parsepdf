use tauri::State;

use crate::{
    state::AppState,
    watcher::{FolderConfig, WatcherService},
};

#[tauri::command]
pub async fn watcher_add_folder(
    state: State<'_, AppState>,
    watcher: State<'_, WatcherService>,
    path: String,
    recursive: bool,
) -> Result<FolderConfig, String> {
    watcher
        .add_folder(&state.db_path, path, recursive)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn watcher_remove_folder(
    state: State<'_, AppState>,
    watcher: State<'_, WatcherService>,
    path: String,
) -> Result<(), String> {
    watcher
        .remove_folder(&state.db_path, path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn watcher_list_folders(
    state: State<'_, AppState>,
    watcher: State<'_, WatcherService>,
) -> Result<Vec<FolderConfig>, String> {
    watcher
        .list_folders(&state.db_path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn watcher_set_enabled(
    state: State<'_, AppState>,
    watcher: State<'_, WatcherService>,
    path: String,
    enabled: bool,
) -> Result<(), String> {
    watcher
        .set_enabled(&state.db_path, path, enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn watcher_scan_now(
    state: State<'_, AppState>,
    watcher: State<'_, WatcherService>,
    path: String,
) -> Result<u32, String> {
    watcher
        .scan_now(&state.db_path, path)
        .await
        .map_err(|error| error.to_string())
}
