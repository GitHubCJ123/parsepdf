use std::{fs, path::Path};

#[test]
fn no_reqwest_callers_in_rust_startup_path() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let startup_files = [
        "src/lib.rs",
        "src/main.rs",
        "src/state.rs",
        "src/db/mod.rs",
        "src/jobs/mod.rs",
        "src/watcher/mod.rs",
    ];

    for relative in startup_files {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("reqwest::") && !source.contains("reqwest::Client"),
            "startup file {} must not create outbound HTTP clients",
            path.display()
        );
    }
}

#[test]
fn network_egress_allowlist_is_documented_in_callers() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed = [
        "src/ai/openrouter.rs",
        "src/ai/ollama.rs",
        "src/ocr/rapidocr_install.rs",
    ];
    for relative in allowed {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            source.contains("reqwest::Client::builder"),
            "{} should keep explicit reqwest client construction for auditability",
            path.display()
        );
    }
}
