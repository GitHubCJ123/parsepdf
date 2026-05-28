#[derive(Debug, Clone, Copy)]
pub struct ModelManifest {
    pub version: &'static str,
    pub files: &'static [ModelFile],
    pub total_size_mb: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub url: &'static str,
    pub relative_path: &'static str,
    /// Pinned SHA256. If None, the installer trusts the first successful download
    /// and stores the observed hash in `.installed.json` for tamper-detection on
    /// subsequent loads (trust-on-first-use). Pin a value here once the upstream
    /// release is verified.
    pub sha256: Option<&'static str>,
    /// Pinned size in bytes. If None, the installer accepts any size > 1 KiB and
    /// records the observed size for future verification.
    pub size: Option<u64>,
}

pub const RAPIDOCR_V1: ModelManifest = ModelManifest {
    version: "ppocrv5-en-multilingual-2026-q2",
    total_size_mb: 179,
    files: &[
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx",
            relative_path: "det/ch_PP-OCRv5_det_server.onnx",
            sha256: None,
            size: None,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_server.onnx",
            relative_path: "rec/ch_PP-OCRv5_rec_server.onnx",
            sha256: None,
            size: None,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/en_PP-OCRv5_rec_mobile.onnx",
            relative_path: "rec/en_PP-OCRv5_rec_mobile.onnx",
            sha256: None,
            size: None,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx",
            relative_path: "cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx",
            sha256: None,
            size: None,
        },
    ],
};
