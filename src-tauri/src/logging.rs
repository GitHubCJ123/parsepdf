use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use chrono::Local;
use regex::Regex;
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter::LevelFilter,
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    prelude::*,
    registry::LookupSpan,
};

/// Redact sensitive content from log messages.
/// Replaces:
///   - Anything matching the OpenRouter/Anthropic/OpenAI API key pattern
///   - Full file paths with just the basename
///   - Long text blobs (>200 chars) with "<TEXT len=N>"
pub fn redact(msg: &str) -> String {
    let normalized_msg = msg.replace("\\\\?\\", "");
    let without_keys = api_key_regex().replace_all(&normalized_msg, "<API_KEY>");
    let without_windows_paths = windows_path_regex().replace_all(&without_keys, "$1");
    let without_unc_paths = unc_path_regex().replace_all(&without_windows_paths, "$1");
    let without_paths = unix_path_regex().replace_all(&without_unc_paths, "$1$2");
    let redacted = without_paths.into_owned();
    let char_len = redacted.chars().count();
    if char_len > 200 {
        format!("<TEXT len={char_len}>")
    } else {
        redacted
    }
}

pub fn install_tracing_subscriber(log_dir: &Path) -> Result<WorkerGuard, anyhow::Error> {
    fs::create_dir_all(log_dir)?;
    let writer = DailyLogWriter::new(log_dir.to_path_buf());
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);
    let layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .event_format(RedactingFormatter)
        .with_writer(non_blocking)
        .with_filter(LevelFilter::INFO);

    tracing_subscriber::registry().with(layer).try_init()?;
    Ok(guard)
}

pub fn current_log_path(log_dir: &Path) -> PathBuf {
    log_dir.join(format!("app-{}.log", Local::now().format("%Y-%m-%d")))
}

pub fn newest_log_path(log_dir: &Path) -> Option<PathBuf> {
    let current = current_log_path(log_dir);
    if current.exists() {
        return Some(current);
    }

    fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy();
            if !name.starts_with("app-") || !name.ends_with(".log") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

struct DailyLogWriter {
    log_dir: PathBuf,
    current_day: String,
    file: Option<File>,
}

impl DailyLogWriter {
    fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            current_day: String::new(),
            file: None,
        }
    }

    fn ensure_file(&mut self) -> io::Result<&mut File> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        if self.current_day != today || self.file.is_none() {
            fs::create_dir_all(&self.log_dir)?;
            let path = self.log_dir.join(format!("app-{today}.log"));
            self.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
            self.current_day = today;
        }
        Ok(self.file.as_mut().expect("log file initialized"))
    }
}

impl Write for DailyLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.ensure_file()?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()
        } else {
            Ok(())
        }
    }
}

struct RedactingFormatter;

impl<S, N> FormatEvent<S, N> for RedactingFormatter
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut visitor = RedactingVisitor::default();
        event.record(&mut visitor);
        let metadata = event.metadata();
        let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z");

        write!(
            writer,
            "{timestamp} {:<5} {} ",
            metadata.level(),
            metadata.target()
        )?;

        if let Some(message) = visitor.message.as_deref() {
            writer.write_str(&redact(message))?;
        }

        for (name, value) in visitor.fields {
            if visitor.message.is_some() {
                writer.write_char(' ')?;
            }
            write!(writer, "{name}={}", redact(&value))?;
        }

        writer.write_char('\n')
    }
}

#[derive(Default)]
struct RedactingVisitor {
    message: Option<String>,
    fields: Vec<(&'static str, String)>,
}

impl tracing::field::Visit for RedactingVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_value(field, value.to_string());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_value(field, value.to_string());
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.record_value(field, value.to_string());
    }
}

impl RedactingVisitor {
    fn record_value(&mut self, field: &tracing::field::Field, value: String) {
        let value = trim_debug_quotes(value);
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push((field.name(), value));
        }
    }
}

fn trim_debug_quotes(value: String) -> String {
    value
        .strip_prefix('"')
        .and_then(|stripped| stripped.strip_suffix('"'))
        .unwrap_or(&value)
        .replace("\\\"", "\"")
}

fn api_key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(?:sk-or-v1-[A-Za-z0-9._-]+|sk-ant-[A-Za-z0-9._-]+|sk-[A-Za-z0-9._-]{12,})\b",
        )
        .unwrap()
    })
}

fn windows_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)\b[A-Z]:\\(?:[^\\/:*?"<>|\r\n]+\\)+([^\\/:*?"<>|\r\n]+)"#).unwrap()
    })
}

fn unc_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\\\\(?:[^\\/:*?"<>|\r\n]+\\)+([^\\/:*?"<>|\r\n]+)"#).unwrap())
}

fn unix_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(^|[\s=:'"(\[])/(?:[^/\s]+/)+([^/\s]+)"#).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::info;
    use tracing_subscriber::fmt::MakeWriter;

    #[test]
    fn tracing_event_redacts_api_keys() {
        let output = capture_event(|| info!("api_key={}", "sk-or-v1-abc..xyz"));
        assert!(output.contains("<API_KEY>"));
        assert!(!output.contains("sk-or-v1-abc..xyz"));
    }

    #[test]
    fn tracing_event_redacts_full_paths_to_basename() {
        let output =
            capture_event(|| info!("processing {}", "/very/long/sensitive/path/secret.pdf"));
        assert!(output.contains("secret.pdf"));
        assert!(!output.contains("/very/long/sensitive/path"));
    }

    #[test]
    fn redact_strips_windows_verbatim_prefixes() {
        assert_eq!(
            redact(
                r"pdfium=\\?\C:\Users\jacob\Projects\PDF-Parser\src-tauri\binaries\pdfium\pdfium.dll"
            ),
            "pdfium=pdfium.dll"
        );
    }

    #[test]
    fn redact_elides_long_ocr_text() {
        let text = "Recognized text ".repeat(16);
        assert_eq!(
            redact(&text),
            format!("<TEXT len={}>", text.chars().count())
        );
    }

    fn capture_event(action: impl FnOnce()) -> String {
        let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = TestWriter(buffer.clone());
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .event_format(RedactingFormatter)
                .with_writer(writer),
        );
        let dispatch = tracing::Dispatch::new(subscriber);
        tracing::dispatcher::with_default(&dispatch, action);
        let bytes = buffer.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[derive(Clone)]
    struct TestWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for TestWriter {
        type Writer = TestSink;

        fn make_writer(&'a self) -> Self::Writer {
            TestSink(self.0.clone())
        }
    }

    struct TestSink(Arc<Mutex<Vec<u8>>>);

    impl Write for TestSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
