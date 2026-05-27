#[tauri::command]
pub fn prepare_for_update() -> Result<(), String> {
    // TODO(Phase 1/4 W11): signal the OCR worker pool to stop accepting work and let
    // in-flight jobs settle to cancelled/done before updater installation starts.
    Ok(())
}
