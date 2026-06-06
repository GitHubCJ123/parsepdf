use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context};
use pdfium_render::prelude::Pdfium;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::{events::ProgressAggregator, ocr::worker_pool::OcrWorkerPool, rag::EmbeddingRuntime};

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub pdfium_path: PathBuf,
    pub worker_pool: OcrWorkerPool,
    pub progress: ProgressAggregator,
    pub embeddings: Arc<EmbeddingRuntime>,
}

impl AppState {
    pub fn new(app: &AppHandle, db_path: PathBuf) -> anyhow::Result<Self> {
        let pdfium_path = resolve_pdfium_path(app)?;
        // Smoke-test that the DLL can be loaded at startup. The actual per-thread
        // Pdfium instances are created lazily inside the OCR pipeline via
        // thread_local caching, so the DLL gets loaded once per worker thread
        // and reused for every page on that thread.
        Pdfium::bind_to_library(&pdfium_path).with_context(|| {
            format!("failed to bind pdfium library at {}", pdfium_path.display())
        })?;
        let worker_pool = OcrWorkerPool::new();
        let progress = ProgressAggregator::new(app.clone());
        let embeddings = Arc::new(EmbeddingRuntime::from_default_cache()?);
        embeddings.prewarm();
        info!(
            pdfium_path = %pdfium_path.display(),
            workers = worker_pool.workers(),
            "initialized OCR runtime"
        );
        Ok(Self {
            db_path,
            pdfium_path,
            worker_pool,
            progress,
            embeddings,
        })
    }
}

fn resolve_pdfium_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let filename = pdfium_library_filename();
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("binaries").join("pdfium").join(filename));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("binaries").join("pdfium").join(filename));
        candidates.push(
            cwd.join("src-tauri")
                .join("binaries")
                .join("pdfium")
                .join(filename),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("binaries").join("pdfium").join(filename));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("unable to locate bundled binaries/pdfium/{filename}"))
}

/// Filename of the bundled PDFium dynamic library for the current platform.
fn pdfium_library_filename() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "pdfium.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libpdfium.dylib"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "libpdfium.so"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pdfium_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("pdfium")
            .join(pdfium_library_filename());
        // The PDFium dynamic library is only committed for Windows; on other
        // platforms it is fetched during the release bundle step, so skip the
        // load smoke-test when the binary is not present locally.
        if !path.exists() {
            return;
        }
        Pdfium::bind_to_library(&path)
            .unwrap_or_else(|error| panic!("failed to load {}: {error}", path.display()));
    }
}
