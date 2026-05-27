use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use thiserror::Error;
use tokio::sync::{mpsc, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{
    commands::engines::read_default_engine,
    db,
    events::JobLifecycle,
    ocr::{
        pdf_pipeline::{self, DocumentRecord, PipelineError},
        tesseract::TesseractAdapter,
        OcrAdapter,
    },
    state::AppState,
    watcher::{IngestJob, JobOrigin},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Paused,
    Done,
    Error,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "done" => Self::Done,
            "error" => Self::Error,
            "cancelled" => Self::Cancelled,
            _ => Self::Queued,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Ingest,
    Reocr,
    Rename,
    Index,
    Embed,
}

impl JobKind {
    fn from_db(value: &str) -> Self {
        match value {
            "reocr" => Self::Reocr,
            "rename" => Self::Rename,
            "index" => Self::Index,
            "embed" => Self::Embed,
            _ => Self::Ingest,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobFilter {
    pub status: Option<Vec<JobStatus>>,
    pub kind: Option<JobKind>,
    pub since: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    500
}

#[derive(Debug, Clone, Serialize)]
pub struct JobSummary {
    pub id: i64,
    pub document_id: Option<i64>,
    pub filename: String,
    pub original_path: Option<String>,
    pub source: JobOrigin,
    pub kind: JobKind,
    pub status: JobStatus,
    pub stage: String,
    pub progress_pct: f32,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub page_count: i64,
    pub engine: Option<String>,
}

#[derive(Clone)]
pub struct JobController {
    pub job_id: i64,
    pub document_id: Option<i64>,
    pub cancel: CancellationToken,
    pub status: Arc<RwLock<JobStatus>>,
}

#[derive(Clone)]
pub struct JobManager {
    active: Arc<RwLock<HashMap<i64, JobController>>>,
    db_path: PathBuf,
    app: AppHandle,
    state: AppState,
    ingest_tx: mpsc::Sender<IngestJob>,
    run_tx: mpsc::Sender<i64>,
    paused: Arc<AtomicBool>,
    resume_notify: Arc<Notify>,
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database setup error: {0}")]
    DatabaseSetup(#[from] db::DbError),
    #[error("file IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("pipeline error: {0}")]
    Pipeline(#[from] PipelineError),
    #[error("queue is not available")]
    QueueClosed,
    #[error("job not found")]
    NotFound,
    #[error("input path must be a PDF file")]
    NotPdf,
    #[error("source file no longer exists")]
    MissingSource,
    #[error("{0}")]
    Other(String),
}

impl JobManager {
    pub fn new(app: AppHandle, state: AppState) -> Self {
        let (ingest_tx, ingest_rx) = mpsc::channel(512);
        let (run_tx, run_rx) = mpsc::channel(512);
        let manager = Self {
            active: Arc::new(RwLock::new(HashMap::new())),
            db_path: state.db_path.clone(),
            app,
            state,
            ingest_tx,
            run_tx,
            paused: Arc::new(AtomicBool::new(false)),
            resume_notify: Arc::new(Notify::new()),
        };

        let intake_manager = manager.clone();
        tauri::async_runtime::spawn(async move {
            intake_manager.intake_loop(ingest_rx).await;
        });

        let worker_manager = manager.clone();
        tauri::async_runtime::spawn(async move {
            worker_manager.worker_loop(run_rx).await;
        });

        manager
    }

    pub fn ingest_sender(&self) -> mpsc::Sender<IngestJob> {
        self.ingest_tx.clone()
    }

    pub async fn enqueue_ingest(&self, job: IngestJob) -> Result<JobSummary, JobError> {
        let (document_id, job_id) = create_queued_job(
            &self.db_path,
            &canonical_pdf_path(&job.source_path)?,
            job.origin,
            job.engine.as_deref(),
        )?;
        self.run_tx
            .send(job_id)
            .await
            .map_err(|_| JobError::QueueClosed)?;
        self.summary(job_id).await.map(|mut summary| {
            summary.document_id = Some(document_id);
            summary
        })
    }

    pub async fn recover_jobs(&self) -> Result<u32, JobError> {
        let stale_jobs = stale_jobs(&self.db_path)?;
        let mut recovered = 0_u32;
        let mut missing = 0_u32;

        for (job_id, document_id, original_path) in stale_jobs {
            if Path::new(&original_path).is_file() {
                reset_job_for_recovery(&self.db_path, job_id, document_id)?;
                if self.run_tx.send(job_id).await.is_ok() {
                    recovered += 1;
                }
            } else {
                mark_job_error(
                    &self.db_path,
                    job_id,
                    Some(document_id),
                    &format!("Source file no longer exists at {original_path}"),
                )?;
                missing += 1;
            }
        }

        info!(recovered, missing, "job recovery completed");
        Ok(recovered)
    }

    pub async fn cancel(&self, job_id: i64) -> Result<(), JobError> {
        if let Some(controller) = self.active.read().await.get(&job_id).cloned() {
            controller.cancel.cancel();
            *controller.status.write().await = JobStatus::Cancelled;
        }
        self.resume_notify.notify_waiters();
        set_job_status(&self.db_path, job_id, JobStatus::Cancelled, None, true)?;
        self.state.progress.notify_lifecycle(JobLifecycle::new(
            job_id,
            job_document_id(&self.db_path, job_id).ok().flatten(),
            JobStatus::Cancelled.as_str(),
            Some("Job cancelled".to_string()),
        ));
        Ok(())
    }

    pub async fn cancel_all(&self) -> Result<u32, JobError> {
        let ids = jobs_with_statuses(
            &self.db_path,
            &[JobStatus::Queued, JobStatus::Running, JobStatus::Paused],
        )?;
        for job_id in &ids {
            let _ = self.cancel(*job_id).await;
        }
        Ok(ids.len() as u32)
    }

    pub async fn pause_all(&self) -> Result<u32, JobError> {
        self.paused.store(true, Ordering::SeqCst);
        let count = update_queued_to_paused(&self.db_path)?;
        Ok(count)
    }

    pub async fn resume_all(&self) -> Result<u32, JobError> {
        self.paused.store(false, Ordering::SeqCst);
        self.resume_notify.notify_waiters();
        let ids = resume_paused_jobs(&self.db_path)?;
        for job_id in &ids {
            let _ = self.run_tx.send(*job_id).await;
        }
        Ok(ids.len() as u32)
    }

    pub async fn retry(&self, job_id: i64) -> Result<(), JobError> {
        let (document_id, original_path) =
            job_source(&self.db_path, job_id)?.ok_or(JobError::NotFound)?;
        if !Path::new(&original_path).is_file() {
            mark_job_error(
                &self.db_path,
                job_id,
                Some(document_id),
                &format!("Source file no longer exists at {original_path}"),
            )?;
            return Err(JobError::MissingSource);
        }
        reset_job_for_retry(&self.db_path, job_id, document_id)?;
        self.run_tx
            .send(job_id)
            .await
            .map_err(|_| JobError::QueueClosed)
    }

    pub async fn list(&self) -> Vec<JobSummary> {
        self.list_filtered(JobFilter {
            status: None,
            kind: None,
            since: None,
            limit: default_limit(),
        })
        .await
        .unwrap_or_default()
    }

    pub async fn list_filtered(&self, filter: JobFilter) -> Result<Vec<JobSummary>, JobError> {
        let mut rows = list_jobs(&self.db_path, filter.since, filter.limit.clamp(1, 2_000))?;
        if let Some(status) = filter.status {
            rows.retain(|job| status.contains(&job.status));
        }
        if let Some(kind) = filter.kind {
            rows.retain(|job| job.kind == kind);
        }
        Ok(rows)
    }

    pub async fn clear_completed(&self) -> Result<u32, JobError> {
        let connection = db::open_connection_at(&self.db_path)?;
        let deleted =
            connection.execute("DELETE FROM jobs WHERE status IN ('done', 'cancelled')", [])?;
        Ok(deleted as u32)
    }

    async fn intake_loop(&self, mut rx: mpsc::Receiver<IngestJob>) {
        while let Some(job) = rx.recv().await {
            if let Err(error) = self.enqueue_ingest(job).await {
                warn!(error = %error, "failed to enqueue watcher job");
            }
        }
    }

    async fn worker_loop(&self, mut rx: mpsc::Receiver<i64>) {
        while let Some(job_id) = rx.recv().await {
            if let Err(error) = self.run_job(job_id).await {
                warn!(job_id, error = %error, "job execution failed");
            }
        }
    }

    async fn run_job(&self, job_id: i64) -> Result<(), JobError> {
        self.wait_until_runnable(job_id).await?;
        let Some(job) = load_runnable_job(&self.db_path, job_id)? else {
            return Ok(());
        };

        set_job_running(&self.db_path, job_id)?;
        let cancel = CancellationToken::new();
        let controller = JobController {
            job_id,
            document_id: Some(job.document_id),
            cancel: cancel.clone(),
            status: Arc::new(RwLock::new(JobStatus::Running)),
        };
        self.active.write().await.insert(job_id, controller);
        self.state.progress.notify_lifecycle(JobLifecycle::new(
            job_id,
            Some(job.document_id),
            JobStatus::Running.as_str(),
            Some("Job started".to_string()),
        ));

        let result = self.process_runnable_job(job.clone(), cancel.clone()).await;
        self.active.write().await.remove(&job_id);

        match result {
            Ok(_) => {
                finish_job(&self.db_path, job_id, JobStatus::Done, None)?;
                self.state.progress.notify_lifecycle(JobLifecycle::new(
                    job_id,
                    job_document_id(&self.db_path, job_id).ok().flatten(),
                    JobStatus::Done.as_str(),
                    Some("Job finished".to_string()),
                ));
            }
            Err(JobError::Pipeline(PipelineError::Cancelled)) => {
                finish_job(&self.db_path, job_id, JobStatus::Cancelled, None)?;
                update_document_status(
                    &self.db_path,
                    job.document_id,
                    JobStatus::Cancelled.as_str(),
                    None,
                )?;
                self.state.progress.notify_lifecycle(JobLifecycle::new(
                    job_id,
                    Some(job.document_id),
                    JobStatus::Cancelled.as_str(),
                    Some("Job cancelled".to_string()),
                ));
            }
            Err(error) => {
                let message = error.to_string();
                mark_job_error(&self.db_path, job_id, Some(job.document_id), &message)?;
                self.state.progress.notify_lifecycle(JobLifecycle::new(
                    job_id,
                    Some(job.document_id),
                    JobStatus::Error.as_str(),
                    Some(message),
                ));
            }
        }

        Ok(())
    }

    async fn process_runnable_job(
        &self,
        job: RunnableJob,
        cancel: CancellationToken,
    ) -> Result<DocumentRecord, JobError> {
        let input_path = canonical_pdf_path(Path::new(&job.original_path))?;
        let output_dir = pdf_pipeline::resolve_output_dir(&self.db_path)?;
        let requested_engine = match job
            .engine
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(engine) => engine.to_string(),
            None => read_default_engine(&self.db_path)?,
        };
        set_job_engine(&self.db_path, job.job_id, &requested_engine)?;
        let engine = build_ocr_adapter(&self.app, &requested_engine).await?;

        Ok(pdf_pipeline::process_pdf(
            self.app.clone(),
            self.state.clone(),
            input_path,
            output_dir,
            job.document_id,
            job.job_id,
            engine,
            cancel,
        )
        .await?)
    }

    async fn wait_until_runnable(&self, job_id: i64) -> Result<(), JobError> {
        loop {
            let status = current_job_status(&self.db_path, job_id)?.ok_or(JobError::NotFound)?;
            if status == JobStatus::Cancelled
                || status == JobStatus::Done
                || status == JobStatus::Error
            {
                return Err(JobError::Other("job is not runnable".to_string()));
            }
            if !self.paused.load(Ordering::SeqCst) && status == JobStatus::Queued {
                return Ok(());
            }
            self.resume_notify.notified().await;
        }
    }

    async fn summary(&self, job_id: i64) -> Result<JobSummary, JobError> {
        list_jobs(&self.db_path, None, 2_000)?
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or(JobError::NotFound)
    }
}

async fn build_ocr_adapter(
    app: &AppHandle,
    engine_id: &str,
) -> Result<Arc<dyn OcrAdapter>, JobError> {
    match engine_id {
        "tesseract" => TesseractAdapter::new(app.clone())
            .map(|adapter| Arc::new(adapter) as Arc<dyn OcrAdapter>)
            .map_err(|error| JobError::Other(error.to_string())),
        "rapidocr" => build_rapidocr_adapter().await,
        other => Err(JobError::Other(format!("unknown OCR engine: {other}"))),
    }
}

#[cfg(feature = "rapidocr")]
async fn build_rapidocr_adapter() -> Result<Arc<dyn OcrAdapter>, JobError> {
    use crate::ocr::{rapidocr::RapidOcrAdapter, rapidocr_install::default_rapidocr_dir};

    let models_dir = default_rapidocr_dir().map_err(|error| JobError::Other(error.to_string()))?;
    let adapter = RapidOcrAdapter::new(models_dir);
    adapter
        .verify_install()
        .await
        .map_err(|error| JobError::Other(error.to_string()))?;
    Ok(Arc::new(adapter) as Arc<dyn OcrAdapter>)
}

#[cfg(not(feature = "rapidocr"))]
async fn build_rapidocr_adapter() -> Result<Arc<dyn OcrAdapter>, JobError> {
    Err(JobError::Other(
        "RapidOCR support is not enabled in this build".to_string(),
    ))
}

#[derive(Clone)]
struct RunnableJob {
    job_id: i64,
    document_id: i64,
    original_path: String,
    engine: Option<String>,
}

fn create_queued_job(
    db_path: &Path,
    input_path: &Path,
    origin: JobOrigin,
    engine: Option<&str>,
) -> Result<(i64, i64), JobError> {
    let sha256 = pdf_pipeline::compute_sha256(input_path)?;
    let mut connection = db::open_connection_at(db_path)?;
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
                 display_name = NULL,
                 ai_summary = NULL,
                 page_count = 0,
                 ocr_engine = NULL,
                 ai_provider = NULL,
                 ai_naming_enabled = 0,
                 status = 'queued',
                 error_message = NULL,
                 updated_at = ?3
             WHERE id = ?1",
            params![document_id, original_path, now],
        )?;
        let old_chunk_ids = {
            let mut statement =
                transaction.prepare("SELECT id FROM chunks WHERE document_id = ?1")?;
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
        transaction.execute(
            "DELETE FROM pending_renames WHERE document_id = ?1",
            params![document_id],
        )?;
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
        "INSERT INTO jobs(document_id, kind, status, created_at, origin, engine)
         VALUES(?1, 'ingest', 'queued', ?2, ?3, ?4)",
        params![document_id, now, origin.as_str(), engine],
    )?;
    let job_id = transaction.last_insert_rowid();
    transaction.commit()?;
    Ok((document_id, job_id))
}

fn canonical_pdf_path(path: &Path) -> Result<PathBuf, JobError> {
    let canonical = path.canonicalize()?;
    if !canonical.is_file() {
        return Err(JobError::MissingSource);
    }
    if canonical
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(JobError::NotPdf);
    }
    Ok(canonical)
}

fn load_runnable_job(db_path: &Path, job_id: i64) -> Result<Option<RunnableJob>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection
        .query_row(
            "SELECT j.id, d.id, d.original_path, j.engine
             FROM jobs j
             JOIN documents d ON d.id = j.document_id
             WHERE j.id = ?1 AND j.status = 'queued'",
            params![job_id],
            |row| {
                Ok(RunnableJob {
                    job_id: row.get(0)?,
                    document_id: row.get(1)?,
                    original_path: row.get(2)?,
                    engine: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(JobError::from)
}

fn set_job_running(db_path: &Path, job_id: i64) -> Result<(), JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE jobs SET status = 'running', error_message = NULL, started_at = ?2, finished_at = NULL WHERE id = ?1",
        params![job_id, now_ts()],
    )?;
    Ok(())
}

fn finish_job(
    db_path: &Path,
    job_id: i64,
    status: JobStatus,
    error_message: Option<&str>,
) -> Result<(), JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE jobs
         SET status = ?2,
             error_message = ?3,
             finished_at = ?4
         WHERE id = ?1",
        params![job_id, status.as_str(), error_message, now_ts()],
    )?;
    Ok(())
}

fn set_job_status(
    db_path: &Path,
    job_id: i64,
    status: JobStatus,
    error_message: Option<&str>,
    finished: bool,
) -> Result<(), JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE jobs
         SET status = ?2,
             error_message = ?3,
             finished_at = CASE WHEN ?4 THEN ?5 ELSE finished_at END
         WHERE id = ?1",
        params![job_id, status.as_str(), error_message, finished, now_ts()],
    )?;
    Ok(())
}

