use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tracing::{info, warn};

use crate::{db, events::AppEventPayload, state::AppState};

use super::{
    chunking::{chunk_pages, Chunk, PageText},
    embeddings::{vector_to_json, EmbedError, EmbeddingRuntime},
};

const TARGET_CHUNK_TOKENS: usize = 512;
const OVERLAP_CHUNK_TOKENS: usize = 64;

#[derive(Debug, Clone, Serialize)]
pub struct EmbedReport {
    pub document_id: i64,
    pub chunks: usize,
    pub pages: usize,
}

#[derive(Debug, Error)]
pub enum IndexError {
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    DbOpen(#[from] db::DbError),
    #[error(transparent)]
    Embed(#[from] EmbedError),
    #[error("embedding count mismatch: {chunks} chunks but {embeddings} embeddings")]
    CountMismatch { chunks: usize, embeddings: usize },
    #[error("embedding dimension mismatch: expected {expected}, got {actual}")]
    Dimension { expected: usize, actual: usize },
}

pub fn queue_document_embedding(app: AppHandle, state: AppState, document_id: i64) {
    tauri::async_runtime::spawn(async move {
        let job_id = match create_embed_job(&state.db_path, document_id) {
            Ok(job_id) => job_id,
            Err(error) => {
                warn!(document_id, error = %error, "failed to create embed job");
                return;
            }
        };
        if let Err(error) = set_job_running(&state.db_path, job_id) {
            warn!(document_id, job_id, error = %error, "failed to mark embed job running");
            return;
        }

        let filename = document_display_name(&state.db_path, document_id)
            .unwrap_or_else(|| "document".to_string());
        emit_progress(
            &app,
            job_id,
            document_id,
            &filename,
            0.0,
            "Initializing embeddings",
        );
        let started = std::time::Instant::now();
        let result = index_document(&state.db_path, &state.embeddings, document_id).await;
        match result {
            Ok(report) => {
                let _ = finish_job(&state.db_path, job_id, "done", None);
                emit_progress(
                    &app,
                    job_id,
                    document_id,
                    &filename,
                    100.0,
                    "Embeddings indexed",
                );
                let _ = app.emit(
                    "document:updated",
                    AppEventPayload::DocumentUpdated {
                        document_id,
                        status: "embedded".to_string(),
                    },
                );
                info!(
                    document_id,
                    job_id,
                    chunks = report.chunks,
                    pages = report.pages,
                    took_ms = started.elapsed().as_millis(),
                    "document embeddings indexed"
                );
            }
            Err(error) => {
                let message = error.to_string();
                let _ = finish_job(&state.db_path, job_id, "error", Some(&message));
                let _ = app.emit(
                    "job:failed",
                    EmbedJobFailedEvent {
                        event_type: "job:failed",
                        job_id,
                        document_id,
                        error: message.clone(),
                    },
                );
                warn!(document_id, job_id, error = %message, "document embedding failed");
            }
        }
    });
}

pub async fn index_document(
    db_path: &Path,
    embeddings: &EmbeddingRuntime,
    document_id: i64,
) -> Result<EmbedReport, IndexError> {
    let pages = load_pages(db_path, document_id)?;
    let chunks = chunk_pages(&pages, TARGET_CHUNK_TOKENS, OVERLAP_CHUNK_TOKENS);
    if chunks.is_empty() {
        persist_chunks(db_path, document_id, Vec::new(), Vec::new(), 384)?;
        return Ok(EmbedReport {
            document_id,
            chunks: 0,
            pages: pages.len(),
        });
    }

    let service = embeddings.get_or_initialize().await?;
    let texts = chunks
        .iter()
        .map(|chunk| chunk.text.clone())
        .collect::<Vec<_>>();
    let vectors = service.embed_batch(texts).await?;
    let dim = service.dim;
    persist_chunks(db_path, document_id, chunks, vectors, dim)?;
    Ok(EmbedReport {
        document_id,
        chunks: count_chunks(db_path, document_id)?,
        pages: pages.len(),
    })
}

fn load_pages(db_path: &Path, document_id: i64) -> Result<Vec<PageText>, IndexError> {
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(
        "SELECT id, text
         FROM pages
         WHERE document_id = ?1
         ORDER BY page_number ASC",
    )?;
    let pages = statement
        .query_map(params![document_id], |row| {
            Ok(PageText {
                document_id,
                page_id: row.get(0)?,
                text: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pages)
}

fn persist_chunks(
    db_path: &Path,
    document_id: i64,
    chunks: Vec<Chunk>,
    embeddings: Vec<Vec<f32>>,
    dim: usize,
) -> Result<(), IndexError> {
    if chunks.len() != embeddings.len() {
        return Err(IndexError::CountMismatch {
            chunks: chunks.len(),
            embeddings: embeddings.len(),
        });
    }
    let mut connection = db::open_connection_at(db_path)?;
    let transaction = connection.transaction()?;

    let old_ids = {
        let mut statement = transaction.prepare("SELECT id FROM chunks WHERE document_id = ?1")?;
        let rows = statement.query_map(params![document_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for chunk_id in old_ids {
        transaction.execute("DELETE FROM chunks_vec WHERE rowid = ?1", params![chunk_id])?;
    }
    transaction.execute(
        "DELETE FROM chunks WHERE document_id = ?1",
        params![document_id],
    )?;

    {
        let mut chunk_insert = transaction.prepare(
            "INSERT INTO chunks(document_id, page_id, char_start, char_end, token_count, text)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut vec_insert =
            transaction.prepare("INSERT INTO chunks_vec(rowid, embedding) VALUES(?1, ?2)")?;
        for (chunk, vector) in chunks.into_iter().zip(embeddings) {
            if vector.len() != dim {
                return Err(IndexError::Dimension {
                    expected: dim,
                    actual: vector.len(),
                });
            }
            chunk_insert.execute(params![
                chunk.document_id,
                chunk.page_id,
                chunk.char_start as i64,
                chunk.char_end as i64,
                chunk.token_count as i64,
                chunk.text,
            ])?;
            let chunk_id = transaction.last_insert_rowid();
            vec_insert.execute(params![chunk_id, vector_to_json(&vector)])?;
        }
    }

    transaction.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')", [])?;
    transaction.commit()?;
    Ok(())
}

fn count_chunks(db_path: &Path, document_id: i64) -> Result<usize, IndexError> {
    let connection = db::open_connection_at(db_path)?;
    let count = connection.query_row(
        "SELECT COUNT(*) FROM chunks WHERE document_id = ?1",
        params![document_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count as usize)
}

fn create_embed_job(db_path: &Path, document_id: i64) -> Result<i64, rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "INSERT INTO jobs(document_id, kind, status, created_at)
         VALUES(?1, 'embed', 'queued', ?2)",
        params![document_id, now_ts()],
    )?;
    Ok(connection.last_insert_rowid())
}

fn set_job_running(db_path: &Path, job_id: i64) -> Result<(), rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "UPDATE jobs SET status = 'running', started_at = ?2 WHERE id = ?1",
        params![job_id, now_ts()],
    )?;
    Ok(())
}

fn finish_job(
    db_path: &Path,
    job_id: i64,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "UPDATE jobs
         SET status = ?2,
             error_message = ?3,
             finished_at = ?4
         WHERE id = ?1",
        params![job_id, status, error_message, now_ts()],
    )?;
    Ok(())
}

fn document_display_name(db_path: &Path, document_id: i64) -> Option<String> {
    let connection = db::open_connection_at(db_path).ok()?;
    let row = connection
        .query_row(
            "SELECT display_name, original_path FROM documents WHERE id = ?1",
            params![document_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .ok()
        .flatten()?;
    row.0.filter(|value| !value.trim().is_empty()).or_else(|| {
        std::path::Path::new(&row.1)
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
    })
}

fn emit_progress(
    app: &AppHandle,
    job_id: i64,
    document_id: i64,
    filename: &str,
    progress_pct: f32,
    message: &str,
) {
    let _ = app.emit(
        "job:progress",
        EmbedJobProgressEvent {
            event_type: "job:progress",
            job_id,
            document_id,
            filename: filename.to_string(),
            stage: "embedding".to_string(),
            progress_pct,
            message: message.to_string(),
            page_number: None,
            page_count: 0,
        },
    );
}

#[derive(Debug, Clone, Serialize)]
struct EmbedJobProgressEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    job_id: i64,
    document_id: i64,
    filename: String,
    stage: String,
    progress_pct: f32,
    message: String,
    page_number: Option<i64>,
    page_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct EmbedJobFailedEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    job_id: i64,
    document_id: i64,
    error: String,
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
