use std::{fs, path::PathBuf};

use chrono::Local;
use serde::Serialize;

use crate::{db, logging};

#[derive(Debug, Serialize)]
pub struct AppPaths {
    pub data_dir: String,
    pub log_dir: String,
    pub current_log: String,
}

#[tauri::command]
pub fn app_paths() -> Result<AppPaths, String> {
    let data_dir = db::app_data_dir().map_err(|error| error.to_string())?;
    let log_dir = db::log_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
    let current_log = logging::current_log_path(&log_dir);
    Ok(AppPaths {
        data_dir: data_dir.to_string_lossy().into_owned(),
        log_dir: log_dir.to_string_lossy().into_owned(),
        current_log: current_log.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn log_tail(level: Option<String>, max_lines: Option<usize>) -> Result<String, String> {
    let log_dir = db::log_dir().map_err(|error| error.to_string())?;
    let Some(path) = logging::newest_log_path(&log_dir) else {
        return Ok(String::new());
    };
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let level = level
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty() && value != "ALL");
    let lines: Vec<&str> = contents
        .lines()
        .filter(|line| match level.as_deref() {
            Some("INFO") => line.contains(" INFO "),
            Some("WARN") => line.contains(" WARN "),
            Some("ERROR") => line.contains(" ERROR"),
            _ => true,
        })
        .collect();
    let limit = max_lines.unwrap_or(500).clamp(50, 5_000);
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].join("\n"))
}

#[tauri::command]
pub fn log_save_selection(path: String, text: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    fs::write(
        path,
        format!("PDF-Parser log selection saved {stamp}\n\n{text}"),
    )
    .map_err(|error| error.to_string())
}
