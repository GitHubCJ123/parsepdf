use rusqlite::params;
use serde::Serialize;
use tauri::State;
use tracing::info;

use crate::{db, state::AppState};

#[derive(Debug, Serialize)]
pub struct DebugDocument {
    pub id: i64,
    pub sha256_short: String,
    pub original_path: String,
    pub output_path: Option<String>,
    pub display_name: Option<String>,
    pub status: String,
    pub page_count: i64,
    pub ingested_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct DebugJob {
    pub id: i64,
    pub document_id: Option<i64>,
    pub kind: String,
    pub status: String,
    pub origin: String,
    pub engine: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DebugStateDump {
    pub db_path: String,
    pub documents: Vec<DebugDocument>,
    pub jobs: Vec<DebugJob>,
    pub documents_count: i64,
    pub jobs_count: i64,
}

#[tauri::command]
pub async fn debug_dump_state(state: State<'_, AppState>) -> Result<DebugStateDump, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || dump(&db_path))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn dump(db_path: &std::path::Path) -> Result<DebugStateDump, rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(|error| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(error.to_string()),
        )
    })?;

    let mut documents = Vec::new();
    let mut document_stmt = connection.prepare(
        "SELECT id, sha256, original_path, output_path, display_name, status, page_count, ingested_at, updated_at
         FROM documents ORDER BY ingested_at DESC LIMIT 500",
    )?;
    let mut rows = document_stmt.query([])?;
    while let Some(row) = rows.next()? {
        let sha: String = row.get(1)?;
        documents.push(DebugDocument {
            id: row.get(0)?,
            sha256_short: sha.chars().take(16).collect(),
            original_path: row.get(2)?,
            output_path: row.get(3)?,
            display_name: row.get(4)?,
            status: row.get(5)?,
            page_count: row.get(6)?,
            ingested_at: row.get(7)?,
            updated_at: row.get(8)?,
        });
    }

    let mut jobs = Vec::new();
    let mut job_stmt = connection.prepare(
        "SELECT id, document_id, kind, status, COALESCE(origin, 'manual'), engine,
                created_at, started_at, finished_at, error_message
         FROM jobs ORDER BY created_at DESC LIMIT 500",
    )?;
    let mut rows = job_stmt.query([])?;
    while let Some(row) = rows.next()? {
        jobs.push(DebugJob {
            id: row.get(0)?,
            document_id: row.get(1)?,
            kind: row.get(2)?,
            status: row.get(3)?,
            origin: row.get(4)?,
            engine: row.get(5)?,
            created_at: row.get(6)?,
            started_at: row.get(7)?,
            finished_at: row.get(8)?,
            error_message: row.get(9)?,
        });
    }

    let documents_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
    let jobs_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;

    info!(
        documents_count,
        jobs_count,
        documents_returned = documents.len(),
        jobs_returned = jobs.len(),
        "[command] debug_dump_state"
    );

    Ok(DebugStateDump {
        db_path: db_path.to_string_lossy().into_owned(),
        documents,
        jobs,
        documents_count,
        jobs_count,
    })
}

#[tauri::command]
pub async fn debug_reset_library(state: State<'_, AppState>) -> Result<u32, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || reset(&db_path))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn reset(db_path: &std::path::Path) -> Result<u32, rusqlite::Error> {
    let mut connection = db::open_connection_at(db_path).map_err(|error| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(error.to_string()),
        )
    })?;
    let transaction = connection.transaction()?;
    let documents_count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
    transaction.execute("DELETE FROM jobs", params![])?;
    transaction.execute("DELETE FROM pages", params![])?;
    transaction.execute("DELETE FROM chunks", params![])?;
    transaction.execute("DELETE FROM pending_renames", params![])?;
    transaction.execute("DELETE FROM documents", params![])?;
    // sqlite-vec rows have rowids tied to chunks.id which we just wiped.
    // The virtual table isn't auto-cascaded, so wipe it too.
    let _ = transaction.execute("DELETE FROM chunks_vec", params![]);
    transaction.commit()?;
    info!(
        documents_deleted = documents_count,
        "[command] debug_reset_library wiped library"
    );
    Ok(documents_count as u32)
}
