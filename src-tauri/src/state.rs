use std::path::PathBuf;

use anyhow::{anyhow, Context};
use pdfium_render::prelude::Pdfium;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::ocr::worker_pool::OcrWorkerPool;

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub pdfium_path: PathBuf,
    pub worker_pool: OcrWorkerPool,
}

impl AppState {
    pub fn new(app: &AppHandle, db_path: PathBuf) -> anyhow::Result<Self> {
        let pdfium_path = resolve_pdfium_path(app)?;
        Pdfium::bind_to_library(&pdfium_path).with_context(|| {
            format!("failed to bind pdfium library at {}", pdfium_path.display())
        })?;
        let worker_pool = OcrWorkerPool::new();
        info!(
            pdfium_path = %pdfium_path.display(),
            workers = worker_pool.workers(),
            "initialized OCR runtime"
        );
        Ok(Self {
            db_path,
            pdfium_path,
            worker_pool,
        })
    }
}

fn resolve_pdfium_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("binaries").join("pdfium").join("pdfium.dll"));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("binaries").join("pdfium").join("pdfium.dll"));
        candidates.push(
            cwd.join("src-tauri")
                .join("binaries")
                .join("pdfium")
                .join("pdfium.dll"),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("binaries").join("pdfium").join("pdfium.dll"));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("unable to locate bundled binaries/pdfium/pdfium.dll"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pdfium_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("pdfium")
            .join("pdfium.dll");
        Pdfium::bind_to_library(&path)
            .unwrap_or_else(|error| panic!("failed to load {}: {error}", path.display()));
    }
}
