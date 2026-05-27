use std::{ffi::OsString, path::PathBuf};

use anyhow::{anyhow, Context};
use image::ImageFormat;
use quick_xml::{events::Event, Reader};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{process::CommandEvent, ShellExt};
use tempfile::Builder;
use tokio_util::sync::CancellationToken;

use super::{BBoxPx, OcrAdapter, OcrBlock, OcrLine, OcrPage, OcrWord, TextDirection};

#[derive(Clone)]
pub struct TesseractAdapter {
    app: AppHandle,
    tessdata_prefix: PathBuf,
}

impl TesseractAdapter {
    pub fn new(app: AppHandle) -> anyhow::Result<Self> {
        let tessdata_prefix = resolve_tesseract_resource_dir(&app)?;
        Ok(Self {
            app,
            tessdata_prefix,
        })
    }
}

#[async_trait::async_trait]
impl OcrAdapter for TesseractAdapter {
    fn name(&self) -> &'static str {
        "tesseract"
    }

    async fn ocr_page(
        &self,
        image: &image::RgbaImage,
        page_index: u32,
        dpi: u32,
        cancel: CancellationToken,
    ) -> anyhow::Result<OcrPage> {
        if cancel.is_cancelled() {
            return Err(anyhow!("OCR cancelled"));
        }
        let input = Builder::new()
            .prefix("pdf-parser-page-")
            .suffix(".png")
            .tempfile()
            .context("failed to allocate temporary OCR image")?;

        image
            .save_with_format(input.path(), ImageFormat::Png)
            .context("failed to write temporary OCR image")?;

        let mut args = Vec::<OsString>::new();
        args.push(input.path().as_os_str().to_os_string());
        for arg in [
            "stdout",
            "-l",
            "eng",
            "--psm",
            "3",
            "--oem",
            "1",
            "-c",
            "tessedit_create_hocr=1",
            "hocr",
        ] {
            args.push(OsString::from(arg));
        }

        let command = self
            .app
            .shell()
            .sidecar("tesseract")
            .context("failed to resolve bundled tesseract sidecar")?
            .args(args)
            .current_dir(&self.tessdata_prefix)
            .env("OMP_THREAD_LIMIT", "1")
            .env("TESSDATA_PREFIX", self.tessdata_prefix.join("tessdata"))
            .set_raw_out(true);
        let (mut rx, child) = command
            .spawn()
            .context("failed to execute tesseract sidecar")?;
        let mut child = Some(child);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut code = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    if let Some(child) = child.take() {
                        let _ = child.kill();
                    }
                    return Err(anyhow!("OCR cancelled"));
                }
                event = rx.recv() => {
                    let Some(event) = event else { break; };
                    match event {
                        CommandEvent::Stdout(bytes) => stdout.extend(bytes),
                        CommandEvent::Stderr(bytes) => stderr.extend(bytes),
                        CommandEvent::Terminated(payload) => code = payload.code,
                        CommandEvent::Error(error) => stderr.extend(error.into_bytes()),
                        _ => {}
                    }
                }
            }
        }

        let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
        if code != Some(0) {
            return Err(anyhow!("tesseract exited with code {:?}: {}", code, stderr));
        }

        let hocr = String::from_utf8(stdout).context("tesseract hOCR was not UTF-8")?;
        parse_hocr(&hocr, page_index, image.width(), image.height(), dpi)
    }
}

fn resolve_tesseract_resource_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("binaries").join("tesseract"));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("binaries").join("tesseract"));
        candidates.push(cwd.join("src-tauri").join("binaries").join("tesseract"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("binaries").join("tesseract"));
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("tessdata").join("eng.traineddata").exists())
        .ok_or_else(|| anyhow!("unable to locate bundled tessdata/eng.traineddata"))
}