fn set_job_engine(db_path: &Path, job_id: i64, engine: &str) -> Result<(), JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE jobs SET engine = ?2 WHERE id = ?1",
        params![job_id, engine],
    )?;
    Ok(())
}

fn mark_job_error(
    db_path: &Path,
    job_id: i64,
    document_id: Option<i64>,
    message: &str,
) -> Result<(), JobError> {
    finish_job(db_path, job_id, JobStatus::Error, Some(message))?;
    if let Some(document_id) = document_id {
        update_document_status(db_path, document_id, "error", Some(message))?;
    }
    Ok(())
}

fn update_document_status(
    db_path: &Path,
    document_id: i64,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection.execute(
        "UPDATE documents
         SET status = ?2,
             error_message = ?3,
             updated_at = ?4
         WHERE id = ?1",
        params![document_id, status, error_message, now_ts()],
    )?;
    Ok(())
}

fn current_job_status(db_path: &Path, job_id: i64) -> Result<Option<JobStatus>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection
        .query_row(
            "SELECT status FROM jobs WHERE id = ?1",
            params![job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map(|status| status.map(|value| JobStatus::from_db(&value)))
        .map_err(JobError::from)
}

fn job_document_id(db_path: &Path, job_id: i64) -> Result<Option<i64>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection
        .query_row(
            "SELECT document_id FROM jobs WHERE id = ?1",
            params![job_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map(|value| value.flatten())
        .map_err(JobError::from)
}

fn jobs_with_statuses(db_path: &Path, statuses: &[JobStatus]) -> Result<Vec<i64>, JobError> {
    let wanted = statuses
        .iter()
        .map(|status| status.as_str())
        .collect::<Vec<_>>();
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare("SELECT id, status FROM jobs")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, status)| wanted.contains(&status.as_str()).then_some(id))
        .collect())
}

