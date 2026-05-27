use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, State};

use crate::{
    db,
    ocr::{
        pdf_pipeline::{self, DocumentRecord, PipelineError},
        tesseract::TesseractAdapter,
        OcrAdapter,
    },
    state::AppState,
};

#[tauri::command]
pub async fn process_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    input_path: String,
) -> Result<DocumentRecord, String> {
    let state = state.inner().clone();
    let input_path = canonical_pdf_path(&input_path).map_err(|error| error.to_string())?;
    let sha256 = pdf_pipeline::compute_sha256(&input_path).map_err(|error| error.to_string())?;
    let output_dir = pdf_pipeline::resolve_output_dir(&state.db_path).map_err(|error| error.to_string())?;
    let (document_id, job_id) = create_queued_job(&state.db_path, &input_path, &sha256)
        .map_err(|error| error.to_string())?;

    set_job_running(&state.db_path, job_id).map_err(|error| error.to_string())?;

    let engine: Arc<dyn OcrAdapter> = Arc::new(
        TesseractAdapter::new(app.clone()).map_err(|error| {
            let message = error.to_string();
            let _ = fail_job_and_document(&state.db_path, job_id, document_id, "error", &message);
            message
        })?,
    );

    match pdf_pipeline::process_pdf(
        app,
        state.clone(),
        input_path,
        output_dir,
        document_id,
        job_id,
        engine,
    )
    .await
    {
        Ok(record) => {
            finish_job(&state.db_path, job_id, "done", None).map_err(|error| error.to_string())?;
            Ok(record)
        }
        Err(error) => {
            let document_status = if matches!(error, PipelineError::PasswordRequired) {
                "needs_password"
            } else {
                "error"
            };
            let message = error.to_string();
            let _ = fail_job_and_document(&state.db_path, job_id, document_id, document_status, &message);
            Err(message)
        }
    }
}

fn canonical_pdf_path(input_path: &str) -> Result<PathBuf, std::io::Error> {
    let path = PathBuf::from(input_path);
    let canonical = path.canonicalize()?;
    if !canonical.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "input path is not a file",
        ));
    }
    if canonical
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "input path must be a PDF",
        ));
    }
    Ok(canonical)
}

fn create_queued_job(
    db_path: &std::path::Path,
    input_path: &std::path::Path,
    sha256: &str,
) -> Result<(i64, i64), rusqlite::Error> {
    let mut connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let transaction = connection.transaction()?;
    let now = now_ts();
    let original_path = input_path.to_string_lossy().into_owned();

    let document_id = transaction
        .query_row(
            "SELECT id FROM documents WHERE sha256 = ?1",
            params![sha256],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;

    let document_id = if let Some(document_id) = document_id {
        transaction.execute(
            "UPDATE documents
             SET original_path = ?2,
                 output_path = NULL,
                 page_count = 0,
                 ocr_engine = NULL,
                 status = 'queued',
                 error_message = NULL,
                 updated_at = ?3
             WHERE id = ?1",
            params![document_id, original_path, now],
        )?;
        transaction.execute("DELETE FROM pages WHERE document_id = ?1", params![document_id])?;
        document_id
    } else {
        transaction.execute(
            "INSERT INTO documents(sha256, original_path, page_count, status, ingested_at, updated_at)
             VALUES(?1, ?2, 0, 'queued', ?3, ?3)",
            params![sha256, original_path, now],
        )?;
        transaction.last_insert_rowid()
    };

    transaction.execute(
        "INSERT INTO jobs(document_id, kind, status, created_at)
         VALUES(?1, 'ingest', 'queued', ?2)",
        params![document_id, now],
    )?;
    let job_id = transaction.last_insert_rowid();
    transaction.commit()?;
    Ok((document_id, job_id))
}

fn set_job_running(db_path: &std::path::Path, job_id: i64) -> Result<(), rusqlite::Error> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "UPDATE jobs SET status = 'running', started_at = ?2 WHERE id = ?1",
        params![job_id, now_ts()],
    )?;
    Ok(())
}

fn finish_job(
    db_path: &std::path::Path,
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

fn fail_job_and_document(
    db_path: &std::path::Path,
    job_id: i64,
    document_id: i64,
    document_status: &str,
    error_message: &str,
) -> Result<(), rusqlite::Error> {
    finish_job(db_path, job_id, "error", Some(error_message))?;
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "UPDATE documents
         SET status = ?2,
             error_message = ?3,
             updated_at = ?4
         WHERE id = ?1",
        params![document_id, document_status, error_message, now_ts()],
    )?;
    Ok(())
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
