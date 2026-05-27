use std::sync::Arc;

use tokio::sync::Semaphore;

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
    ) -> anyhow::Result<OcrPage> {
        let _permit = self.semaphore.acquire().await?;
        engine.ocr_page(&image, page_index, dpi).await
    }
}

impl Default for OcrWorkerPool {
    fn default() -> Self {
        Self::new()
    }
}