fn update_queued_to_paused(db_path: &Path) -> Result<u32, JobError> {
    let connection = db::open_connection_at(db_path)?;
    let changed = connection.execute(
        "UPDATE jobs SET status = 'paused' WHERE status = 'queued'",
        [],
    )?;
    Ok(changed as u32)
}

fn resume_paused_jobs(db_path: &Path) -> Result<Vec<i64>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare("SELECT id FROM jobs WHERE status = 'paused'")?;
    let ids = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    connection.execute(
        "UPDATE jobs SET status = 'queued' WHERE status = 'paused'",
        [],
    )?;
    Ok(ids)
}

fn job_source(db_path: &Path, job_id: i64) -> Result<Option<(i64, String)>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    connection
        .query_row(
            "SELECT d.id, d.original_path
             FROM jobs j JOIN documents d ON d.id = j.document_id
             WHERE j.id = ?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(JobError::from)
}

fn reset_job_for_retry(db_path: &Path, job_id: i64, document_id: i64) -> Result<(), JobError> {
    let mut connection = db::open_connection_at(db_path)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE jobs
         SET status = 'queued', error_message = NULL, started_at = NULL, finished_at = NULL
         WHERE id = ?1",
        params![job_id],
    )?;
    transaction.execute(
        "UPDATE documents
         SET status = 'queued', output_path = NULL, error_message = NULL, page_count = 0, updated_at = ?2
         WHERE id = ?1",
        params![document_id, now_ts()],
    )?;
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
    transaction.execute(
        "DELETE FROM pending_renames WHERE document_id = ?1",
        params![document_id],
    )?;
    transaction.commit()?;
    Ok(())
}

