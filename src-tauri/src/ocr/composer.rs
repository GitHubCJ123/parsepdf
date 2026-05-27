use std::{collections::BTreeMap, path::Path};

use anyhow::Context;
use lopdf::{
    content::Content, content::Operation, dictionary, Dictionary, Document, Object, ObjectId,
    Stream,
};

use super::{geometry::PageGeometry, OcrPage};

#[derive(Debug, Clone)]
pub struct PageOcrLayer {
    pub page_number: u32,
    pub geometry: PageGeometry,
    pub ocr_page: OcrPage,
}

pub fn compose_searchable_pdf(
    input_path: &Path,
    output_path: &Path,
    layers: &[PageOcrLayer],
) -> anyhow::Result<()> {
    let mut document =
        Document::load(input_path).context("failed to load original PDF for composition")?;
    let pages = document.get_pages();
    let layers_by_page = layers
        .iter()
        .map(|layer| (layer.page_number, layer))
        .collect::<BTreeMap<_, _>>();

    if layers_by_page.is_empty() {
        document
            .save(output_path)
            .context("failed to save original PDF copy")?;
        return Ok(());
    }

    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

    for (page_number, page_id) in pages {
        if let Some(layer) = layers_by_page.get(&page_number) {
            add_font_resource(&mut document, page_id, font_id)
                .with_context(|| format!("failed to add OCR font on page {page_number}"))?;
            let stream_id = append_text_layer_stream(&mut document, layer)
                .with_context(|| format!("failed to build OCR text layer on page {page_number}"))?;
            append_content_stream(&mut document, page_id, stream_id).with_context(|| {
                format!("failed to append OCR text layer on page {page_number}")
            })?;
        }
    }

    document.prune_objects();
    document.compress();
    document
        .save(output_path)
        .context("failed to save searchable PDF")?;
    Ok(())
}

fn add_font_resource(
    document: &mut Document,
    page_id: ObjectId,
    font_id: ObjectId,
) -> anyhow::Result<()> {
    let (direct_resources, inherited_ids) = document.get_page_resources(page_id)?;
    let mut resources = if let Some(resources) = direct_resources {
        resources.clone()
    } else if let Some(resource_id) = inherited_ids.first() {
        document.get_dictionary(*resource_id)?.clone()
    } else {
        Dictionary::new()
    };

    let mut fonts = match resources.get(b"Font") {
        Ok(Object::Dictionary(fonts)) => fonts.clone(),
        Ok(Object::Reference(id)) => document.get_dictionary(*id).cloned().unwrap_or_default(),
        _ => Dictionary::new(),
    };
    fonts.set("Focr", Object::Reference(font_id));
    resources.set("Font", Object::Dictionary(fonts));

    document
        .get_object_mut(page_id)?
        .as_dict_mut()?
        .set("Resources", Object::Dictionary(resources));

    Ok(())
}

fn append_text_layer_stream(
    document: &mut Document,
    layer: &PageOcrLayer,
) -> anyhow::Result<ObjectId> {
    let mut operations = Vec::new();
    operations.push(Operation::new("q", vec![]));

    for block in &layer.ocr_page.blocks {
        for line in &block.lines {
            for word in &line.words {
                let text = word.text.trim();
                if text.is_empty() {
                    continue;
                }

                let rect = layer.geometry.px_to_pdf(word.bbox_px);
                let x = rect[0];
                let y = rect[1];
                let width = (rect[2] - rect[0]).max(1.0);
                let height = (rect[3] - rect[1]).max(1.0);
                let font_size = height.max(1.0);
                let horizontal_scale = estimate_horizontal_scale(text, width, font_size);

                operations.push(Operation::new("BT", vec![]));
                operations.push(Operation::new(
                    "Tf",
                    vec![Object::Name(b"Focr".to_vec()), Object::Real(font_size)],
                ));
                operations.push(Operation::new("Tr", vec![Object::Integer(3)]));
                operations.push(Operation::new("Tz", vec![Object::Real(horizontal_scale)]));
                operations.push(Operation::new(
                    "Tm",
                    vec![
                        Object::Real(1.0),
                        Object::Real(0.0),
                        Object::Real(0.0),
                        Object::Real(1.0),
                        Object::Real(x),
                        Object::Real(y),
                    ],
                ));
                operations.push(Operation::new(
                    "Tj",
                    vec![Object::string_literal(text.as_bytes().to_vec())],
                ));
                operations.push(Operation::new("ET", vec![]));
            }
        }
    }

    operations.push(Operation::new("Q", vec![]));
    let content = Content { operations };
    let stream = Stream::new(dictionary! {}, content.encode()?);
    Ok(document.add_object(stream))
}

fn estimate_horizontal_scale(text: &str, target_width: f32, font_size: f32) -> f32 {
    let estimated_width = text.chars().count().max(1) as f32 * font_size * 0.5;
    ((target_width / estimated_width) * 100.0).clamp(20.0, 400.0)
}

fn append_content_stream(
    document: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
) -> anyhow::Result<()> {
    let page = document.get_object_mut(page_id)?.as_dict_mut()?;
    let existing = page.get(b"Contents").ok().cloned();
    let new_contents = match existing {
        Some(Object::Array(mut contents)) => {
            contents.push(Object::Reference(stream_id));
            Object::Array(contents)
        }
        Some(contents) => Object::Array(vec![contents, Object::Reference(stream_id)]),
        None => Object::Reference(stream_id),
    };
    page.set("Contents", new_contents);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use crate::ocr::{BBoxPx, OcrBlock, OcrLine, OcrWord, TextDirection};

    #[test]
    fn composed_pdf_extracts_invisible_text() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("phase1-composer-test");
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("input.pdf");
        let output = dir.join("output.pdf");
        write_blank_pdf(&input);

        let layer = PageOcrLayer {
            page_number: 1,
            geometry: PageGeometry {
                media_box: [0.0, 0.0, 612.0, 792.0],
                crop_box: [0.0, 0.0, 612.0, 792.0],
                rotation: 0,
                image_width_px: 1700,
                image_height_px: 2200,
                dpi: 200,
            },
            ocr_page: OcrPage {
                page_index: 0,
                image_width_px: 1700,
                image_height_px: 2200,
                dpi: 200,
                orientation_deg: 0,
                blocks: vec![OcrBlock {
                    lines: vec![OcrLine {
                        words: vec![OcrWord {
                            text: "Needle".to_string(),
                            bbox_px: BBoxPx {
                                left: 100,
                                top: 100,
                                right: 300,
                                bottom: 160,
                            },
                            confidence: Some(99.0),
                            direction: TextDirection::Ltr,
                        }],
                    }],
                }],
                plain_text: "Needle".to_string(),
                mean_confidence: Some(99.0),
            },
        };

        compose_searchable_pdf(&input, &output, &[layer]).unwrap();
        let document = Document::load(&output).unwrap();
        let extracted = document.extract_text(&[1]).unwrap();
        assert!(extracted.contains("Needle"), "{extracted}");
    }

    #[test]
    fn horizontal_scale_is_clamped() {
        assert_eq!(estimate_horizontal_scale("hello", 100.0, 10.0), 400.0);
        assert_eq!(estimate_horizontal_scale("verylongword", 1.0, 10.0), 20.0);
    }

    fn write_blank_pdf(path: &Path) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        document.objects.insert(
            page_id,
            dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => dictionary! {},
                "Contents" => Object::Reference(content_id),
            }
            .into(),
        );
        document.objects.insert(
            pages_id,
            dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }
            .into(),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        document.trailer.set("Root", Object::Reference(catalog_id));
        document.save(path).unwrap();
    }
}
