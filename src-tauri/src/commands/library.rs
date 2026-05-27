use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::{db, state::AppState};

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRow {
    pub id: i64,
    pub display_name: String,
    pub original_name: String,
    pub original_path: String,
    pub output_path: Option<String>,
    pub page_count: i64,
    pub ingested_at: i64,
    pub updated_at: i64,
    pub ocr_engine: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_summary: Option<String>,
    pub status: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentDetail {
    pub document: DocumentRow,
    pub pages: Vec<DocumentPagePreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentPagePreview {
    pub page_number: i64,
    pub text: String,
    pub ocr_status: String,
    pub mean_confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingRenameRow {
    pub document_id: i64,
    pub original_name: String,
    pub current_name: String,
    pub output_path: Option<String>,
    pub proposed_name: String,
    pub summary: Option<String>,
    pub provider: String,
    pub created_at: i64,
    pub reviewed: i64,
}

#[tauri::command]
pub fn library_list(
    query: Option<String>,
    limit: u32,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<Vec<DocumentRow>, String> {
    list_documents(&state.db_path, query, limit, offset).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_get(document_id: i64, state: State<'_, AppState>) -> Result<DocumentDetail, String> {
    let connection = db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;
    let document =
        query_document_row(&connection, document_id).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT page_number, substr(text, 1, 1200), ocr_status, mean_confidence
             FROM pages WHERE document_id = ?1 ORDER BY page_number ASC LIMIT 50",
        )
        .map_err(|error| error.to_string())?;
    let pages = statement
        .query_map(params![document_id], |row| {
            Ok(DocumentPagePreview {
                page_number: row.get(0)?,
                text: row.get(1)?,
                ocr_status: row.get(2)?,
                mean_confidence: row.get::<_, Option<f32>>(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(DocumentDetail { document, pages })
}

#[tauri::command]
pub fn library_delete(
    document_id: i64,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let connection = db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;
    let output_path = connection
        .query_row(
            "SELECT output_path FROM documents WHERE id = ?1",
            params![document_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();

    if force.unwrap_or(false) {
        if let Some(path) = output_path.as_ref() {
            let path = PathBuf::from(path);
            if path.exists() {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        connection
            .execute("DELETE FROM documents WHERE id = ?1", params![document_id])
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "UPDATE documents SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![document_id, now_ts()],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn library_pending_renames(
    state: State<'_, AppState>,
) -> Result<Vec<PendingRenameRow>, String> {
    pending_renames(&state.db_path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_skip_rename(document_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    skip_rename(&state.db_path, document_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn library_open_external(document_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;
    let output_path: String = connection
        .query_row(
            "SELECT output_path FROM documents WHERE id = ?1",
            params![document_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    open_external(Path::new(&output_path)).map_err(|error| error.to_string())
}

pub fn list_documents(
    db_path: &Path,
    query: Option<String>,
    limit: u32,
    offset: u32,
) -> rusqlite::Result<Vec<DocumentRow>> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let limit = limit.clamp(1, 500);
    let search = query
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(search) = search {
        let like = format!("%{}%", search.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = connection.prepare(
            "SELECT id FROM documents
             WHERE deleted_at IS NULL
               AND (display_name LIKE ?1 ESCAPE '\\' OR original_path LIKE ?1 ESCAPE '\\')
             ORDER BY ingested_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let ids = statement
            .query_map(params![like, limit, offset], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| query_document_row(&connection, id))
            .collect()
    } else {
        let mut statement = connection.prepare(
            "SELECT id FROM documents
             WHERE deleted_at IS NULL AND output_path IS NOT NULL
             ORDER BY ingested_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let ids = statement
            .query_map(params![limit, offset], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| query_document_row(&connection, id))
            .collect()
    }
}

pub fn pending_renames(db_path: &Path) -> rusqlite::Result<Vec<PendingRenameRow>> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let mut statement = connection.prepare(
        "SELECT p.document_id,
                d.original_path,
                COALESCE(d.display_name, ''),
                d.output_path,
                p.proposed_name,
                p.summary,
                p.provider,
                p.created_at,
                p.reviewed
         FROM pending_renames p
         JOIN documents d ON d.id = p.document_id
         WHERE p.reviewed = 0 AND d.deleted_at IS NULL
         ORDER BY p.created_at DESC",
    )?;
    let rows = statement
        .query_map([], |row| {
            let original_path: String = row.get(1)?;
            Ok(PendingRenameRow {
                document_id: row.get(0)?,
                original_name: basename(&original_path),
                current_name: row.get(2)?,
                output_path: row.get(3)?,
                proposed_name: row.get(4)?,
                summary: row.get(5)?,
                provider: row.get(6)?,
                created_at: row.get(7)?,
                reviewed: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn skip_rename(db_path: &Path, document_id: i64) -> rusqlite::Result<()> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let original_path: String = connection.query_row(
        "SELECT original_path FROM documents WHERE id = ?1",
        params![document_id],
        |row| row.get(0),
    )?;
    let original_name = basename(&original_path);
    connection.execute(
        "UPDATE pending_renames SET reviewed = 2 WHERE document_id = ?1",
        params![document_id],
    )?;
    connection.execute(
        "UPDATE documents
         SET status = 'done',
             display_name = COALESCE(display_name, ?2),
             ai_provider = COALESCE(ai_provider, 'none'),
             updated_at = ?3
         WHERE id = ?1",
        params![document_id, original_name, now_ts()],
    )?;
    Ok(())
}

fn query_document_row(
    connection: &rusqlite::Connection,
    document_id: i64,
) -> rusqlite::Result<DocumentRow> {
    connection.query_row(
        "SELECT id, original_path, output_path, display_name, page_count, ingested_at, updated_at,
                ocr_engine, ai_provider, ai_summary, status
         FROM documents WHERE id = ?1 AND deleted_at IS NULL",
        params![document_id],
        |row| {
            let original_path: String = row.get(1)?;
            let output_path: Option<String> = row.get(2)?;
            let display_name = row
                .get::<_, Option<String>>(3)?
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| basename(&original_path));
            let size_bytes = output_path
                .as_ref()
                .and_then(|path| fs::metadata(path).ok())
                .map(|metadata| metadata.len());
            Ok(DocumentRow {
                id: row.get(0)?,
                display_name,
                original_name: basename(&original_path),
                original_path,
                output_path,
                page_count: row.get(4)?,
                ingested_at: row.get(5)?,
                updated_at: row.get(6)?,
                ocr_engine: row.get(7)?,
                ai_provider: row.get(8)?,
                ai_summary: row.get(9)?,
                status: row.get(10)?,
                size_bytes,
            })
        },
    )
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn open_external(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        Command::new("explorer.exe").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}

fn db_error_to_rusqlite(error: db::DbError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