fn reset_job_for_recovery(db_path: &Path, job_id: i64, document_id: i64) -> Result<(), JobError> {
    reset_job_for_retry(db_path, job_id, document_id)
}

fn stale_jobs(db_path: &Path) -> Result<Vec<(i64, i64, String)>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(
        "SELECT j.id, d.id, d.original_path
         FROM jobs j JOIN documents d ON d.id = j.document_id
         WHERE j.status IN ('running', 'queued')",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(JobError::from)
}

fn list_jobs(db_path: &Path, since: Option<i64>, limit: u32) -> Result<Vec<JobSummary>, JobError> {
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(
        "SELECT j.id,
                j.document_id,
                j.kind,
                j.status,
                j.error_message,
                j.started_at,
                j.finished_at,
                j.created_at,
                COALESCE(j.origin, 'manual'),
                j.engine,
                d.original_path,
                d.status,
                COALESCE(d.page_count, 0)
         FROM jobs j
         LEFT JOIN documents d ON d.id = j.document_id
         WHERE (?1 IS NULL OR j.created_at >= ?1)
         ORDER BY j.created_at DESC, j.id DESC
         LIMIT ?2",
    )?;

    let rows = statement.query_map(params![since, limit], |row| {
        let kind_text: String = row.get(2)?;
        let status_text: String = row.get(3)?;
        let origin_text: String = row.get(8)?;
        let original_path = row.get::<_, Option<String>>(10)?;
        let filename = original_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("PDF")
            .to_string();
        let status = JobStatus::from_db(&status_text);
        let document_status = row
            .get::<_, Option<String>>(11)?
            .unwrap_or_else(|| status.as_str().to_string());
        Ok(JobSummary {
            id: row.get(0)?,
            document_id: row.get(1)?,
            filename,
            original_path,
            source: JobOrigin::from_db(&origin_text),
            kind: JobKind::from_db(&kind_text),
            status,
            stage: document_status,
            progress_pct: default_progress(status),
            error_message: row.get(4)?,
            started_at: row.get(5)?,
            finished_at: row.get(6)?,
            created_at: row.get(7)?,
            page_count: row.get(12)?,
            engine: row.get(9)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(JobError::from)
}

fn default_progress(status: JobStatus) -> f32 {
    match status {
        JobStatus::Done => 100.0,
        JobStatus::Error | JobStatus::Cancelled => 100.0,
        _ => 0.0,
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn cancellation_token_completes_mock_job_within_two_seconds() {
        let cancel = CancellationToken::new();
        let child = cancel.child_token();
        let handle = tokio::spawn(async move {
            loop {
                if child.is_cancelled() {
                    return JobStatus::Cancelled;
                }
                sleep(Duration::from_millis(25)).await;
            }
        });

        cancel.cancel();
        let status = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("mock job timed out")
            .expect("mock job join failed");
        assert_eq!(status, JobStatus::Cancelled);
    }
}
