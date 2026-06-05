use super::{BBoxPx, OcrBlock, OcrLine, OcrPage, OcrWord, TextDirection};
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct RapidOcrTextLine {
    pub polygon: [(f32, f32); 4],
    pub text: String,
    pub confidence: f32,
}

pub fn rapid_lines_to_ocr_page(
    mut lines: Vec<RapidOcrTextLine>,
    page_index: u32,
    image_width_px: u32,
    image_height_px: u32,
    dpi: u32,
) -> OcrPage {
    lines.sort_by(|a, b| {
        let a_box = polygon_bbox(&a.polygon, image_width_px, image_height_px);
        let b_box = polygon_bbox(&b.polygon, image_width_px, image_height_px);
        a_box
            .top
            .cmp(&b_box.top)
            .then_with(|| a_box.left.cmp(&b_box.left))
            .then(Ordering::Equal)
    });

    let mut confidences = Vec::new();
    let mut ocr_lines = Vec::new();
    for line in lines {
        let text = line.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        confidences.push(line.confidence);
        ocr_lines.push(OcrLine {
            words: vec![OcrWord {
                text,
                bbox_px: polygon_bbox(&line.polygon, image_width_px, image_height_px),
                confidence: Some(line.confidence),
                direction: TextDirection::Ltr,
            }],
        });
    }

    let plain_text = ocr_lines
        .iter()
        .map(|line| {
            line.words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mean_confidence = if confidences.is_empty() {
        None
    } else {
        Some(confidences.iter().sum::<f32>() / confidences.len() as f32)
    };

    OcrPage {
        page_index,
        image_width_px,
        image_height_px,
        dpi,
        orientation_deg: 0,
        blocks: if ocr_lines.is_empty() {
            Vec::new()
        } else {
            vec![OcrBlock { lines: ocr_lines }]
        },
        plain_text,
        mean_confidence,
    }
}

fn polygon_bbox(polygon: &[(f32, f32); 4], image_width_px: u32, image_height_px: u32) -> BBoxPx {
    let (mut left, mut top) = (f32::MAX, f32::MAX);
    let (mut right, mut bottom) = (0.0_f32, 0.0_f32);
    for (x, y) in polygon {
        left = left.min(*x);
        top = top.min(*y);
        right = right.max(*x);
        bottom = bottom.max(*y);
    }

    BBoxPx {
        left: clamp_coord(left.floor(), image_width_px),
        top: clamp_coord(top.floor(), image_height_px),
        right: clamp_coord(right.ceil(), image_width_px),
        bottom: clamp_coord(bottom.ceil(), image_height_px),
    }
}

fn clamp_coord(value: f32, max: u32) -> u32 {
    if value.is_nan() || value <= 0.0 {
        0
    } else if value >= max as f32 {
        max
    } else {
        value as u32
    }
}

#[cfg(feature = "rapidocr")]
mod adapter {
    use std::collections::HashMap;
    use std::path::Path;

    use anyhow::{anyhow, Context};
    use image::{imageops, RgbaImage};
    use imageproc::region_labelling::{connected_components, Connectivity};
    use ndarray::Array4;
    use ort::{session::Session, value::TensorRef};
    use tokio::sync::{Mutex, OnceCell};

    use std::path::PathBuf;

    use super::{rapid_lines_to_ocr_page, BBoxPx, OcrPage, RapidOcrTextLine};
    use crate::ocr::{
        rapidocr_install::{manifest_file_path, verify_install_dir, InstallError},
        rapidocr_manifest::RAPIDOCR_V1,
        OcrAdapter,
    };

    const DET_THRESHOLD: f32 = 0.30;
    const DET_BOX_THRESHOLD: f32 = 0.50;
    const DET_MAX_SIDE: u32 = 2_000;
    const DET_MIN_SIDE: u32 = 736;
    const REC_HEIGHT: u32 = 48;
    const REC_BASE_WIDTH: u32 = 320;
    const REC_MAX_WIDTH: u32 = 2_048;

    pub struct RapidOcrAdapter {
        models_dir: PathBuf,
        session: OnceCell<Mutex<RapidOcrSession>>,
    }

    impl RapidOcrAdapter {
        pub fn new(models_dir: PathBuf) -> Self {
            Self {
                models_dir,
                session: OnceCell::new(),
            }
        }

        pub async fn verify_install(&self) -> Result<(), InstallError> {
            // Full-file SHA256 verification of the ~179 MB model set must not run
            // on the async runtime thread, or it stalls the app during OCR.
            let models_dir = self.models_dir.clone();
            tokio::task::spawn_blocking(move || verify_install_dir(&models_dir, &RAPIDOCR_V1))
                .await
                .map_err(|error| {
                    InstallError::Io(std::io::Error::other(error.to_string()))
                })?
        }

        async fn session(&self) -> anyhow::Result<&Mutex<RapidOcrSession>> {
            self.session
                .get_or_try_init(|| async {
                    self.verify_install()
                        .await
                        .context("RapidOCR models are missing or failed SHA256 verification")?;
                    let models_dir = self.models_dir.clone();
                    let session =
                        tokio::task::spawn_blocking(move || RapidOcrSession::load(&models_dir))
                            .await
                            .context("failed to join RapidOCR model load task")??;
                    Ok(Mutex::new(session))
                })
                .await
        }
    }

    #[async_trait::async_trait]
    impl OcrAdapter for RapidOcrAdapter {
        fn name(&self) -> &'static str {
            "rapidocr"
        }

        async fn ocr_page(
            &self,
            image: &image::RgbaImage,
            page_index: u32,
            dpi: u32,
            cancel: tokio_util::sync::CancellationToken,
        ) -> anyhow::Result<OcrPage> {
            if cancel.is_cancelled() {
                return Err(anyhow!("OCR cancelled"));
            }
            // No per-page verify: `session()` performs the authoritative SHA256
            // check once on first init and caches the loaded models, so
            // re-hashing ~179 MB before every page only added latency.
            let session = self.session().await?;
            let mut session = session.lock().await;
            let page = session.ocr_page(image, page_index, dpi)?;
            if cancel.is_cancelled() {
                return Err(anyhow!("OCR cancelled"));
            }
            Ok(page)
        }
    }

    struct RapidOcrSession {
        det: Session,
        rec_ch: Session,
        rec_ch_chars: Vec<String>,
        rec_en: Session,
        rec_en_chars: Vec<String>,
        _cls: Session,
    }

    impl RapidOcrSession {
        fn load(models_dir: &Path) -> anyhow::Result<Self> {
            let _ = ort::init().commit();
            let det_path = model_path(models_dir, "det/ch_PP-OCRv5_det_server.onnx")?;
            let rec_ch_path = model_path(models_dir, "rec/ch_PP-OCRv5_rec_server.onnx")?;
            let rec_en_path = model_path(models_dir, "rec/en_PP-OCRv5_rec_mobile.onnx")?;
            let cls_path = model_path(
                models_dir,
                "cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx",
            )?;

            let det = build_session(det_path)?;
            let rec_ch = build_session(rec_ch_path)?;
            let rec_ch_chars = character_list(&rec_ch)
                .context("ch_PP-OCRv5_rec_server has no character metadata")?;
            let rec_en = build_session(rec_en_path)?;
            let rec_en_chars = character_list(&rec_en)
                .context("en_PP-OCRv5_rec_mobile has no character metadata")?;
            let cls = build_session(cls_path)?;

            Ok(Self {
                det,
                rec_ch,
                rec_ch_chars,
                rec_en,
                rec_en_chars,
                _cls: cls,
            })
        }

        fn ocr_page(
            &mut self,
            image: &RgbaImage,
            page_index: u32,
            dpi: u32,
        ) -> anyhow::Result<OcrPage> {
            let det_input = prepare_det_input(image)?;
            let boxes = self.detect_text_boxes(&det_input)?;
            let mut lines = Vec::new();

            for bbox in boxes.into_iter().take(1_000) {
                if let Some((text, confidence)) = self.recognize_best(image, bbox) {
                    lines.push(RapidOcrTextLine {
                        polygon: bbox_to_polygon(bbox),
                        text,
                        confidence,
                    });
                }
            }

            Ok(rapid_lines_to_ocr_page(
                lines,
                page_index,
                image.width(),
                image.height(),
                dpi,
            ))
        }

        fn detect_text_boxes(&mut self, input: &DetInput) -> anyhow::Result<Vec<BBoxPx>> {
            let tensor = Array4::from_shape_vec(
                (1, 3, input.height as usize, input.width as usize),
                input.data.clone(),
            )?;
            let tensor_input = TensorRef::from_array_view(tensor.view())
                .map_err(|error| anyhow!(error.to_string()))?;
            let outputs = self
                .det
                .run(ort::inputs![tensor_input])
                .map_err(|error| anyhow!(error.to_string()))?;
            let output = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|error| anyhow!(error.to_string()))?;
            let (shape, data) = output;
            let dims = shape.iter().copied().collect::<Vec<_>>();
            if dims.len() < 2 {
                return Err(anyhow!(
                    "RapidOCR detection output had invalid shape: {dims:?}"
                ));
            }
            let out_h = *dims
                .get(dims.len().saturating_sub(2))
                .ok_or_else(|| anyhow!("RapidOCR detection output missing height"))?
                as usize;
            let out_w = *dims
                .get(dims.len().saturating_sub(1))
                .ok_or_else(|| anyhow!("RapidOCR detection output missing width"))?
                as usize;
            if out_h == 0 || out_w == 0 || data.len() < out_h * out_w {
                return Err(anyhow!(
                    "RapidOCR detection output data length did not match shape"
                ));
            }
            let plane = &data[..out_h * out_w];
            Ok(heatmap_to_boxes(
                plane,
                out_w,
                out_h,
                input.scale_x,
                input.scale_y,
            ))
        }

        fn recognize_best(&mut self, image: &RgbaImage, bbox: BBoxPx) -> Option<(String, f32)> {
            let ch = recognize_with(&mut self.rec_ch, &self.rec_ch_chars, image, bbox)
                .ok()
                .flatten();
            let en = recognize_with(&mut self.rec_en, &self.rec_en_chars, image, bbox)
                .ok()
                .flatten();
            match (ch, en) {
                (Some(ch), Some(en)) => Some(if en.1 > ch.1 { en } else { ch }),
                (Some(ch), None) => Some(ch),
                (None, Some(en)) => Some(en),
                (None, None) => None,
            }
        }
    }

    struct DetInput {
        data: Vec<f32>,
        width: u32,
        height: u32,
        scale_x: f32,
        scale_y: f32,
    }

    fn build_session(path: PathBuf) -> anyhow::Result<Session> {
        let builder = Session::builder().map_err(|error| anyhow!(error.to_string()))?;
        let mut builder = builder
            .with_intra_threads(1)
            .map_err(|error| anyhow!(error.to_string()))?;
        builder
            .commit_from_file(path)
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn model_path(models_dir: &Path, relative_path: &'static str) -> anyhow::Result<PathBuf> {
        let file = RAPIDOCR_V1
            .files
            .iter()
            .find(|file| file.relative_path == relative_path)
            .ok_or_else(|| anyhow!("RapidOCR manifest is missing {relative_path}"))?;
        manifest_file_path(models_dir, file).map_err(Into::into)
    }

    fn character_list(session: &Session) -> anyhow::Result<Vec<String>> {
        let metadata = session
            .metadata()
            .map_err(|error| anyhow!(error.to_string()))?;
        let characters = metadata
            .custom("character")
            .ok_or_else(|| anyhow!("model metadata key 'character' not found"))?;
        let mut list = vec!["blank".to_string()];
        list.extend(characters.lines().map(ToString::to_string));
        list.push(" ".to_string());
        Ok(list)
    }

    fn prepare_det_input(image: &RgbaImage) -> anyhow::Result<DetInput> {
        let (target_w, target_h) = det_dimensions(image.width(), image.height());
        let resized = imageops::resize(image, target_w, target_h, imageops::FilterType::Triangle);
        let mut data = vec![0.0_f32; 3 * target_w as usize * target_h as usize];
        let plane = target_w as usize * target_h as usize;
        for (x, y, pixel) in resized.enumerate_pixels() {
            let idx = y as usize * target_w as usize + x as usize;
            data[idx] = (pixel[0] as f32 / 255.0 - 0.5) / 0.5;
            data[plane + idx] = (pixel[1] as f32 / 255.0 - 0.5) / 0.5;
            data[(2 * plane) + idx] = (pixel[2] as f32 / 255.0 - 0.5) / 0.5;
        }
        Ok(DetInput {
            data,
            width: target_w,
            height: target_h,
            scale_x: image.width() as f32 / target_w as f32,
            scale_y: image.height() as f32 / target_h as f32,
        })
    }

    fn det_dimensions(width: u32, height: u32) -> (u32, u32) {
        let min_side = width.min(height).max(1);
        let max_side = width.max(height).max(1);
        let mut scale = 1.0_f32;
        if max_side > DET_MAX_SIDE {
            scale = DET_MAX_SIDE as f32 / max_side as f32;
        }
        if (min_side as f32 * scale) < DET_MIN_SIDE as f32 {
            scale = DET_MIN_SIDE as f32 / min_side as f32;
        }
        let resize_w = round_to_32((width as f32 * scale).round() as u32);
        let resize_h = round_to_32((height as f32 * scale).round() as u32);
        (resize_w.max(32), resize_h.max(32))
    }

    fn round_to_32(value: u32) -> u32 {
        (((value.max(32) + 16) / 32) * 32).max(32)
    }

    fn heatmap_to_boxes(
        heatmap: &[f32],
        width: usize,
        height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> Vec<BBoxPx> {
        let mut mask = image::GrayImage::new(width as u32, height as u32);
        for y in 0..height {
            for x in 0..width {
                let value = if heatmap[y * width + x] > DET_THRESHOLD {
                    255
                } else {
                    0
                };
                mask.put_pixel(x as u32, y as u32, image::Luma([value]));
            }
        }

        let labels = connected_components(&mask, Connectivity::Eight, image::Luma([0_u8]));
        let mut components = HashMap::<u32, ComponentStats>::new();
        for y in 0..height {
            for x in 0..width {
                let label = labels.get_pixel(x as u32, y as u32)[0];
                if label == 0 {
                    continue;
                }
                let score = heatmap[y * width + x];
                components
                    .entry(label)
                    .and_modify(|stats| stats.update(x as u32, y as u32, score))
                    .or_insert_with(|| ComponentStats::new(x as u32, y as u32, score));
            }
        }

        let mut boxes = components
            .into_values()
            .filter(|stats| stats.count >= 9 && stats.mean_score() >= DET_BOX_THRESHOLD)
            .map(|stats| BBoxPx {
                left: (stats.left as f32 * scale_x).floor().max(0.0) as u32,
                top: (stats.top as f32 * scale_y).floor().max(0.0) as u32,
                right: ((stats.right + 1) as f32 * scale_x).ceil().max(0.0) as u32,
                bottom: ((stats.bottom + 1) as f32 * scale_y).ceil().max(0.0) as u32,
            })
            .filter(|bbox| {
                bbox.right.saturating_sub(bbox.left) > 3 && bbox.bottom.saturating_sub(bbox.top) > 3
            })
            .collect::<Vec<_>>();
        boxes.sort_by_key(|bbox| (bbox.top / 10, bbox.left));
        boxes
    }

    struct ComponentStats {
        left: u32,
        top: u32,
        right: u32,
        bottom: u32,
        score_sum: f32,
        count: u32,
    }

    impl ComponentStats {
        fn new(x: u32, y: u32, score: f32) -> Self {
            Self {
                left: x,
                top: y,
                right: x,
                bottom: y,
                score_sum: score,
                count: 1,
            }
        }

        fn update(&mut self, x: u32, y: u32, score: f32) {
            self.left = self.left.min(x);
            self.top = self.top.min(y);
            self.right = self.right.max(x);
            self.bottom = self.bottom.max(y);
            self.score_sum += score;
            self.count += 1;
        }

        fn mean_score(&self) -> f32 {
            self.score_sum / self.count as f32
        }
    }

    fn recognize_with(
        session: &mut Session,
        characters: &[String],
        image: &RgbaImage,
        bbox: BBoxPx,
    ) -> anyhow::Result<Option<(String, f32)>> {
        let Some(crop) = crop_bbox(image, bbox) else {
            return Ok(None);
        };
        let input = prepare_rec_input(&crop)?;
        let tensor = Array4::from_shape_vec(
            (1, 3, REC_HEIGHT as usize, input.width as usize),
            input.data,
        )?;
        let input = TensorRef::from_array_view(tensor.view())
            .map_err(|error| anyhow!(error.to_string()))?;
        let outputs = session
            .run(ort::inputs![input])
            .map_err(|error| anyhow!(error.to_string()))?;
        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| anyhow!(error.to_string()))?;
        let (shape, data) = output;
        let dims = shape.iter().copied().collect::<Vec<_>>();
        if dims.len() != 3 || dims[0] != 1 {
            return Err(anyhow!(
                "RapidOCR recognition output had invalid shape: {dims:?}"
            ));
        }
        let steps = dims[1] as usize;
        let classes = dims[2] as usize;
        if data.len() < steps * classes {
            return Err(anyhow!(
                "RapidOCR recognition output data length did not match shape"
            ));
        }
        let decoded = decode_ctc(data, steps, classes, characters);
        Ok(decoded.filter(|(text, _)| !text.trim().is_empty()))
    }

    struct RecInput {
        data: Vec<f32>,
        width: u32,
    }

    fn crop_bbox(image: &RgbaImage, bbox: BBoxPx) -> Option<RgbaImage> {
        let left = bbox.left.min(image.width());
        let top = bbox.top.min(image.height());
        let right = bbox.right.min(image.width());
        let bottom = bbox.bottom.min(image.height());
        let width = right.checked_sub(left)?;
        let height = bottom.checked_sub(top)?;
        if width == 0 || height == 0 {
            return None;
        }
        Some(imageops::crop_imm(image, left, top, width, height).to_image())
    }

    fn prepare_rec_input(crop: &RgbaImage) -> anyhow::Result<RecInput> {
        let ratio = crop.width().max(1) as f32 / crop.height().max(1) as f32;
        let resized_w = ((REC_HEIGHT as f32 * ratio).ceil() as u32).clamp(1, REC_MAX_WIDTH);
        let tensor_w = resized_w.clamp(REC_BASE_WIDTH, REC_MAX_WIDTH);
        let resized = imageops::resize(crop, resized_w, REC_HEIGHT, imageops::FilterType::Triangle);
        let mut data = vec![0.0_f32; 3 * REC_HEIGHT as usize * tensor_w as usize];
        let plane = REC_HEIGHT as usize * tensor_w as usize;
        for (x, y, pixel) in resized.enumerate_pixels() {
            let idx = y as usize * tensor_w as usize + x as usize;
            data[idx] = (pixel[0] as f32 / 255.0 - 0.5) / 0.5;
            data[plane + idx] = (pixel[1] as f32 / 255.0 - 0.5) / 0.5;
            data[(2 * plane) + idx] = (pixel[2] as f32 / 255.0 - 0.5) / 0.5;
        }
        Ok(RecInput {
            data,
            width: tensor_w,
        })
    }

    fn decode_ctc(
        data: &[f32],
        steps: usize,
        classes: usize,
        characters: &[String],
    ) -> Option<(String, f32)> {
        let mut last_idx = usize::MAX;
        let mut text = String::new();
        let mut confidences = Vec::new();
        for step in 0..steps {
            let start = step * classes;
            let row = &data[start..start + classes];
            let (idx, probability) = row
                .iter()
                .copied()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
            if idx != 0 && idx != last_idx {
                if let Some(character) = characters.get(idx) {
                    text.push_str(character);
                    confidences.push(probability);
                }
            }
            last_idx = idx;
        }
        let confidence = if confidences.is_empty() {
            0.0
        } else {
            confidences.iter().sum::<f32>() / confidences.len() as f32
        };
        Some((text, confidence))
    }

    fn bbox_to_polygon(bbox: BBoxPx) -> [(f32, f32); 4] {
        [
            (bbox.left as f32, bbox.top as f32),
            (bbox.right as f32, bbox.top as f32),
            (bbox.right as f32, bbox.bottom as f32),
            (bbox.left as f32, bbox.bottom as f32),
        ]
    }
}

#[cfg(feature = "rapidocr")]
pub use adapter::RapidOcrAdapter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_rapidocr_quadrilaterals_to_ocr_page() {
        let page = rapid_lines_to_ocr_page(
            vec![RapidOcrTextLine {
                polygon: [(40.0, 12.0), (92.0, 10.0), (95.0, 30.0), (38.0, 28.0)],
                text: "Invoice total".to_string(),
                confidence: 0.93,
            }],
            2,
            120,
            80,
            200,
        );

        assert_eq!(page.page_index, 2);
        assert_eq!(page.plain_text, "Invoice total");
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].lines.len(), 1);
        let word = &page.blocks[0].lines[0].words[0];
        assert_eq!(word.text, "Invoice total");
        assert_eq!(word.bbox_px.left, 38);
        assert_eq!(word.bbox_px.top, 10);
        assert_eq!(word.bbox_px.right, 95);
        assert_eq!(word.bbox_px.bottom, 30);
        assert_eq!(page.mean_confidence, Some(0.93));
    }
}