fn parse_hocr(
    hocr: &str,
    page_index: u32,
    image_width_px: u32,
    image_height_px: u32,
    dpi: u32,
) -> anyhow::Result<OcrPage> {
    let mut reader = Reader::from_str(hocr);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut blocks = Vec::<OcrBlock>::new();
    let mut current_block: Option<OcrBlock> = None;
    let mut current_line: Option<OcrLine> = None;
    let mut current_word: Option<OcrWord> = None;
    let mut confidences = Vec::<f32>::new();
    let mut span_stack = Vec::<SpanKind>::new();
    let mut div_stack = Vec::<bool>::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(start) => {
                let name = String::from_utf8_lossy(start.name().as_ref()).to_string();
                let attrs = collect_attrs(&reader, &start)?;
                if name.eq_ignore_ascii_case("div") {
                    let is_block = attrs.class.as_deref().is_some_and(|class| {
                        class.contains("ocr_carea") || class.contains("ocrx_block")
                    });
                    if is_block {
                        finish_line(&mut current_block, &mut current_line);
                        finish_block(&mut blocks, &mut current_block);
                        current_block = Some(OcrBlock { lines: Vec::new() });
                    }
                    div_stack.push(is_block);
                } else if name.eq_ignore_ascii_case("span") {
                    let class = attrs.class.as_deref().unwrap_or_default();
                    if class.contains("ocr_line") {
                        finish_line(&mut current_block, &mut current_line);
                        if current_block.is_none() {
                            current_block = Some(OcrBlock { lines: Vec::new() });
                        }
                        current_line = Some(OcrLine { words: Vec::new() });
                        span_stack.push(SpanKind::Line);
                    } else if class.contains("ocrx_word") {
                        let (bbox_px, confidence) = parse_word_title(attrs.title.as_deref())?;
                        if let Some(confidence) = confidence {
                            confidences.push(confidence);
                        }
                        current_word = Some(OcrWord {
                            text: String::new(),
                            bbox_px,
                            confidence,
                            direction: TextDirection::Ltr,
                        });
                        span_stack.push(SpanKind::Word);
                    } else {
                        span_stack.push(SpanKind::Other);
                    }
                }
            }
            Event::Empty(start) => {
                let name = String::from_utf8_lossy(start.name().as_ref()).to_string();
                let attrs = collect_attrs(&reader, &start)?;
                if name.eq_ignore_ascii_case("span")
                    && attrs
                        .class
                        .as_deref()
                        .is_some_and(|class| class.contains("ocrx_word"))
                {
                    let (bbox_px, confidence) = parse_word_title(attrs.title.as_deref())?;
                    if let Some(confidence) = confidence {
                        confidences.push(confidence);
                    }
                    push_word(
                        &mut current_block,
                        &mut current_line,
                        OcrWord {
                            text: String::new(),
                            bbox_px,
                            confidence,
                            direction: TextDirection::Ltr,
                        },
                    );
                }
            }
            Event::Text(text) => {
                if let Some(word) = current_word.as_mut() {
                    word.text.push_str(text.unescape()?.as_ref());
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if name.eq_ignore_ascii_case("span") {
                    match span_stack.pop().unwrap_or(SpanKind::Other) {
                        SpanKind::Word => {
                            if let Some(word) = current_word.take() {
                                push_word(&mut current_block, &mut current_line, word);
                            }
                        }
                        SpanKind::Line => finish_line(&mut current_block, &mut current_line),
                        SpanKind::Other => {}
                    }
                } else if name.eq_ignore_ascii_case("div") && div_stack.pop().unwrap_or(false) {
                    finish_line(&mut current_block, &mut current_line);
                    finish_block(&mut blocks, &mut current_block);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if let Some(word) = current_word.take() {
        push_word(&mut current_block, &mut current_line, word);
    }
    finish_line(&mut current_block, &mut current_line);
    finish_block(&mut blocks, &mut current_block);

    let plain_text = blocks
        .iter()
        .map(|block| {
            block
                .lines
                .iter()
                .map(|line| {
                    line.words
                        .iter()
                        .map(|word| word.text.trim())
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mean_confidence = if confidences.is_empty() {
        None
    } else {
        Some(confidences.iter().sum::<f32>() / confidences.len() as f32)
    };

    Ok(OcrPage {
        page_index,
        image_width_px,
        image_height_px,
        dpi,
        orientation_deg: 0,
        blocks,
        plain_text,
        mean_confidence,
    })
}

#[derive(Debug, Clone, Copy)]
enum SpanKind {
    Line,
    Word,
    Other,
}

#[derive(Default)]
struct HocrAttrs {
    class: Option<String>,
    title: Option<String>,
}

fn collect_attrs(
    reader: &Reader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
) -> anyhow::Result<HocrAttrs> {
    let mut attrs = HocrAttrs::default();
    for attr in start.attributes().with_checks(false) {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .decode_and_unescape_value(reader.decoder())?
            .into_owned();
        match key {
            "class" => attrs.class = Some(value),
            "title" => attrs.title = Some(value),
            _ => {}
        }
    }
    Ok(attrs)
}

fn parse_word_title(title: Option<&str>) -> anyhow::Result<(BBoxPx, Option<f32>)> {
    let mut bbox = None;
    let mut confidence = None;

    if let Some(title) = title {
        for part in title.split(';').map(str::trim) {
            let mut tokens = part.split_whitespace();
            match tokens.next() {
                Some("bbox") => {
                    let left = parse_u32_token(tokens.next(), "bbox left")?;
                    let top = parse_u32_token(tokens.next(), "bbox top")?;
                    let right = parse_u32_token(tokens.next(), "bbox right")?;
                    let bottom = parse_u32_token(tokens.next(), "bbox bottom")?;
                    bbox = Some(BBoxPx {
                        left,
                        top,
                        right,
                        bottom,
                    });
                }
                Some("x_wconf") => {
                    if let Some(value) = tokens.next() {
                        confidence = value.parse::<f32>().ok();
                    }
                }
                _ => {}
            }
        }
    }

    Ok((
        bbox.unwrap_or(BBoxPx {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }),
        confidence,
    ))
}

fn parse_u32_token(token: Option<&str>, field: &str) -> anyhow::Result<u32> {
    token
        .ok_or_else(|| anyhow!("missing {field}"))?
        .parse::<u32>()
        .with_context(|| format!("invalid {field}"))
}

fn push_word(
    current_block: &mut Option<OcrBlock>,
    current_line: &mut Option<OcrLine>,
    word: OcrWord,
) {
    if current_block.is_none() {
        *current_block = Some(OcrBlock { lines: Vec::new() });
    }
    if current_line.is_none() {
        *current_line = Some(OcrLine { words: Vec::new() });
    }
    if !word.text.trim().is_empty() {
        current_line.as_mut().unwrap().words.push(word);
    }
}

fn finish_line(current_block: &mut Option<OcrBlock>, current_line: &mut Option<OcrLine>) {
    if let Some(line) = current_line.take() {
        if !line.words.is_empty() {
            if current_block.is_none() {
                *current_block = Some(OcrBlock { lines: Vec::new() });
            }
            current_block.as_mut().unwrap().lines.push(line);
        }
    }
}

fn finish_block(blocks: &mut Vec<OcrBlock>, current_block: &mut Option<OcrBlock>) {
    if let Some(block) = current_block.take() {
        if !block.lines.is_empty() {
            blocks.push(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn bundled_tesseract_runs() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("tesseract");
        let exe = dir.join("tesseract-x86_64-pc-windows-msvc.exe");
        let output = Command::new(&exe)
            .arg("--version")
            .current_dir(&dir)
            .env("TESSDATA_PREFIX", dir.join("tessdata"))
            .output()
            .unwrap_or_else(|error| panic!("failed to run {}: {error}", exe.display()));
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("tesseract 5.5.0"), "{stdout}");

        let langs = Command::new(&exe)
            .arg("--list-langs")
            .current_dir(&dir)
            .env("TESSDATA_PREFIX", dir.join("tessdata"))
            .output()
            .unwrap();
        assert!(langs.status.success());
        let langs_stdout = String::from_utf8_lossy(&langs.stdout);
        assert!(
            langs_stdout.lines().any(|line| line.trim() == "eng"),
            "{langs_stdout}"
        );
    }

    #[test]
    fn parses_basic_hocr_words() {
        let hocr = r#"
          <html><body><div class='ocr_page'>
            <div class='ocr_carea'>
              <span class='ocr_line'>
                <span class='ocrx_word' title='bbox 10 20 50 40; x_wconf 91'>Hello</span>
                <span class='ocrx_word' title='bbox 55 20 90 40; x_wconf 87'>PDF</span>
              </span>
            </div>
          </div></body></html>
        "#;
        let page = parse_hocr(hocr, 0, 100, 100, 200).unwrap();
        assert_eq!(page.plain_text, "Hello PDF");
        assert_eq!(page.blocks[0].lines[0].words.len(), 2);
        assert_eq!(page.mean_confidence, Some(89.0));
    }
}
