pub mod composer;
pub mod geometry;
pub mod pdf_pipeline;
pub mod tesseract;
pub mod worker_pool;

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrPage {
    pub page_index: u32,
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub dpi: u32,
    pub orientation_deg: i32,
    pub blocks: Vec<OcrBlock>,
    pub plain_text: String,
    pub mean_confidence: Option<f32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrBlock {
    pub lines: Vec<OcrLine>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrLine {
    pub words: Vec<OcrWord>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrWord {
    pub text: String,
    pub bbox_px: BBoxPx,
    pub confidence: Option<f32>,
    pub direction: TextDirection,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BBoxPx {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

#[async_trait::async_trait]
pub trait OcrAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    async fn ocr_page(
        &self,
        image: &image::RgbaImage,
        page_index: u32,
        dpi: u32,
    ) -> anyhow::Result<OcrPage>;
}
