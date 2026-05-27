pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &'static str;
}
