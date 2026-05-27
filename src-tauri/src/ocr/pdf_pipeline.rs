use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use pdfium_render::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use thiserror::Error;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{commands::ai as ai_commands, db, events::JobProgressUpdate, rag, state::AppState};

use super::{
    composer::{compose_searchable_pdf, PageOcrLayer},
    geometry::PageGeometry,
    OcrAdapter, OcrPage,
};

const OCR_DPI: u32 = 200;
const NATIVE_TEXT_THRESHOLD: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRecord {
    pub id: i64,
    pub sha256: String,
    pub original_path: String,
    pub output_path: Option<String>,
    pub display_name: Option<String>,
    pub page_count: i64,
    pub ocr_engine: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub ingested_at: i64,
    pub updated_at: i64,
    pub pages: Vec<PageRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageRecord {
    pub id: i64,
    pub page_number: i64,
    pub text: String,
    pub ocr_status: String,
    pub mean_confidence: Option<f32>,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    pub dpi: Option<u32>,
    pub rotation: i32,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("password-protected PDFs are not supported yet")]
    PasswordRequired,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database setup error: {0}")]
    DatabaseSetup(#[from] db::DbError),
    #[error("file IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDFium error: {0}")]
    Pdfium(String),
    #[error("PDF composition error: {0}")]
    Composition(#[from] anyhow::Error),
    #[error("task join error: {0}")]
    Join(String),
    #[error("unsafe output path: {0}")]
    UnsafeOutputPath(String),
    #[error("job was cancelled")]
    Cancelled,
}

#[expect(
    clippy::too_many_arguments,
    reason = "pipeline entry point passes explicit job context"
)]
pub async fn process_pdf(
    app: AppHandle,
    state: AppState,
    input_path: PathBuf,
    output_dir: PathBuf,
    document_id: i64,
    job_id: i64,
    engine: Arc<dyn OcrAdapter>,
    cancel: CancellationToken,
) -> Result<DocumentRecord, PipelineError> {
    ensure_not_cancelled(&cancel)?;
    let filename = input_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("PDF")
        .to_string();
    let page_count = load_page_count(state.pdfium_path.clone(), input_path.clone()).await?;
    ensure_not_cancelled(&cancel)?;
    update_document_stage(
        &state.db_path,
        document_id,
        "rasterizing",
        Some(engine.name()),
        Some(page_count as i64),
        None,
        None,
    )?;
    emit_progress(
        &state,
        &filename,
        job_id,
        document_id,
        "rasterize",
        0.0,
        "Opening PDF",
        None,
        page_count,
    );

    info!(
        document_id,
        job_id,
        page_count,
        engine = engine.name(),
        "starting OCR pipeline"
    );

    let mut processed_pages = Vec::<ProcessedPage>::new();
    let mut any_ocr_failed = false;

    for page_index in 0..page_count {
        ensure_not_cancelled(&cancel)?;
        let page_number = page_index as i64 + 1;
        emit_progress(
            &state,
            &filename,
            job_id,
            document_id,
            "rasterize",
            page_progress(page_index, page_count, 0.0),
            "Rasterizing page",
            Some(page_number),
            page_count,
        );

        let extraction = extract_or_render_page(
            state.pdfium_path.clone(),
            input_path.clone(),
            page_index,
            OCR_DPI,
        )
        .await?;
        ensure_not_cancelled(&cancel)?;

        let processed = match extraction {
            PageExtraction::Native(native) => {
                info!(
                    document_id,
                    job_id, page_number, "skipping OCR for native text page"
                );
                ProcessedPage {
                    page_number,
                    text: native.text,
                    ocr_status: "native_text".to_string(),
                    mean_confidence: None,
                    width_px: Some(native.width_px),
                    height_px: Some(native.height_px),
                    dpi: Some(OCR_DPI),
                    rotation: native.rotation,
                    geometry: native.geometry,
                    ocr_page: None,
                }
            }
            PageExtraction::Rasterized(rasterized) => {
                update_document_stage(
                    &state.db_path,
                    document_id,
                    "ocr",
                    Some(engine.name()),
                    Some(page_count as i64),
                    None,
                    None,
                )?;
                emit_progress(
                    &state,
                    &filename,
                    job_id,
                    document_id,
                    "ocr",
                    page_progress(page_index, page_count, 0.35),
                    "Running OCR",
                    Some(page_number),
                    page_count,
                );

                match state
                    .worker_pool
                    .ocr_page(
                        engine.clone(),
                        rasterized.image,
                        page_index as u32,
                        OCR_DPI,
                        cancel.clone(),
                    )
                    .await
                {
                    Ok(ocr_page) => ProcessedPage {
                        page_number,
                        text: ocr_page.plain_text.clone(),
                        ocr_status: "ocr_done".to_string(),
                        mean_confidence: ocr_page.mean_confidence,
                        width_px: Some(ocr_page.image_width_px),
                        height_px: Some(ocr_page.image_height_px),
                        dpi: Some(ocr_page.dpi),
                        rotation: rasterized.rotation,
                        geometry: rasterized.geometry,
                        ocr_page: Some(ocr_page),
                    },
                    Err(error) => {
                        if cancel.is_cancelled() {
                            return Err(PipelineError::Cancelled);
                        }
                        any_ocr_failed = true;
                        info!(document_id, job_id, page_number, error = %error, "OCR failed for page");
                        ProcessedPage {
                            page_number,
                            text: String::new(),
                            ocr_status: "ocr_failed".to_string(),
                            mean_confidence: None,
                            width_px: Some(rasterized.width_px),
                            height_px: Some(rasterized.height_px),
                            dpi: Some(OCR_DPI),
                            rotation: rasterized.rotation,
                            geometry: rasterized.geometry,
                            ocr_page: None,
                        }
                    }
                }
            }
        };

        processed_pages.push(processed);
        emit_progress(
            &state,
            &filename,
            job_id,
            document_id,
            "indexing",
            page_progress(page_index + 1, page_count, 0.0),
            "Indexed page text",
            Some(page_number),
            page_count,
        );
    }

    emit_progress(
        &state,
        &filename,
        job_id,
        document_id,
        "composing",
        92.0,
        "Composing searchable PDF",
        None,
        page_count,
    );
    update_document_stage(
        &state.db_path,
        document_id,
        "indexing",
        Some(engine.name()),
        Some(page_count as i64),
        None,
        None,
    )?;

    ensure_not_cancelled(&cancel)?;
    let sha256 = compute_sha256(&input_path)?;
    let output_path = prepare_output_path(&input_path, &output_dir, &sha256)?;
    let layers = processed_pages
        .iter()
        .filter_map(|page| {
            page.ocr_page.as_ref().map(|ocr_page| PageOcrLayer {
                page_number: page.page_number as u32,
                geometry: page.geometry,
                ocr_page: ocr_page.clone(),
            })
        })
        .collect::<Vec<_>>();

    let compose_input = input_path.clone();
    let compose_output = output_path.clone();
    task::spawn_blocking(move || compose_searchable_pdf(&compose_input, &compose_output, &layers))
        .await
        .map_err(|error| PipelineError::Join(error.to_string()))??;
    if cancel.is_cancelled() {
        cleanup_partial_output(&output_path);
        return Err(PipelineError::Cancelled);
    }
    replace_page_records(&state.db_path, document_id, &processed_pages)?;

    let queue_ai_naming = ai_commands::should_queue_naming(&state.db_path);
    let final_status = if queue_ai_naming {
        "naming"
    } else if any_ocr_failed {
        "partial_success"
    } else {
        "done"
    };
    update_document_stage(
        &state.db_path,
        document_id,
        final_status,
        Some(engine.name()),
        Some(page_count as i64),
        Some(&output_path),
        None,
    )?;
    set_document_ai_naming_enabled(&state.db_path, document_id, queue_ai_naming)?;
    if queue_ai_naming {
        ai_commands::queue_document_naming(app.clone(), state.clone(), document_id);
    } else {
        set_default_document_name(&state.db_path, document_id, &filename)?;
    }
    emit_progress(
        &state,
        &filename,
        job_id,
        document_id,
        final_status,
        100.0,
        if queue_ai_naming {
            "OCR complete; AI naming review queued"
        } else {
            "Processing complete"
        },
        None,
        page_count,
    );
    rag::indexing::queue_document_embedding(app.clone(), state.clone(), document_id);

    load_document_record(&state.db_path, document_id)
}

