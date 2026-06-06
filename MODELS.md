# RapidOCR model manifest

Phase 6 uses the pure Rust `ort` integration path behind the Cargo `rapidocr` feature. The default installer does not bundle these files.

RapidAI's GitHub releases do not currently publish model assets. The pinned files below come from `python/rapidocr/default_models.yaml` in RapidAI/RapidOCR v3.8.x and are served over HTTPS by ModelScope. The `X-Linked-ETag` returned by ModelScope matches the SHA256 published in RapidOCR's model manifest.

| Relative path | Source | SHA256 | Size |
| --- | --- | --- | ---: |
| `det/ch_PP-OCRv5_det_server.onnx` | ModelScope RapidAI/RapidOCR v3.8.0 | `0f8846b1d4bba223a2a2f9d9b44022fbc22cc019051a602b41a7fda9667e4cad` | 88,118,768 |
| `rec/ch_PP-OCRv5_rec_server.onnx` | ModelScope RapidAI/RapidOCR v3.8.0 | `e09385400eaaaef34ceff54aeb7c4f0f1fe014c27fa8b9905d4709b65746562a` | 84,577,022 |
| `rec/en_PP-OCRv5_rec_mobile.onnx` | ModelScope RapidAI/RapidOCR v3.8.0 | `c3461add59bb4323ecba96a492ab75e06dda42467c9e3d0c18db5d1d21924be8` | 7,872,351 |
| `cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx` | ModelScope RapidAI/RapidOCR v3.8.0 | `7d3c02ef6c7da8ae08b4347cc7695b2081aae68c325d64375724ecf39c99e743` | 6,776,876 |

Regenerate the manifest snippet with:

```powershell
.\scripts\gen_rapidocr_manifest.ps1
```

## Which OCR engine should I use?

| Engine | Speed | Accuracy | Languages | Download size | Best for |
| --- | --- | --- | --- | ---: | --- |
| Tesseract | Fastest and lightweight | Good for clean text | English and Latin-script documents | Bundled, no extra download | Everyday PDFs when you want quick offline OCR |
| RapidOCR PP-OCRv5 | Slower than Tesseract | Higher accuracy on difficult scans | Multilingual, including CJK | Optional larger model download | Scans, tables, multi-column layouts, and non-Latin text |
