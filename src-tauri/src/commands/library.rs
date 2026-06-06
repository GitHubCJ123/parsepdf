use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::{
    db,
    jobs::{JobManager, JobSummary},
    state::AppState,
    watcher::{IngestJob, JobOrigin},
};

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
pub fn library_open_external(document_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;
    let output_path: String = connection
        .query_row(
            "SELECT output_path FROM documents WHERE id = ?1",
            params![document_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    super::open_in_file_manager(Path::new(&output_path)).map_err(|error| error.to_string())
}

/// Result of the pre-ingest duplicate check (Duplicate Protection).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DuplicateCheck {
    /// Nothing in the library matches by content or name — safe to process.
    New,
    /// The exact file (same SHA256) is already in the library.
    ContentDuplicate {
        // Boxed to keep the enum small (DocumentRow is large); serde serializes
        // the boxed value transparently, so the JSON shape is unchanged.
        existing: Box<DocumentRow>,
        active_job_id: Option<i64>,
    },
    /// A different file already uses this name — suggest a "(vN)" variant.
    NameCollision { suggested_name: String },
}

/// Pre-ingest check: hash the file once and classify it against the library.
/// Powers the duplicate modal / silent-skip / auto-versioning in the inbox.
#[tauri::command]
pub fn library_check_duplicate(
    input_path: String,
    state: State<'_, AppState>,
) -> Result<DuplicateCheck, String> {
    let path = Path::new(&input_path);
    let sha256 =
        crate::ocr::pdf_pipeline::compute_sha256(path).map_err(|error| error.to_string())?;
    let connection = db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;

    // Content duplicate takes priority: same bytes already imported (not deleted).
    let existing_id = connection
        .query_row(
            "SELECT id FROM documents WHERE sha256 = ?1 AND deleted_at IS NULL",
            params![sha256],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(id) = existing_id {
        let existing = query_document_row(&connection, id).map_err(|error| error.to_string())?;
        let active_job_id = connection
            .query_row(
                "SELECT id FROM jobs
                 WHERE document_id = ?1 AND status IN ('queued', 'running', 'paused')
                 ORDER BY created_at DESC LIMIT 1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        return Ok(DuplicateCheck::ContentDuplicate {
            existing: Box::new(existing),
            active_job_id,
        });
    }

    // Different content but a colliding name → suggest a versioned name.
    let incoming = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.pdf");
    if let Some(suggested) =
        next_version_name(&connection, incoming).map_err(|error| error.to_string())?
    {
        return Ok(DuplicateCheck::NameCollision {
            suggested_name: suggested,
        });
    }

    Ok(DuplicateCheck::New)
}

/// Re-run OCR for an existing document (reuses its row, matched by SHA256 of
/// the original source). Used by the duplicate modal and the Library "Reprocess"
/// action. `engine_override` lets the caller pick a different OCR engine; when
/// `None` the document's current engine is kept.
#[tauri::command]
pub async fn library_force_reprocess(
    document_id: i64,
    engine_override: Option<String>,
    manager: State<'_, JobManager>,
    state: State<'_, AppState>,
) -> Result<JobSummary, String> {
    let (original_path, stored_engine) = {
        let connection =
            db::open_connection_at(&state.db_path).map_err(|error| error.to_string())?;
        connection
            .query_row(
                "SELECT original_path, ocr_engine FROM documents WHERE id = ?1",
                params![document_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map_err(|error| error.to_string())?
    };
    // Reprocessing re-reads the ORIGINAL source so the SHA256 still matches this
    // document's row. If that file is gone, re-OCR isn't possible from here.
    if !Path::new(&original_path).is_file() {
        return Err(format!(
            "The original file is no longer available at {original_path}, so it can't be reprocessed."
        ));
    }
    let engine = engine_override
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or(stored_engine);
    manager
        .enqueue_ingest(IngestJob {
            source_path: original_path.into(),
            origin: JobOrigin::Manual,
            engine,
            display_name: None,
        })
        .await
        .map_err(|error| error.to_string())
}

/// If an existing (non-deleted) document already uses the incoming file's name
/// (or a "(vN)" variant of it), return the next free "<stem> (vN).<ext>" name.
/// Returns `None` when there is no name collision.
fn next_version_name(
    connection: &rusqlite::Connection,
    incoming_filename: &str,
) -> rusqlite::Result<Option<String>> {
    let (incoming_stem, ext) = split_name(incoming_filename);
    let (root_stem, _) = strip_version_suffix(&incoming_stem);
    let target = root_stem.to_lowercase();

    let mut statement = connection.prepare(
        "SELECT COALESCE(NULLIF(TRIM(display_name), ''), original_path)
         FROM documents WHERE deleted_at IS NULL",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut max_version: Option<u32> = None;
    for name in names {
        let display_stem = split_name(&basename(&name)).0;
        let (root, version) = strip_version_suffix(&display_stem);
        if root.to_lowercase() == target {
            let observed = version.unwrap_or(1);
            max_version = Some(max_version.map_or(observed, |current| current.max(observed)));
        }
    }

    Ok(max_version.map(|max| {
        let next = max + 1;
        match ext {
            Some(ext) => format!("{root_stem} (v{next}).{ext}"),
            None => format!("{root_stem} (v{next})"),
        }
    }))
}

/// Split a filename into (stem, extension). "report.pdf" -> ("report", Some("pdf")).
fn split_name(filename: &str) -> (String, Option<String>) {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename)
        .to_string();
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string());
    (stem, ext)
}

/// Strip a trailing " (vN)" from a name stem. "report (v2)" -> ("report", Some(2)).
fn strip_version_suffix(stem: &str) -> (String, Option<u32>) {
    let trimmed = stem.trim_end();
    if trimmed.ends_with(')') {
        if let Some(open) = trimmed.rfind(" (v") {
            let inner = &trimmed[open + 3..trimmed.len() - 1];
            if !inner.is_empty() && inner.chars().all(|ch| ch.is_ascii_digit()) {
                if let Ok(version) = inner.parse::<u32>() {
                    return (trimmed[..open].to_string(), Some(version));
                }
            }
        }
    }
    (trimmed.to_string(), None)
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

fn db_error_to_rusqlite(error: db::DbError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::{next_version_name, split_name, strip_version_suffix};
    use rusqlite::Connection;

    fn seed(names: &[(&str, Option<i64>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE documents(
                id INTEGER PRIMARY KEY,
                display_name TEXT,
                original_path TEXT NOT NULL,
                deleted_at INTEGER
            );",
        )
        .unwrap();
        for (index, (name, deleted)) in names.iter().enumerate() {
            conn.execute(
                "INSERT INTO documents(id, display_name, original_path, deleted_at) VALUES(?1, ?2, ?3, ?4)",
                rusqlite::params![index as i64 + 1, name, format!("C:/in/{name}"), deleted],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn strip_version_suffix_parses_v_numbers() {
        assert_eq!(strip_version_suffix("report"), ("report".to_string(), None));
        assert_eq!(strip_version_suffix("report (v2)"), ("report".to_string(), Some(2)));
        assert_eq!(strip_version_suffix("report (v10)"), ("report".to_string(), Some(10)));
        // Not a version suffix — left intact.
        assert_eq!(strip_version_suffix("report (draft)"), ("report (draft)".to_string(), None));
    }

    #[test]
    fn split_name_separates_stem_and_ext() {
        assert_eq!(split_name("report.pdf"), ("report".to_string(), Some("pdf".to_string())));
        assert_eq!(split_name("report"), ("report".to_string(), None));
    }

    #[test]
    fn next_version_name_none_when_no_collision() {
        let conn = seed(&[("invoice.pdf", None)]);
        assert_eq!(next_version_name(&conn, "report.pdf").unwrap(), None);
    }

    #[test]
    fn next_version_name_suggests_v2_on_first_collision() {
        let conn = seed(&[("report.pdf", None)]);
        assert_eq!(
            next_version_name(&conn, "report.pdf").unwrap(),
            Some("report (v2).pdf".to_string())
        );
    }

    #[test]
    fn next_version_name_picks_next_after_highest_existing() {
        let conn = seed(&[("report.pdf", None), ("report (v2).pdf", None)]);
        assert_eq!(
            next_version_name(&conn, "report.pdf").unwrap(),
            Some("report (v3).pdf".to_string())
        );
    }

    #[test]
    fn next_version_name_ignores_deleted_documents() {
        let conn = seed(&[("report.pdf", Some(123))]);
        assert_eq!(next_version_name(&conn, "report.pdf").unwrap(), None);
    }

    #[test]
    fn next_version_name_is_case_insensitive() {
        let conn = seed(&[("Report.pdf", None)]);
        assert_eq!(
            next_version_name(&conn, "report.pdf").unwrap(),
            Some("report (v2).pdf".to_string())
        );
    }
}
