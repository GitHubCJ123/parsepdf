use std::{
    env,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use super::rapidocr_manifest::{ModelFile, ModelManifest};

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<String>,
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("RapidOCR model URL must use HTTPS: {0}")]
    NonHttpsUrl(String),
    #[error("RapidOCR model file is missing: {path:?}")]
    MissingFile { path: PathBuf },
    #[error("RapidOCR model download was rejected: {relative_path} returned HTTP {status}")]
    DownloadRejected { relative_path: String, status: u16 },
    #[error("RapidOCR model file has unexpected size: {path:?} expected {expected} bytes, got {actual} bytes")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    #[error("RapidOCR model file hash mismatch: {path:?} expected {expected}, got {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("invalid RapidOCR manifest path: {0}")]
    InvalidManifestPath(String),
    #[error("failed to resolve LOCALAPPDATA for RapidOCR engines")]
    MissingLocalAppData,
    #[error("HTTP error while downloading RapidOCR models: {0}")]
    Http(#[from] reqwest::Error),
    #[error("file IO error while installing RapidOCR models: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to write RapidOCR install marker: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct InstalledMarker {
    version: String,
    completed_at_unix: u64,
    /// Map of relative_path -> recorded {sha256, size} captured during install.
    /// Used for trust-on-first-use verification of files whose pinned hashes
    /// were not known at build time.
    #[serde(default)]
    files: std::collections::BTreeMap<String, RecordedFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RecordedFile {
    sha256: String,
    size: u64,
}

pub async fn install_rapidocr(
    target_dir: &Path,
    manifest: &ModelManifest,
    progress: impl Fn(InstallProgress) + Send + Sync + 'static,
) -> Result<(), InstallError> {
    fs::create_dir_all(target_dir)?;
    validate_manifest(manifest)?;

    tracing::info!(
        target: "pdf_parser_lib::ocr::rapidocr_install",
        version = manifest.version,
        file_count = manifest.files.len(),
        "installing RapidOCR models"
    );

    let client = reqwest::Client::builder()
        // The modelscope CDN returns 403 for requests without a User-Agent, so
        // we must send one or every download fails before a byte is written.
        .user_agent(concat!("PDF-Parser/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    // For pinned files we know the size; otherwise we'll discover sizes from
    // Content-Length headers, so total_bytes starts at 0 and grows as we go.
    let known_total = manifest
        .files
        .iter()
        .map(|file| file.size.unwrap_or(0))
        .sum::<u64>();
    let mut total_bytes = known_total.max(1);
    let mut aggregate_done = 0_u64;

    let mut recorded = read_marker(target_dir).unwrap_or_default();
    recorded.files.retain(|key, _| {
        manifest
            .files
            .iter()
            .any(|file| file.relative_path == key.as_str())
    });

    progress(InstallProgress {
        phase: "starting".to_string(),
        bytes_done: 0,
        bytes_total: total_bytes,
        current_file: None,
    });

    for file in manifest.files {
        let final_path = manifest_file_path(target_dir, file)?;
        if final_path.exists() {
            // Keep an already-downloaded file only if it still verifies, and
            // always record its hash+size so the written marker is complete
            // (manifest files are unpinned, so the marker is the only source of
            // truth the fast install check can trust later).
            if let Ok(record) =
                record_existing_file(&final_path, file, recorded.files.get(file.relative_path))
            {
                aggregate_done = aggregate_done.saturating_add(record.size);
                total_bytes = total_bytes.max(aggregate_done);
                recorded
                    .files
                    .insert(file.relative_path.to_string(), record);
                progress(InstallProgress {
                    phase: "verified".to_string(),
                    bytes_done: aggregate_done,
                    bytes_total: total_bytes,
                    current_file: Some(file.relative_path.to_string()),
                });
                continue;
            }
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp_path = final_path.with_extension(format!(
            "{}.tmp",
            final_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("download")
        ));
        if tmp_path.exists() {
            fs::remove_file(&tmp_path)?;
        }

        progress(InstallProgress {
            phase: "downloading".to_string(),
            bytes_done: aggregate_done,
            bytes_total: total_bytes,
            current_file: Some(file.relative_path.to_string()),
        });

        let response = client
            .get(file.url)
            .send()
            .await
            .map_err(InstallError::Http)?;
        // Handle HTTP status failures explicitly so we never surface or log the
        // signed CDN redirect URL (it carries a short-lived auth_key).
        if let Err(error) = response.error_for_status_ref() {
            let status = error.status().map(|code| code.as_u16()).unwrap_or(0);
            let _ = fs::remove_file(&tmp_path);
            tracing::warn!(
                target: "pdf_parser_lib::ocr::rapidocr_install",
                relative_path = file.relative_path,
                status,
                "RapidOCR model download was rejected by the server"
            );
            return Err(InstallError::DownloadRejected {
                relative_path: file.relative_path.to_string(),
                status,
            });
        }
        if let Some(content_length) = response.content_length() {
            total_bytes = total_bytes.max(aggregate_done.saturating_add(content_length));
        }
        let mut body = response.bytes_stream();
        let mut tmp_file = tokio::fs::File::create(&tmp_path).await?;
        let mut hasher = Sha256::new();
        let mut file_done = 0_u64;
        // Throttle per-chunk progress: emitting a Tauri event for every chunk of
        // a ~179 MB download floods IPC and triggers thousands of React renders,
        // which makes the window appear to hang. Phase/terminal events below are
        // never throttled, so the UI always settles on the final state.
        let mut last_emit = std::time::Instant::now();
        let mut last_pct = u64::MAX;

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            file_done = file_done.saturating_add(chunk.len() as u64);
            hasher.update(&chunk);
            tmp_file.write_all(&chunk).await?;
            total_bytes = total_bytes.max(aggregate_done.saturating_add(file_done));
            let bytes_done = aggregate_done.saturating_add(file_done);
            let pct = bytes_done.saturating_mul(100) / total_bytes.max(1);
            let now = std::time::Instant::now();
            if pct != last_pct || now.duration_since(last_emit) >= Duration::from_millis(200) {
                last_pct = pct;
                last_emit = now;
                progress(InstallProgress {
                    phase: "downloading".to_string(),
                    bytes_done,
                    bytes_total: total_bytes,
                    current_file: Some(file.relative_path.to_string()),
                });
            }
        }
        tmp_file.flush().await?;
        drop(tmp_file);

        let actual_hash = format!("{:x}", hasher.finalize());
        // Pinned-hash mismatch is always a hard failure.
        if let Some(expected) = file.sha256 {
            if !actual_hash.eq_ignore_ascii_case(expected) {
                let _ = fs::remove_file(&tmp_path);
                return Err(InstallError::HashMismatch {
                    path: tmp_path,
                    expected: expected.to_string(),
                    actual: actual_hash,
                });
            }
        }
        if let Some(expected_size) = file.size {
            if file_done != expected_size {
                let _ = fs::remove_file(&tmp_path);
                return Err(InstallError::SizeMismatch {
                    path: tmp_path,
                    expected: expected_size,
                    actual: file_done,
                });
            }
        }
        // Sanity-check the file is large enough to plausibly be a model.
        if file_done < 1024 {
            let _ = fs::remove_file(&tmp_path);
            return Err(InstallError::SizeMismatch {
                path: tmp_path,
                expected: 1024,
                actual: file_done,
            });
        }

        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(&tmp_path, &final_path)?;
        recorded.files.insert(
            file.relative_path.to_string(),
            RecordedFile {
                sha256: actual_hash,
                size: file_done,
            },
        );
        aggregate_done = aggregate_done.saturating_add(file_done);
        progress(InstallProgress {
            phase: "verified".to_string(),
            bytes_done: aggregate_done,
            bytes_total: total_bytes,
            current_file: Some(file.relative_path.to_string()),
        });
    }

    write_marker(target_dir, manifest, &recorded.files)?;
    tracing::info!(
        target: "pdf_parser_lib::ocr::rapidocr_install",
        version = manifest.version,
        bytes_total = total_bytes,
        "RapidOCR models installed"
    );
    progress(InstallProgress {
        phase: "complete".to_string(),
        bytes_done: total_bytes,
        bytes_total: total_bytes,
        current_file: None,
    });
    Ok(())
}

pub fn verify_install_dir(target_dir: &Path, manifest: &ModelManifest) -> Result<(), InstallError> {
    validate_manifest(manifest)?;
    let recorded = read_marker(target_dir).unwrap_or_default();
    for file in manifest.files {
        let path = manifest_file_path(target_dir, file)?;
        verify_model_file(&path, file, recorded.files.get(file.relative_path))?;
    }
    Ok(())
}

/// Fast install check for hot paths (UI status polls and job-start guards).
/// Confirms an install looks complete *without* hashing file contents:
/// - the `.installed.json` marker exists and its version matches the manifest,
/// - every manifest file exists on disk,
/// - each file's size matches its pinned size, otherwise the size recorded in
///   the marker (required, because the manifest files are unpinned).
///
/// A full SHA256 verification still runs once when the models are actually
/// loaded for OCR, so this stays an inexpensive "is it installed?" probe that
/// can be called on every status refresh without thrashing the disk.
pub fn quick_check_install_dir(
    target_dir: &Path,
    manifest: &ModelManifest,
) -> Result<(), InstallError> {
    validate_manifest(manifest)?;
    let marker_path = target_dir.join(".installed.json");
    let marker = read_marker(target_dir).ok_or_else(|| InstallError::MissingFile {
        path: marker_path.clone(),
    })?;
    // A marker for a different version means the install is stale/incomplete.
    if marker.version != manifest.version {
        return Err(InstallError::MissingFile { path: marker_path });
    }
    for file in manifest.files {
        let path = manifest_file_path(target_dir, file)?;
        if !path.exists() {
            return Err(InstallError::MissingFile { path });
        }
        let expected_size = file
            .size
            .or_else(|| marker.files.get(file.relative_path).map(|record| record.size));
        let Some(expected) = expected_size else {
            // No pinned or recorded size for this file => incomplete marker.
            return Err(InstallError::MissingFile { path });
        };
        let actual = fs::metadata(&path)?.len();
        if actual != expected {
            return Err(InstallError::SizeMismatch {
                path,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

pub fn default_rapidocr_dir() -> Result<PathBuf, InstallError> {
    if let Some(local_appdata) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local_appdata)
            .join("PDF-Parser")
            .join("engines")
            .join("rapidocr"));
    }
    dirs::data_local_dir()
        .map(|path| path.join("PDF-Parser").join("engines").join("rapidocr"))
        .ok_or(InstallError::MissingLocalAppData)
}

pub fn manifest_file_path(target_dir: &Path, file: &ModelFile) -> Result<PathBuf, InstallError> {
    safe_relative_path(file.relative_path).map(|relative| target_dir.join(relative))
}

pub fn sha256_file(path: &Path) -> Result<String, InstallError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_manifest(manifest: &ModelManifest) -> Result<(), InstallError> {
    for file in manifest.files {
        if !file.url.starts_with("https://") {
            return Err(InstallError::NonHttpsUrl(file.url.to_string()));
        }
        let _ = safe_relative_path(file.relative_path)?;
    }
    Ok(())
}

fn verify_model_file(
    path: &Path,
    file: &ModelFile,
    recorded: Option<&RecordedFile>,
) -> Result<(), InstallError> {
    if !path.exists() {
        return Err(InstallError::MissingFile {
            path: path.to_path_buf(),
        });
    }
    let actual_size = fs::metadata(path)?.len();
    // Determine the effective expected size: pinned > recorded > skip.
    let expected_size = file.size.or_else(|| recorded.map(|r| r.size));
    if let Some(expected) = expected_size {
        if actual_size != expected {
            return Err(InstallError::SizeMismatch {
                path: path.to_path_buf(),
                expected,
                actual: actual_size,
            });
        }
    }
    // Determine the effective expected hash: pinned > recorded > skip.
    let expected_hash = file
        .sha256
        .map(str::to_string)
        .or_else(|| recorded.map(|r| r.sha256.clone()));
    if let Some(expected) = expected_hash {
        let actual_hash = sha256_file(path)?;
        if !actual_hash.eq_ignore_ascii_case(&expected) {
            return Err(InstallError::HashMismatch {
                path: path.to_path_buf(),
                expected,
                actual: actual_hash,
            });
        }
    }
    Ok(())
}

/// Verify an already-present file and return the marker entry to persist for it.
/// Reuses a pinned or previously recorded hash when available; otherwise hashes
/// the file once (trust-on-first-use) so the written marker always carries a
/// hash+size for every kept file.
fn record_existing_file(
    path: &Path,
    file: &ModelFile,
    recorded: Option<&RecordedFile>,
) -> Result<RecordedFile, InstallError> {
    verify_model_file(path, file, recorded)?;
    let size = fs::metadata(path)?.len();
    let sha256 = match file.sha256 {
        Some(hash) => hash.to_string(),
        None => match recorded {
            Some(record) => record.sha256.clone(),
            None => sha256_file(path)?,
        },
    };
    Ok(RecordedFile { sha256, size })
}

fn safe_relative_path(relative_path: &str) -> Result<PathBuf, InstallError> {
    let path = Path::new(relative_path);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            _ => return Err(InstallError::InvalidManifestPath(relative_path.to_string())),
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(InstallError::InvalidManifestPath(relative_path.to_string()));
    }
    Ok(safe)
}

fn read_marker(target_dir: &Path) -> Option<InstalledMarker> {
    let raw = fs::read(target_dir.join(".installed.json")).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn write_marker(
    target_dir: &Path,
    manifest: &ModelManifest,
    files: &std::collections::BTreeMap<String, RecordedFile>,
) -> Result<(), InstallError> {
    let marker = InstalledMarker {
        version: manifest.version.to_string(),
        completed_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        files: files.clone(),
    };
    let json = serde_json::to_vec_pretty(&marker)?;
    fs::write(target_dir.join(".installed.json"), json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::ocr::rapidocr_manifest::{ModelFile, ModelManifest};

    #[test]
    fn verify_manifest_accepts_clean_install_dir() {
        let dir = test_dir("rapidocr-clean");
        let bytes = b"known-good-model";
        let hash = Box::leak(hex(bytes).into_boxed_str());
        let files = Box::leak(
            vec![ModelFile {
                url: "https://example.com/model.onnx",
                relative_path: "det/model.onnx",
                sha256: Some(hash),
                size: Some(bytes.len() as u64),
            }]
            .into_boxed_slice(),
        );
        let manifest = ModelManifest {
            version: "test",
            total_size_mb: 1,
            files,
        };
        let path = dir.join("det").join("model.onnx");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();

        verify_install_dir(&dir, &manifest).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_manifest_rejects_tampered_file() {
        let dir = test_dir("rapidocr-tampered");
        let good = b"original-model";
        let hash = Box::leak(hex(good).into_boxed_str());
        let files = Box::leak(
            vec![ModelFile {
                url: "https://example.com/model.onnx",
                relative_path: "rec/model.onnx",
                sha256: Some(hash),
                size: Some(good.len() as u64),
            }]
            .into_boxed_slice(),
        );
        let manifest = ModelManifest {
            version: "test",
            total_size_mb: 1,
            files,
        };
        let path = dir.join("rec").join("model.onnx");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"tampered-model").unwrap();

        let error = verify_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(
            error,
            InstallError::SizeMismatch { .. } | InstallError::HashMismatch { .. }
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_manifest_rejects_missing_model() {
        let dir = test_dir("rapidocr-missing");
        let hash = Box::leak(hex(b"missing-model").into_boxed_str());
        let files = Box::leak(
            vec![ModelFile {
                url: "https://example.com/model.onnx",
                relative_path: "cls/model.onnx",
                sha256: Some(hash),
                size: Some(13),
            }]
            .into_boxed_slice(),
        );
        let manifest = ModelManifest {
            version: "test",
            total_size_mb: 1,
            files,
        };

        let error = verify_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(error, InstallError::MissingFile { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    fn hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn unpinned_manifest(rel: &'static str, version: &'static str) -> ModelManifest {
        let files = Box::leak(
            vec![ModelFile {
                url: "https://example.com/model.onnx",
                relative_path: rel,
                sha256: None,
                size: None,
            }]
            .into_boxed_slice(),
        );
        ModelManifest {
            version,
            total_size_mb: 1,
            files,
        }
    }

    fn write_marker_with(dir: &Path, version: &str, rel: &str, sha256: String, size: u64) {
        let mut files = std::collections::BTreeMap::new();
        files.insert(rel.to_string(), RecordedFile { sha256, size });
        let marker = InstalledMarker {
            version: version.to_string(),
            completed_at_unix: 0,
            files,
        };
        fs::write(
            dir.join(".installed.json"),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn quick_check_accepts_complete_install() {
        let dir = test_dir("quick-complete");
        let rel = "det/model.onnx";
        let bytes = b"a-real-enough-model";
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        write_marker_with(&dir, "v1", rel, hex(bytes), bytes.len() as u64);

        let manifest = unpinned_manifest(rel, "v1");
        quick_check_install_dir(&dir, &manifest).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quick_check_reports_missing_without_marker() {
        let dir = test_dir("quick-no-marker");
        let rel = "det/model.onnx";
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"present-but-unmarked").unwrap();

        let manifest = unpinned_manifest(rel, "v1");
        let error = quick_check_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(error, InstallError::MissingFile { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quick_check_reports_missing_on_version_mismatch() {
        let dir = test_dir("quick-version");
        let rel = "det/model.onnx";
        let bytes = b"model-bytes";
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        write_marker_with(&dir, "old-version", rel, hex(bytes), bytes.len() as u64);

        let manifest = unpinned_manifest(rel, "new-version");
        let error = quick_check_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(error, InstallError::MissingFile { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quick_check_reports_missing_when_file_absent() {
        let dir = test_dir("quick-absent-file");
        let rel = "det/model.onnx";
        write_marker_with(&dir, "v1", rel, hex(b"whatever"), 8);

        let manifest = unpinned_manifest(rel, "v1");
        let error = quick_check_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(error, InstallError::MissingFile { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quick_check_reports_error_on_size_mismatch() {
        let dir = test_dir("quick-size");
        let rel = "det/model.onnx";
        let bytes = b"short";
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        // Marker claims a different size than what is on disk.
        write_marker_with(&dir, "v1", rel, hex(bytes), 9999);

        let manifest = unpinned_manifest(rel, "v1");
        let error = quick_check_install_dir(&dir, &manifest).unwrap_err();
        assert!(matches!(error, InstallError::SizeMismatch { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    fn test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
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
