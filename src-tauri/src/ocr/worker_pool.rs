use std::sync::Arc;

use anyhow::anyhow;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use super::{OcrAdapter, OcrPage};

#[derive(Clone)]
pub struct OcrWorkerPool {
    workers: usize,
    semaphore: Arc<Semaphore>,
}

impl OcrWorkerPool {
    pub fn new() -> Self {
        let physical = num_cpus::get_physical().max(1);
        let workers = physical.saturating_sub(1).clamp(1, 4);

        Self {
            workers,
            semaphore: Arc::new(Semaphore::new(workers)),
        }
    }

    pub fn workers(&self) -> usize {
        self.workers
    }

    pub async fn ocr_page(
        &self,
        engine: Arc<dyn OcrAdapter>,
        image: image::RgbaImage,
        page_index: u32,
        dpi: u32,
        cancel: CancellationToken,
    ) -> anyhow::Result<OcrPage> {
        let permit = tokio::select! {
            permit = self.semaphore.acquire() => permit?,
            _ = cancel.cancelled() => return Err(anyhow!("OCR cancelled")),
        };
        let _permit = permit;
        if cancel.is_cancelled() {
            return Err(anyhow!("OCR cancelled"));
        }
        engine.ocr_page(&image, page_index, dpi, cancel).await
    }
}

impl Default for OcrWorkerPool {
    fn default() -> Self {
        Self::new()
    }
}
