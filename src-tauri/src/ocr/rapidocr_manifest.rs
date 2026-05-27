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
    pub sha256: &'static str,
    pub size: u64,
}

pub const RAPIDOCR_V1: ModelManifest = ModelManifest {
    version: "ppocrv5-en-multilingual-2026-q2",
    total_size_mb: 179,
    files: &[
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx",
            relative_path: "det/ch_PP-OCRv5_det_server.onnx",
            sha256: "0f8846b1d4bba223a2a2f9d9b44022fbc22cc019051a602b41a7fda9667e4cad",
            size: 88_118_768,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_server.onnx",
            relative_path: "rec/ch_PP-OCRv5_rec_server.onnx",
            sha256: "e09385400eaaaef34ceff54aeb7c4f0f1fe014c27fa8b9905d4709b65746562a",
            size: 84_577_022,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/en_PP-OCRv5_rec_mobile.onnx",
            relative_path: "rec/en_PP-OCRv5_rec_mobile.onnx",
            sha256: "c3461add59bb4323ecba96a492ab75e06dda42467c9e3d0c18db5d1d21924be8",
            size: 7_872_351,
        },
        ModelFile {
            url: "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx",
            relative_path: "cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx",
            sha256: "7d3c02ef6c7da8ae08b4347cc7695b2081aae68c325d64375724ecf39c99e743",
            size: 6_776_876,
        },
    ],
};