pub fn compute_sha256(path: &Path) -> Result<String, PipelineError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn prepare_output_path(
    input_path: &Path,
    output_dir: &Path,
    sha256: &str,
) -> Result<PathBuf, PipelineError> {
    fs::create_dir_all(output_dir)?;
    let canonical_output_dir = output_dir.canonicalize()?;
    let stem = input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(sanitize_file_stem)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "document".to_string());
    let short_sha = sha256.chars().take(8).collect::<String>();

    for attempt in 0..1000 {
        let filename = if attempt == 0 {
            format!("{stem}-{short_sha}-searchable.pdf")
        } else {
            format!("{stem}-{short_sha}-{attempt}-searchable.pdf")
        };
        let candidate = canonical_output_dir.join(filename);
        let parent = candidate.parent().ok_or_else(|| {
            PipelineError::UnsafeOutputPath(candidate.to_string_lossy().into_owned())
        })?;
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&canonical_output_dir) {
            return Err(PipelineError::UnsafeOutputPath(
                candidate.to_string_lossy().into_owned(),
            ));
        }
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(PipelineError::UnsafeOutputPath(format!(
        "could not choose unique output path under {}",
        canonical_output_dir.display()
    )))
}

pub fn load_document_record(
    db_path: &Path,
    document_id: i64,
) -> Result<DocumentRecord, PipelineError> {
    let connection = db::open_connection_at(db_path)?;
    let mut document = connection.query_row(
        "SELECT id, sha256, original_path, output_path, display_name, page_count, ocr_engine, status, error_message, ingested_at, updated_at
         FROM documents WHERE id = ?1",
        params![document_id],
        |row| {
            Ok(DocumentRecord {
                id: row.get(0)?,
                sha256: row.get(1)?,
                original_path: row.get(2)?,
                output_path: row.get(3)?,
                display_name: row.get(4)?,
                page_count: row.get(5)?,
                ocr_engine: row.get(6)?,
                status: row.get(7)?,
                error_message: row.get(8)?,
                ingested_at: row.get(9)?,
                updated_at: row.get(10)?,
                pages: Vec::new(),
            })
        },
    )?;

    let mut stmt = connection.prepare(
        "SELECT id, page_number, text, ocr_status, mean_confidence, width_px, height_px, dpi, rotation
         FROM pages WHERE document_id = ?1 ORDER BY page_number",
    )?;
    let pages = stmt
        .query_map(params![document_id], |row| {
            Ok(PageRecord {
                id: row.get(0)?,
                page_number: row.get(1)?,
                text: row.get(2)?,
                ocr_status: row.get(3)?,
                mean_confidence: row.get::<_, Option<f32>>(4)?,
                width_px: row.get::<_, Option<u32>>(5)?,
                height_px: row.get::<_, Option<u32>>(6)?,
                dpi: row.get::<_, Option<u32>>(7)?,
                rotation: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    document.pages = pages;
    Ok(document)
}

pub fn resolve_output_dir(db_path: &Path) -> Result<PathBuf, PipelineError> {
    let connection = db::open_connection_at(db_path)?;
    if let Some(value) = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'output_dir'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        let path = PathBuf::from(value);
        fs::create_dir_all(&path)?;
        return Ok(path.canonicalize()?);
    }

    let default = db::default_output_dir()?;
    fs::create_dir_all(&default)?;
    let canonical = default.canonicalize()?;
    connection.execute(
        "INSERT OR REPLACE INTO settings(key, value) VALUES('output_dir', ?1)",
        params![canonical.to_string_lossy().as_ref()],
    )?;
    Ok(canonical)
}

fn set_document_ai_naming_enabled(
    db_path: &Path,
    document_id: i64,
    enabled: bool,
) -> Result<(), PipelineError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE documents
         SET ai_naming_enabled = ?2,
             updated_at = ?3
         WHERE id = ?1",
        params![document_id, if enabled { 1 } else { 0 }, now_ts()],
    )?;
    Ok(())
}

fn set_default_document_name(
    db_path: &Path,
    document_id: i64,
    filename: &str,
) -> Result<(), PipelineError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE documents
         SET display_name = COALESCE(display_name, ?2),
             ai_provider = COALESCE(ai_provider, 'none'),
             updated_at = ?3
         WHERE id = ?1",
        params![document_id, filename, now_ts()],
    )?;
    Ok(())
}

