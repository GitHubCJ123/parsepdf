use std::{
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("empty filename")]
    EmptyName,
    #[error("file IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsafe output path: {0}")]
    UnsafePath(String),
    #[error("could not choose a unique filename after 999 collisions")]
    TooManyCollisions,
}

pub fn sanitize_display_name(base: &str) -> Result<String, FileError> {
    let mut input = base.trim().to_lowercase();
    if input.ends_with(".pdf") {
        input.truncate(input.len().saturating_sub(4));
    }

    let invalid = Regex::new(r"[^a-z0-9_-]").expect("valid filename sanitizer regex");
    let underscores = Regex::new(r"_+").expect("valid underscore sanitizer regex");
    let mut sanitized = invalid.replace_all(&input, "_").to_string();
    sanitized = underscores.replace_all(&sanitized, "_").to_string();
    sanitized = sanitized.trim_matches(['_', '-', '.']).to_string();
    if sanitized.len() > 80 {
        sanitized.truncate(80);
        sanitized = sanitized.trim_matches(['_', '-', '.']).to_string();
    }

    if sanitized.is_empty() {
        return Err(FileError::EmptyName);
    }

    if is_reserved_windows_name(&sanitized) {
        sanitized = format!("doc_{sanitized}");
    }

    if sanitized.is_empty() {
        return Err(FileError::EmptyName);
    }

    Ok(format!("{sanitized}.pdf"))
}

pub fn sanitize_filename(base: &str, output_dir: &Path) -> Result<PathBuf, FileError> {
    fs::create_dir_all(output_dir)?;
    let canonical_output_dir = output_dir.canonicalize()?;
    let display_name = sanitize_display_name(base)?;
    let stem = display_name
        .strip_suffix(".pdf")
        .unwrap_or(display_name.as_str())
        .to_string();

    for suffix in 1..=999 {
        let filename = if suffix == 1 {
            format!("{stem}.pdf")
        } else {
            format!("{stem}_{suffix}.pdf")
        };
        let candidate = canonical_output_dir.join(&filename);
        let parent = candidate
            .parent()
            .ok_or_else(|| FileError::UnsafePath(candidate.to_string_lossy().into_owned()))?;
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&canonical_output_dir) {
            return Err(FileError::UnsafePath(
                candidate.to_string_lossy().into_owned(),
            ));
        }
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(FileError::TooManyCollisions)
}

fn is_reserved_windows_name(stem: &str) -> bool {
    let first_segment = stem.split(['.', '_', '-']).next().unwrap_or(stem);
    let upper = first_segment.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper[3..].chars().all(|c| ('1'..='9').contains(&c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{distributions::Alphanumeric, Rng};

    fn test_dir(name: &str) -> PathBuf {
        let random = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect::<String>();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("phase2-sanitizer-tests")
            .join(format!("{name}-{random}"));
        fs::create_dir_all(&dir).expect("create sanitizer test dir");
        dir
    }

    #[test]
    fn rejects_empty_name() {
        assert!(matches!(
            sanitize_display_name(""),
            Err(FileError::EmptyName)
        ));
    }

    #[test]
    fn rejects_only_symbols() {
        assert!(matches!(
            sanitize_display_name("...---___///"),
            Err(FileError::EmptyName)
        ));
    }

    #[test]
    fn strips_traversal_segments() {
        let dir = test_dir("traversal");
        let path = sanitize_filename("..\\..\\windows\\system32\\evil", &dir).unwrap();
        assert!(path.starts_with(dir.canonicalize().unwrap()));
        assert_eq!(
            path.file_name().and_then(|v| v.to_str()),
            Some("windows_system32_evil.pdf")
        );
    }

    #[test]
    fn prefixes_windows_dos_names() {
        assert_eq!(sanitize_display_name("CON").unwrap(), "doc_con.pdf");
        assert_eq!(sanitize_display_name("lpt1").unwrap(), "doc_lpt1.pdf");
    }

    #[test]
    fn handles_duplicate_collision() {
        let dir = test_dir("collision");
        fs::write(dir.join("report.pdf"), b"existing").unwrap();
        let path = sanitize_filename("report", &dir).unwrap();
        assert_eq!(
            path.file_name().and_then(|v| v.to_str()),
            Some("report_2.pdf")
        );
    }

    #[test]
    fn truncates_very_long_input() {
        let long = "a".repeat(180);
        let name = sanitize_display_name(&long).unwrap();
        assert_eq!(name.len(), 84);
        assert!(name.ends_with(".pdf"));
    }
}
