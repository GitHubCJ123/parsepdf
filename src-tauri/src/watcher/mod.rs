use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::stream::{FuturesUnordered, StreamExt};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, FileIdMap};
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{sync::mpsc, time::sleep};
use tracing::{info, warn};

use crate::{db, ocr::pdf_pipeline};

const WATCH_DEBOUNCE: Duration = Duration::from_secs(2);
const STABILITY_WINDOW: Duration = Duration::from_secs(1);
const DEDUP_WINDOW: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderConfig {
    pub path: PathBuf,
    pub enabled: bool,
    pub recursive: bool,
    pub file_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobOrigin {
    Manual,
    Watch,
}

impl JobOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Watch => "watch",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "watch" => Self::Watch,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IngestJob {
    pub source_path: PathBuf,
    pub origin: JobOrigin,
    pub engine: Option<String>,
}

#[derive(Clone)]
pub struct WatcherService {
    debouncer: Arc<Mutex<Debouncer<RecommendedWatcher, FileIdMap>>>,
    folders: Arc<RwLock<HashMap<PathBuf, FolderConfig>>>,
    excluded_paths: Arc<RwLock<HashSet<PathBuf>>>,
    job_tx: mpsc::Sender<IngestJob>,
    app_handle: AppHandle,
    dedup: Arc<Mutex<DedupCache>>,
    stability_failures: Arc<Mutex<HashMap<PathBuf, u32>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database setup error: {0}")]
    DatabaseSetup(#[from] db::DbError),
    #[error("file IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("watcher error: {0}")]
    Notify(#[from] notify::Error),
    #[error("folder is inside an excluded app folder")]
    ExcludedFolder,
    #[error("folder is not readable")]
    Unreadable,
    #[error("folder is not configured for watching")]
    UnknownFolder,
}

impl WatcherService {
    pub fn new(
        app_handle: AppHandle,
        db_path: &Path,
        job_tx: mpsc::Sender<IngestJob>,
    ) -> Result<Self, WatcherError> {
        let folders = Arc::new(RwLock::new(HashMap::new()));
        let excluded_paths = Arc::new(RwLock::new(resolve_excluded_paths(db_path)?));
        let dedup = Arc::new(Mutex::new(DedupCache::default()));
        let stability_failures = Arc::new(Mutex::new(HashMap::new()));

        let context = WatcherContext {
            excluded_paths: excluded_paths.clone(),
            job_tx: job_tx.clone(),
            app_handle: app_handle.clone(),
            dedup: dedup.clone(),
            stability_failures: stability_failures.clone(),
        };

        let debouncer = new_debouncer(
            WATCH_DEBOUNCE,
            None,
            move |events: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| match events {
                Ok(events) => handle_debounced_events(events, context.clone()),
                Err(errors) => {
                    for error in errors {
                        emit_watcher_error(
                            &context.app_handle,
                            "Watcher backend",
                            &error.to_string(),
                        );
                    }
                }
            },
        )?;

        Ok(Self {
            debouncer: Arc::new(Mutex::new(debouncer)),
            folders,
            excluded_paths,
            job_tx,
            app_handle,
            dedup,
            stability_failures,
        })
    }

    pub async fn startup(&self, db_path: &Path) -> Result<(), WatcherError> {
        self.refresh_excluded_paths(db_path)?;
        let folders = load_folders(db_path)?;
        let mut enabled_paths = Vec::new();
        for folder in folders {
            self.folders
                .write()
                .expect("watcher folders lock poisoned")
                .insert(folder.path.clone(), folder.clone());
            if folder.enabled {
                self.watch_path(&folder.path, folder.recursive)?;
                enabled_paths.push(folder.path);
            }
        }

        let mut recovered = 0_u32;
        for path in enabled_paths {
            recovered += self
                .scan_now(db_path, path.to_string_lossy().into_owned())
                .await?;
        }
        info!(
            folders = self.folder_count(),
            queued = recovered,
            "watcher startup completed"
        );
        Ok(())
    }

    pub async fn add_folder(
        &self,
        db_path: &Path,
        path: String,
        recursive: bool,
    ) -> Result<FolderConfig, WatcherError> {
        self.refresh_excluded_paths(db_path)?;
        let canonical = canonical_dir(&path)?;
        if is_under_excluded_path(&canonical, &self.excluded_snapshot()) {
            return Err(WatcherError::ExcludedFolder);
        }
        fs::read_dir(&canonical).map_err(|_| WatcherError::Unreadable)?;

        let now = now_ts();
        let connection = db::open_connection_at(db_path)?;
        connection.execute(
            "INSERT INTO watched_folders(path, enabled, added_at, recursive, last_error)
             VALUES(?1, 1, ?2, ?3, NULL)
             ON CONFLICT(path) DO UPDATE SET enabled = 1, recursive = excluded.recursive, last_error = NULL",
            params![canonical.to_string_lossy().as_ref(), now, if recursive { 1 } else { 0 }],
        )?;

        let config = FolderConfig {
            file_count: count_pdfs(&canonical, recursive),
            path: canonical.clone(),
            enabled: true,
            recursive,
            last_error: None,
        };
        self.folders
            .write()
            .expect("watcher folders lock poisoned")
            .insert(canonical.clone(), config.clone());
        self.watch_path(&canonical, recursive)?;
        Ok(config)
    }

    pub async fn remove_folder(&self, db_path: &Path, path: String) -> Result<(), WatcherError> {
        let canonical = canonical_dir(&path)?;
        let connection = db::open_connection_at(db_path)?;
        connection.execute(
            "DELETE FROM watched_folders WHERE path = ?1",
            params![canonical.to_string_lossy().as_ref()],
        )?;
        self.debouncer
            .lock()
            .expect("watcher lock poisoned")
            .watcher()
            .unwatch(&canonical)?;
        self.folders
            .write()
            .expect("watcher folders lock poisoned")
            .remove(&canonical);
        Ok(())
    }

    pub async fn list_folders(&self, db_path: &Path) -> Result<Vec<FolderConfig>, WatcherError> {
        self.refresh_excluded_paths(db_path)?;
        let folders = load_folders(db_path)?;
        let mut map = self.folders.write().expect("watcher folders lock poisoned");
        map.clear();
        for folder in &folders {
            map.insert(folder.path.clone(), folder.clone());
        }
        Ok(folders)
    }

    pub async fn set_enabled(
        &self,
        db_path: &Path,
        path: String,
        enabled: bool,
    ) -> Result<(), WatcherError> {
        let canonical = canonical_dir(&path)?;
        let connection = db::open_connection_at(db_path)?;
        let recursive = connection
            .query_row(
                "SELECT recursive FROM watched_folders WHERE path = ?1",
                params![canonical.to_string_lossy().as_ref()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or(WatcherError::UnknownFolder)?
            != 0;
        connection.execute(
            "UPDATE watched_folders SET enabled = ?2, last_error = NULL WHERE path = ?1",
            params![
                canonical.to_string_lossy().as_ref(),
                if enabled { 1 } else { 0 }
            ],
        )?;
        if enabled {
            self.watch_path(&canonical, recursive)?;
        } else {
            self.debouncer
                .lock()
                .expect("watcher lock poisoned")
                .watcher()
                .unwatch(&canonical)?;
        }
        if let Some(config) = self
            .folders
            .write()
            .expect("watcher folders lock poisoned")
            .get_mut(&canonical)
        {
            config.enabled = enabled;
            config.last_error = None;
        }
        Ok(())
    }

    pub async fn scan_now(&self, db_path: &Path, path: String) -> Result<u32, WatcherError> {
        self.refresh_excluded_paths(db_path)?;
        let canonical = canonical_dir(&path)?;
        let recursive =
            watched_folder_recursive(db_path, &canonical)?.ok_or(WatcherError::UnknownFolder)?;
        let candidates = collect_pdf_candidates(&canonical, recursive, &self.excluded_snapshot());
        let context = self.context();
        let mut futures = FuturesUnordered::new();
        for candidate in candidates {
            let context = context.clone();
            futures
                .push(async move { queue_candidate(candidate, JobOrigin::Watch, context).await });
        }

        let mut queued = 0_u32;
        while let Some(result) = futures.next().await {
            if result {
                queued += 1;
            }
        }
        let connection = db::open_connection_at(db_path)?;
        connection.execute(
            "UPDATE watched_folders SET last_scan_at = ?2 WHERE path = ?1",
            params![canonical.to_string_lossy().as_ref(), now_ts()],
        )?;
        Ok(queued)
    }

    fn watch_path(&self, path: &Path, recursive: bool) -> Result<(), WatcherError> {
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        let mut debouncer = self.debouncer.lock().expect("watcher lock poisoned");
        match debouncer.watcher().watch(path, mode) {
            Ok(()) => Ok(()),
            Err(error) => {
                emit_watcher_error(
                    &self.app_handle,
                    &path.to_string_lossy(),
                    &error.to_string(),
                );
                Err(error.into())
            }
        }
    }

    fn refresh_excluded_paths(&self, db_path: &Path) -> Result<(), WatcherError> {
        let paths = resolve_excluded_paths(db_path)?;
        *self
            .excluded_paths
            .write()
            .expect("excluded paths lock poisoned") = paths;
        Ok(())
    }

    fn excluded_snapshot(&self) -> HashSet<PathBuf> {
        self.excluded_paths
            .read()
            .expect("excluded paths lock poisoned")
            .clone()
    }

    fn context(&self) -> WatcherContext {
        WatcherContext {
            excluded_paths: self.excluded_paths.clone(),
            job_tx: self.job_tx.clone(),
            app_handle: self.app_handle.clone(),
            dedup: self.dedup.clone(),
            stability_failures: self.stability_failures.clone(),
        }
    }

    fn folder_count(&self) -> usize {
        self.folders
            .read()
            .expect("watcher folders lock poisoned")
            .len()
    }
}

#[derive(Clone)]
struct WatcherContext {
    excluded_paths: Arc<RwLock<HashSet<PathBuf>>>,
    job_tx: mpsc::Sender<IngestJob>,
    app_handle: AppHandle,
    dedup: Arc<Mutex<DedupCache>>,
    stability_failures: Arc<Mutex<HashMap<PathBuf, u32>>>,
}

fn handle_debounced_events(events: Vec<DebouncedEvent>, context: WatcherContext) {
    for event in events {
        for path in event.paths.iter().cloned() {
            let context = context.clone();
            tauri::async_runtime::spawn(async move {
                let _ = queue_candidate(path, JobOrigin::Watch, context).await;
            });
        }
    }
}

async fn queue_candidate(path: PathBuf, origin: JobOrigin, context: WatcherContext) -> bool {
    let candidate = normalize_path(&path);
    let excluded = context
        .excluded_paths
        .read()
        .expect("excluded paths lock poisoned")
        .clone();
    if is_under_excluded_path(&candidate, &excluded) || !is_candidate_pdf(&candidate) {
        return false;
    }

    match wait_until_stable(&candidate, STABILITY_WINDOW).await {
        Ok(()) => {
            context
                .stability_failures
                .lock()
                .expect("stability lock poisoned")
                .remove(&candidate);
        }
        Err(error) => {
            let failures = record_stability_failure(&context, &candidate);
            if failures >= 3 {
                emit_watcher_error(
                    &context.app_handle,
                    candidate
                        .parent()
                        .map(|parent| parent.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Watched folder".to_string())
                        .as_str(),
                    &format!("File was not stable after repeated checks: {error}"),
                );
            }
            return false;
        }
    }

    let sha = match pdf_pipeline::compute_sha256(&candidate) {
        Ok(sha) => sha,
        Err(error) => {
            warn!(error = %error, "failed to hash watcher candidate");
            return false;
        }
    };
    if !context
        .dedup
        .lock()
        .expect("dedup lock poisoned")
        .remember(candidate.clone(), sha)
    {
        return false;
    }

    context
        .job_tx
        .send(IngestJob {
            source_path: candidate,
            origin,
            engine: None,
        })
        .await
        .is_ok()
}

fn record_stability_failure(context: &WatcherContext, path: &Path) -> u32 {
    let mut failures = context
        .stability_failures
        .lock()
        .expect("stability lock poisoned");
    let count = failures.entry(path.to_path_buf()).or_insert(0);
    *count += 1;
    *count
}

fn is_candidate_pdf(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| !should_ignore_filename(name))
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

pub fn should_ignore_filename(filename: &str) -> bool {
    static IGNORE_RE: OnceLock<Regex> = OnceLock::new();
    let re = IGNORE_RE.get_or_init(|| {
        Regex::new(r"(?i)(^\.|^~\$|\.tmp$|\.crdownload$|\.part$|\.tmp\.pdf$)")
            .expect("valid ignore regex")
    });
    re.is_match(filename)
}

pub fn is_under_excluded_path(path: &Path, excluded_paths: &HashSet<PathBuf>) -> bool {
    let normalized = normalize_path_key(path);
    excluded_paths.iter().any(|excluded| {
        let excluded = normalize_path_key(excluded);
        normalized == excluded || normalized.starts_with(&format!("{excluded}/"))
    })
}

fn normalize_path_key(path: &Path) -> String {
    let mut key = normalize_path(path).to_string_lossy().replace('\\', "/");
    if let Some(stripped) = key.strip_prefix("//?/") {
        key = stripped.to_string();
    }
    while key.ends_with('/') {
        key.pop();
    }
    if cfg!(windows) {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSnapshot {
    pub size: u64,
    pub modified: SystemTime,
}

pub fn snapshots_are_stable(
    first: FileSnapshot,
    second: FileSnapshot,
    elapsed: Duration,
    required: Duration,
) -> bool {
    first == second && elapsed >= required
}

async fn wait_until_stable(path: &Path, required: Duration) -> Result<(), std::io::Error> {
    let first = snapshot(path)?;
    let started = Instant::now();
    sleep(required).await;
    let second = snapshot(path)?;
    if snapshots_are_stable(first, second, started.elapsed(), required) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "file size or modified time is still changing",
        ))
    }
}

fn snapshot(path: &Path) -> Result<FileSnapshot, std::io::Error> {
    let metadata = fs::metadata(path)?;
    Ok(FileSnapshot {
        size: metadata.len(),
        modified: metadata.modified()?,
    })
}

fn load_folders(db_path: &Path) -> Result<Vec<FolderConfig>, WatcherError> {
    let connection = db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(
        "SELECT path, enabled, recursive, last_error FROM watched_folders ORDER BY added_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        let path_text: String = row.get(0)?;
        let path = PathBuf::from(path_text);
        let recursive = row.get::<_, i64>(2)? != 0;
        Ok(FolderConfig {
            file_count: count_pdfs(&path, recursive),
            path,
            enabled: row.get::<_, i64>(1)? != 0,
            recursive,
            last_error: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(WatcherError::from)
}

fn watched_folder_recursive(db_path: &Path, path: &Path) -> Result<Option<bool>, WatcherError> {
    let connection = db::open_connection_at(db_path)?;
    connection
        .query_row(
            "SELECT recursive FROM watched_folders WHERE path = ?1",
            params![path.to_string_lossy().as_ref()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map(|value| value.map(|recursive| recursive != 0))
        .map_err(WatcherError::from)
}

fn collect_pdf_candidates(
    path: &Path,
    recursive: bool,
    excluded: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    collect_pdf_candidates_inner(path, recursive, excluded, &mut candidates);
    candidates
}

fn collect_pdf_candidates_inner(
    path: &Path,
    recursive: bool,
    excluded: &HashSet<PathBuf>,
    candidates: &mut Vec<PathBuf>,
) {
    if is_under_excluded_path(path, excluded) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if is_under_excluded_path(&entry_path, excluded) {
            continue;
        }
        if entry_path.is_dir() && recursive {
            collect_pdf_candidates_inner(&entry_path, recursive, excluded, candidates);
        } else if is_candidate_pdf(&entry_path) {
            candidates.push(normalize_path(&entry_path));
        }
    }
}

fn count_pdfs(path: &Path, recursive: bool) -> u32 {
    collect_pdf_candidates(path, recursive, &HashSet::new()).len() as u32
}

fn canonical_dir(path: &str) -> Result<PathBuf, WatcherError> {
    let canonical = PathBuf::from(path).canonicalize()?;
    if !canonical.is_dir() {
        return Err(WatcherError::Unreadable);
    }
    Ok(canonical)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_excluded_paths(db_path: &Path) -> Result<HashSet<PathBuf>, WatcherError> {
    let mut excluded = HashSet::new();
    if let Ok(output) = pdf_pipeline::resolve_output_dir(db_path) {
        excluded.insert(normalize_path(&output));
    }
    if let Ok(app_data) = db::app_data_dir() {
        for segment in ["engines", "cache", "db"] {
            excluded.insert(normalize_path(&app_data.join(segment)));
        }
    }
    Ok(excluded)
}

#[derive(Default)]
struct DedupCache {
    order: VecDeque<(PathBuf, String)>,
    entries: HashMap<(PathBuf, String), Instant>,
}

impl DedupCache {
    fn remember(&mut self, path: PathBuf, sha: String) -> bool {
        self.prune();
        let key = (path, sha);
        if self.entries.contains_key(&key) {
            return false;
        }
        self.entries.insert(key.clone(), Instant::now());
        self.order.push_back(key);
        true
    }

    fn prune(&mut self) {
        let now = Instant::now();
        while let Some(key) = self.order.front() {
            let expired = self
                .entries
                .get(key)
                .is_none_or(|seen| now.duration_since(*seen) > DEDUP_WINDOW);
            if !expired {
                break;
            }
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct WatcherErrorPayload {
    #[serde(rename = "type")]
    event_type: &'static str,
    folder: String,
    error: String,
}

fn emit_watcher_error(app: &AppHandle, folder: &str, error: &str) {
    let _ = app.emit(
        "watcher.error",
        WatcherErrorPayload {
            event_type: "watcher.error",
            folder: folder.to_string(),
            error: error.to_string(),
        },
    );
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

    #[test]
    fn filename_ignore_patterns_match_expected_inputs() {
        assert!(should_ignore_filename(".tmp.pdf"));
        assert!(should_ignore_filename("~$foo.pdf"));
        assert!(should_ignore_filename("scan.tmp.pdf"));
        assert!(should_ignore_filename("download.crdownload"));
        assert!(!should_ignore_filename("normal PDF.PDF"));
    }

    #[test]
    fn stability_checker_uses_size_mtime_and_elapsed_window() {
        let t0 = UNIX_EPOCH + Duration::from_secs(10);
        let first = FileSnapshot {
            size: 42,
            modified: t0,
        };
        let same = FileSnapshot {
            size: 42,
            modified: t0,
        };
        let changed = FileSnapshot {
            size: 43,
            modified: t0,
        };
        assert!(snapshots_are_stable(
            first,
            same,
            Duration::from_secs(1),
            Duration::from_secs(1)
        ));
        assert!(!snapshots_are_stable(
            first,
            same,
            Duration::from_millis(999),
            Duration::from_secs(1)
        ));
        assert!(!snapshots_are_stable(
            first,
            changed,
            Duration::from_secs(1),
            Duration::from_secs(1)
        ));
    }

    #[test]
    fn excluded_path_logic_matches_nested_output_folder() {
        let output = PathBuf::from(r"C:\Users\jacob\Documents\PDF-Parser\Processed");
        let mut excluded = HashSet::new();
        excluded.insert(output.clone());
        assert!(is_under_excluded_path(
            &output.join("nested").join("done.pdf"),
            &excluded
        ));
        assert!(!is_under_excluded_path(
            Path::new(r"C:\Users\jacob\Documents\Input\done.pdf"),
            &excluded
        ));
    }
}
