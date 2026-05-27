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

#[derive(Debug, Serialize, Deserialize)]
struct InstalledMarker {
    version: String,
    completed_at_unix: u64,
}

pub async fn install_rapidocr(
    target_dir: &Path,
    manifest: &ModelManifest,
    progress: impl Fn(InstallProgress) + Send + Sync + 'static,
) -> Result<(), InstallError> {
    fs::create_dir_all(target_dir)?;
    validate_manifest(manifest)?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    let total_bytes = manifest.files.iter().map(|file| file.size).sum::<u64>();
    let mut aggregate_done = 0_u64;

    progress(InstallProgress {
        phase: "starting".to_string(),
        bytes_done: 0,
        bytes_total: total_bytes,
        current_file: None,
    });

    for file in manifest.files {
        let final_path = manifest_file_path(target_dir, file)?;
        if final_path.exists() && verify_model_file(&final_path, file).is_ok() {
            aggregate_done = aggregate_done.saturating_add(file.size);
            progress(InstallProgress {
                phase: "verified".to_string(),
                bytes_done: aggregate_done,
                bytes_total: total_bytes,
                current_file: Some(file.relative_path.to_string()),
            });
            continue;
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

        let response = client.get(file.url).send().await?.error_for_status()?;
        let mut body = response.bytes_stream();
        let mut tmp_file = tokio::fs::File::create(&tmp_path).await?;
        let mut hasher = Sha256::new();
        let mut file_done = 0_u64;

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            file_done = file_done.saturating_add(chunk.len() as u64);
            hasher.update(&chunk);
            tmp_file.write_all(&chunk).await?;
            progress(InstallProgress {
                phase: "downloading".to_string(),
                bytes_done: aggregate_done.saturating_add(file_done),
                bytes_total: total_bytes,
                current_file: Some(file.relative_path.to_string()),
            });
        }
        tmp_file.flush().await?;
        drop(tmp_file);

        let actual_hash = format!("{:x}", hasher.finalize());
        if !actual_hash.eq_ignore_ascii_case(file.sha256) {
            let _ = fs::remove_file(&tmp_path);
            return Err(InstallError::HashMismatch {
                path: tmp_path,
                expected: file.sha256.to_string(),
                actual: actual_hash,
            });
        }
        if file_done != file.size {
            let _ = fs::remove_file(&tmp_path);
            return Err(InstallError::SizeMismatch {
                path: tmp_path,
                expected: file.size,
                actual: file_done,
            });
        }

        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(&tmp_path, &final_path)?;
        verify_model_file(&final_path, file)?;
        aggregate_done = aggregate_done.saturating_add(file.size);
        progress(InstallProgress {
            phase: "verified".to_string(),
            bytes_done: aggregate_done,
            bytes_total: total_bytes,
            current_file: Some(file.relative_path.to_string()),
        });
    }

    write_marker(target_dir, manifest)?;
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
    for file in manifest.files {
        let path = manifest_file_path(target_dir, file)?;
        verify_model_file(&path, file)?;
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

fn verify_model_file(path: &Path, file: &ModelFile) -> Result<(), InstallError> {
    if !path.exists() {
        return Err(InstallError::MissingFile {
            path: path.to_path_buf(),
        });
    }
    let actual_size = fs::metadata(path)?.len();
    if actual_size != file.size {
        return Err(InstallError::SizeMismatch {
            path: path.to_path_buf(),
            expected: file.size,
            actual: actual_size,
        });
    }
    let actual_hash = sha256_file(path)?;
    if !actual_hash.eq_ignore_ascii_case(file.sha256) {
        return Err(InstallError::HashMismatch {
            path: path.to_path_buf(),
            expected: file.sha256.to_string(),
            actual: actual_hash,
        });
    }
    Ok(())
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

fn write_marker(target_dir: &Path, manifest: &ModelManifest) -> Result<(), InstallError> {
    let marker = InstalledMarker {
        version: manifest.version.to_string(),
        completed_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
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
                sha256: hash,
                size: bytes.len() as u64,
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
                sha256: hash,
                size: good.len() as u64,
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
                sha256: hash,
                size: 13,
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
