# PDF-Parser

A Windows desktop app for turning PDFs into a searchable, AI-named local library.

![PDF-Parser library screenshot](docs/screenshot-library.png)

## Features

- **Folder watcher (primary workflow)** — drop PDFs into a watched folder and they are detected, OCR'd, named, and added to your library automatically, with stable-write detection, a configurable rescan interval, queue progress, retries, and cancellation
- Manual **Upload** page for one-off files: drag-and-drop or pick PDFs on demand
- OCR pipeline with Tesseract 5, pdfium-render, and lopdf searchable-PDF output
- RapidOCR PP-OCRv5 opt-in model download (SHA256 verified) for higher-accuracy and CJK scans
- Optional AI naming and chat through a local Ollama server — offline, no API key required
- Folders, Upload, Library, Search, Chat, and Settings panels in a Tauri 2 desktop shell
- FTS5 full-text search with highlighted snippets and saved searches
- Document-aware RAG chat with local embeddings and grounded citations
- GitHub Actions release pipeline with MSI, NSIS, and Tauri updater artifacts

## Quick start

1. Download the latest MSI from the GitHub Releases page.
2. Install and launch PDF-Parser.
3. On the **Folders** page, add a watched folder — every new PDF dropped there is processed automatically. This is the primary way to use PDF-Parser.
4. Handling a single file? Use the **Upload** page to drag in or pick PDFs manually.
5. Optional: open Settings to pick your OCR engine or point PDF-Parser at a local Ollama server for AI naming and chat.

PDF-Parser works offline by default. Optional AI naming and chat run against a local Ollama server you configure in Settings.

## Building from source

### Prerequisites

- Windows 10/11 with WebView2
- Node.js 20+ and pnpm 10+
- Rust stable with the `x86_64-pc-windows-msvc` target
- Microsoft C++ Build Tools

### Development

```powershell
pnpm install
pnpm tauri dev
```

### Production bundle

```powershell
pnpm install
pnpm tauri build
```

Fast local bundle check:

```powershell
pnpm tauri build --debug
```

## Architecture

```text
React + shadcn UI
        │ typed IPC/events
        ▼
Tauri commands ── SQLite + FTS5 + sqlite-vec
        │
        ├─ OCR worker pool ── pdfium-render ── Tesseract/RapidOCR
        ├─ AI naming ─────── Ollama (local)
        ├─ RAG chat ──────── fastembed + BGE chunks
        └─ Watcher/updater ─ notify + Tauri updater
```

## Screenshots

`docs/screenshot-library.png` is a transparent placeholder in this non-interactive build environment. To regenerate placeholders, run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\create_placeholder_screenshots.ps1
```

Replace the placeholder with real app screenshots before publishing the release.

## Roadmap

Not in v0.1.0:

- Code signing and SmartScreen reputation
- Cloud OCR providers
- Multi-language UI
- Strict PDF/A validation
- Sandboxed PDF parsing process isolation

## Contributing

This is a personal project. Keep changes small, local-first, and covered by `cargo test --workspace`, clippy, and a Tauri build when UI or packaging changes.

## License

MIT. See [LICENSE](LICENSE).
