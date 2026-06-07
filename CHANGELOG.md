# Changelog

All notable changes to PDF-Parser will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-06-06

### Added
- macOS (Apple Silicon) build target and cross-platform release workflow
- Duplicate-upload protection using content-hash detection
- Reprocess a document with a chosen OCR engine from the library preview
- Periodic rescan of watched folders with persistent de-duplication
- New application logo and icons

### Fixed
- Auto-updater endpoint pointed at an unset owner placeholder, so update checks never worked
- PDF preview failed to load in the packaged app (asset-protocol CSP was missing `http://asset.localhost`)
- Settings panel could freeze when opened
- Cross-platform path handling and OCR engine availability

## [0.1.0] - 2026-05-27

### Added
- OCR pipeline using Tesseract 5 + pdfium-render + lopdf (Phase 1)
- AI naming via OpenRouter and Ollama with secure key storage (Phase 2)
- Library and Settings panels (Phase 2)
- FTS5 full-text search with snippet highlighting (Phase 3)
- Folder watcher with stable-write detection (Phase 4)
- Cancellable job queue with aggregate progress UX (Phase 4)
- RapidOCR opt-in for higher-accuracy OCR (Phase 6)
- Document-aware RAG chat with local embeddings and grounded citations (Phase 5)
- GitHub Actions release pipeline with Tauri auto-updater (Phase 7)
- Polish pass: empty states, keyboard a11y, preview reading tools, About dialog, log viewer (Phase 8)