pub fn update_document_stage(
    db_path: &Path,
    document_id: i64,
    status: &str,
    ocr_engine: Option<&str>,
    page_count: Option<i64>,
    output_path: Option<&Path>,
    error_message: Option<&str>,
) -> Result<(), PipelineError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE documents
         SET status = ?2,
             ocr_engine = COALESCE(?3, ocr_engine),
             page_count = COALESCE(?4, page_count),
             output_path = COALESCE(?5, output_path),
             error_message = ?6,
             updated_at = ?7
         WHERE id = ?1",
        params![
            document_id,
            status,
            ocr_engine,
            page_count,
            output_path.map(|path| path.to_string_lossy().into_owned()),
            error_message,
            now_ts(),
        ],
    )?;
    Ok(())
}

fn replace_page_records(
    db_path: &Path,
    document_id: i64,
    pages: &[ProcessedPage],
) -> Result<(), PipelineError> {
    let mut connection = db::open_connection_at(db_path)?;
    let transaction = connection.transaction()?;
    let old_chunk_ids = {
        let mut statement = transaction.prepare("SELECT id FROM chunks WHERE document_id = ?1")?;
        let rows = statement.query_map(params![document_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for chunk_id in old_chunk_ids {
        transaction.execute("DELETE FROM chunks_vec WHERE rowid = ?1", params![chunk_id])?;
    }
    transaction.execute(
        "DELETE FROM pages WHERE document_id = ?1",
        params![document_id],
    )?;
    for page in pages {
        transaction.execute(
            "INSERT INTO pages(document_id, page_number, text, ocr_status, mean_confidence, width_px, height_px, dpi, rotation)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                document_id,
                page.page_number,
                page.text,
                page.ocr_status,
                page.mean_confidence,
                page.width_px,
                page.height_px,
                page.dpi,
                page.rotation,
            ],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

async fn load_page_count(pdfium_path: PathBuf, input_path: PathBuf) -> Result<u16, PipelineError> {
    task::spawn_blocking(move || {
        let pdfium = open_pdfium(&pdfium_path)?;
        let document = pdfium
            .load_pdf_from_file(&input_path, None)
            .map_err(map_pdfium_error)?;
        Ok(document.pages().len())
    })
    .await
    .map_err(|error| PipelineError::Join(error.to_string()))?
}

async fn extract_or_render_page(
    pdfium_path: PathBuf,
    input_path: PathBuf,
    page_index: u16,
    dpi: u32,
) -> Result<PageExtraction, PipelineError> {
    task::spawn_blocking(move || {
        extract_or_render_page_blocking(&pdfium_path, &input_path, page_index, dpi)
    })
    .await
    .map_err(|error| PipelineError::Join(error.to_string()))?
}

fn extract_or_render_page_blocking(
    pdfium_path: &Path,
    input_path: &Path,
    page_index: u16,
    dpi: u32,
) -> Result<PageExtraction, PipelineError> {
    let pdfium = open_pdfium(pdfium_path)?;
    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(map_pdfium_error)?;
    let page = document.pages().get(page_index).map_err(map_pdfium_error)?;
    let rotation = page.rotation().map_err(map_pdfium_error)?.as_degrees() as i32;
    let width_pt = page.width().value;
    let height_pt = page.height().value;
    let (image_width_px, image_height_px) = rendered_pixel_size(width_pt, height_pt, rotation, dpi);
    let geometry = PageGeometry {
        media_box: [0.0, 0.0, width_pt, height_pt],
        crop_box: [0.0, 0.0, width_pt, height_pt],
        rotation,
        image_width_px,
        image_height_px,
        dpi,
    };

    let native_text = page.text().map(|text| text.all()).unwrap_or_default();
    if text_density(&native_text) > NATIVE_TEXT_THRESHOLD {
        return Ok(PageExtraction::Native(PageNative {
            text: native_text,
            geometry,
            width_px: image_width_px,
            height_px: image_height_px,
            rotation,
        }));
    }

    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(image_width_px as i32)
                .set_target_height(image_height_px as i32)
                .render_form_data(true),
        )
        .map_err(map_pdfium_error)?;
    let image = bitmap.as_image().into_rgba8();

    Ok(PageExtraction::Rasterized(PageRasterized {
        image,
        geometry,
        width_px: image_width_px,
        height_px: image_height_px,
        rotation,
    }))
}

fn open_pdfium(pdfium_path: &Path) -> Result<Pdfium, PipelineError> {
    let bindings = Pdfium::bind_to_library(pdfium_path).map_err(map_pdfium_error)?;
    Ok(Pdfium::new(bindings))
}

fn rendered_pixel_size(width_pt: f32, height_pt: f32, rotation: i32, dpi: u32) -> (u32, u32) {
    let (display_width_pt, display_height_pt) = if matches!(rotation.rem_euclid(360), 90 | 270) {
        (height_pt, width_pt)
    } else {
        (width_pt, height_pt)
    };
    (
        ((display_width_pt / 72.0) * dpi as f32).round().max(1.0) as u32,
        ((display_height_pt / 72.0) * dpi as f32).round().max(1.0) as u32,
    )
}

fn text_density(text: &str) -> usize {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

fn ensure_not_cancelled(cancel: &CancellationToken) -> Result<(), PipelineError> {
    if cancel.is_cancelled() {
        Err(PipelineError::Cancelled)
    } else {
        Ok(())
    }
}

fn cleanup_partial_output(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn map_pdfium_error(error: PdfiumError) -> PipelineError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            PipelineError::PasswordRequired
        }
        other => PipelineError::Pdfium(other.to_string()),
    }
}

fn sanitize_file_stem(stem: &str) -> String {
    let mut sanitized = stem
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    sanitized.truncate(80);
    sanitized.trim_matches([' ', '.']).to_string()
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn page_progress(completed_pages: u16, total_pages: u16, within_page: f32) -> f32 {
    if total_pages == 0 {
        return 0.0;
    }
    let completed = completed_pages as f32 + within_page.clamp(0.0, 1.0);
    ((completed / total_pages as f32) * 90.0).clamp(0.0, 90.0)
}

#[allow(clippy::too_many_arguments)]
fn emit_progress(
    state: &AppState,
    filename: &str,
    job_id: i64,
    document_id: i64,
    stage: &str,
    progress_pct: f32,
    message: &str,
    page_number: Option<i64>,
    page_count: u16,
) {
    state.progress.notify_progress(JobProgressUpdate {
        job_id,
        document_id,
        filename: filename.to_string(),
        stage: stage.to_string(),
        progress_pct,
        message: message.to_string(),
        page_number,
        page_count: page_count as i64,
        eta_seconds: None,
    });
}

#[derive(Debug)]
enum PageExtraction {
    Native(PageNative),
    Rasterized(PageRasterized),
}

#[derive(Debug)]
struct PageNative {
    text: String,
    geometry: PageGeometry,
    width_px: u32,
    height_px: u32,
    rotation: i32,
}

#[derive(Debug)]
struct PageRasterized {
    image: image::RgbaImage,
    geometry: PageGeometry,
    width_px: u32,
    height_px: u32,
    rotation: i32,
}

#[derive(Debug)]
struct ProcessedPage {
    page_number: i64,
    text: String,
    ocr_status: String,
    mean_confidence: Option<f32>,
    width_px: Option<u32>,
    height_px: Option<u32>,
    dpi: Option<u32>,
    rotation: i32,
    geometry: PageGeometry,
    ocr_page: Option<OcrPage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_windows_file_stems() {
        assert_eq!(sanitize_file_stem("a<b>c:d*e?"), "a_b_c_d_e_");
    }

    #[test]
    fn computes_rotated_pixel_size() {
        assert_eq!(rendered_pixel_size(612.0, 792.0, 0, 200), (1700, 2200));
        assert_eq!(rendered_pixel_size(612.0, 792.0, 90, 200), (2200, 1700));
    }
}
