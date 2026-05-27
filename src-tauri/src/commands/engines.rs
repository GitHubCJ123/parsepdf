use std::{collections::HashSet, fs, sync::OnceLock};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::{
    db,
    ocr::{
        rapidocr_install::{
            default_rapidocr_dir, install_rapidocr, verify_install_dir, InstallProgress,
        },
        rapidocr_manifest::RAPIDOCR_V1,
    },
    state::AppState,
};

const DEFAULT_ENGINE_KEY: &str = "ocr.default_engine";
const RAPIDOCR_ENGINE_ID: &str = "rapidocr";
const TESSERACT_ENGINE_ID: &str = "tesseract";

static INSTALLING_ENGINES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub size_mb: u32,
    pub is_default: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineInstallProgressEvent {
    engine_id: String,
    phase: String,
    bytes_done: u64,
    bytes_total: u64,
    current_file: Option<String>,
}

#[tauri::command]
pub async fn ocr_list_engines(state: State<'_, AppState>) -> Result<Vec<EngineInfo>, String> {
    let default_engine = read_default_engine(&state.db_path).map_err(|error| error.to_string())?;
    let installing = installing_engines().lock().await;
    let rapidocr_status = rapidocr_status(installing.contains(RAPIDOCR_ENGINE_ID));

    Ok(vec![
        EngineInfo {
            id: TESSERACT_ENGINE_ID.to_string(),
            name: "Tesseract".to_string(),
            description:
                "Fast bundled OCR for everyday English documents. Offline and installed by default."
                    .to_string(),
            status: "installed".to_string(),
            size_mb: 50,
            is_default: default_engine == TESSERACT_ENGINE_ID,
            error: None,
        },
        EngineInfo {
            id: RAPIDOCR_ENGINE_ID.to_string(),
            name: "RapidOCR PP-OCRv5".to_string(),
            description:
                "High-quality ONNX OCR for scans, tables, multi-column layouts, and CJK text."
                    .to_string(),
            status: rapidocr_status.status,
            size_mb: RAPIDOCR_V1.total_size_mb,
            is_default: default_engine == RAPIDOCR_ENGINE_ID,
            error: rapidocr_status.error,
        },
    ])
}

#[tauri::command]
pub async fn ocr_install_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    if engine_id != RAPIDOCR_ENGINE_ID {
        return Err(format!("unknown OCR engine: {engine_id}"));
    }

    {
        let mut installing = installing_engines().lock().await;
        if !installing.insert(engine_id.clone()) {
            return Err("RapidOCR is already installing".to_string());
        }
    }

    let target_dir = default_rapidocr_dir().map_err(|error| error.to_string())?;
    let app_for_progress = app.clone();
    let progress_engine_id = engine_id.clone();
    let result = install_rapidocr(&target_dir, &RAPIDOCR_V1, move |progress| {
        emit_install_progress(&app_for_progress, &progress_engine_id, progress);
    })
    .await;

    installing_engines().lock().await.remove(&engine_id);
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn ocr_remove_engine(
    state: State<'_, AppState>,
    engine_id: String,
) -> Result<(), String> {
    if engine_id != RAPIDOCR_ENGINE_ID {
        return Err(format!("cannot remove bundled OCR engine: {engine_id}"));
    }
    let target_dir = default_rapidocr_dir().map_err(|error| error.to_string())?;
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|error| error.to_string())?;
    }

    let current_default = read_default_engine(&state.db_path).map_err(|error| error.to_string())?;
    if current_default == RAPIDOCR_ENGINE_ID {
        write_default_engine(&state.db_path, TESSERACT_ENGINE_ID)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn ocr_set_default(state: State<'_, AppState>, engine_id: String) -> Result<(), String> {
    match engine_id.as_str() {
        TESSERACT_ENGINE_ID => write_default_engine(&state.db_path, TESSERACT_ENGINE_ID),
        RAPIDOCR_ENGINE_ID => {
            let target_dir = default_rapidocr_dir().map_err(|error| error.to_string())?;
            verify_install_dir(&target_dir, &RAPIDOCR_V1).map_err(|error| error.to_string())?;
            write_default_engine(&state.db_path, RAPIDOCR_ENGINE_ID)
        }
        _ => return Err(format!("unknown OCR engine: {engine_id}")),
    }
    .map_err(|error| error.to_string())
}

pub fn read_default_engine(db_path: &std::path::Path) -> Result<String, rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let value = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![DEFAULT_ENGINE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(match value.as_deref() {
        Some(RAPIDOCR_ENGINE_ID) => RAPIDOCR_ENGINE_ID.to_string(),
        _ => TESSERACT_ENGINE_ID.to_string(),
    })
}

fn write_default_engine(db_path: &std::path::Path, engine_id: &str) -> Result<(), rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "INSERT OR REPLACE INTO settings(key, value) VALUES(?1, ?2)",
        params![DEFAULT_ENGINE_KEY, engine_id],
    )?;
    Ok(())
}

fn rapidocr_status(is_installing: bool) -> EngineRuntimeStatus {
    if is_installing {
        return EngineRuntimeStatus {
            status: "installing".to_string(),
            error: None,
        };
    }

    let Ok(target_dir) = default_rapidocr_dir() else {
        return EngineRuntimeStatus {
            status: "error".to_string(),
            error: Some("Unable to resolve LOCALAPPDATA for RapidOCR models".to_string()),
        };
    };

    if !target_dir.exists() {
        return EngineRuntimeStatus {
            status: "available".to_string(),
            error: None,
        };
    }

    match verify_install_dir(&target_dir, &RAPIDOCR_V1) {
        Ok(()) => EngineRuntimeStatus {
            status: "installed".to_string(),
            error: None,
        },
        Err(error) => EngineRuntimeStatus {
            status: "error".to_string(),
            error: Some(error.to_string()),
        },
    }
}

struct EngineRuntimeStatus {
    status: String,
    error: Option<String>,
}

fn emit_install_progress(app: &AppHandle, engine_id: &str, progress: InstallProgress) {
    let _ = app.emit(
        "engine.install.progress",
        EngineInstallProgressEvent {
            engine_id: engine_id.to_string(),
            phase: progress.phase,
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
            current_file: progress.current_file,
        },
    );
}

fn installing_engines() -> &'static Mutex<HashSet<String>> {
    INSTALLING_ENGINES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn db_error_to_rusqlite(error: db::DbError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use crate::ocr::{
        rapidocr_install::verify_install_dir,
        rapidocr_manifest::{ModelFile, ModelManifest},
    };
    use sha2::{Digest, Sha256};

    #[test]
    fn rapidocr_missing_file_reports_error_status() {
        let dir = test_dir("engine-status-missing");
        fs::create_dir_all(&dir).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"model"));
        let files = Box::leak(
            vec![ModelFile {
                url: "https://example.com/model.onnx",
                relative_path: "det/model.onnx",
                sha256: Box::leak(hash.into_boxed_str()),
                size: 5,
            }]
            .into_boxed_slice(),
        );
        let manifest = ModelManifest {
            version: "test",
            total_size_mb: 1,
            files,
        };

        let error = verify_install_dir(&dir, &manifest).unwrap_err();
        assert!(error.to_string().contains("missing"));
        let _ = fs::remove_dir_all(dir);
    }

    fn test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("rapidocr-tests")
            .join(format!("{name}-{suffix}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
