-- PRAGMAs set in Rust code on connection open, not via migration:
--   PRAGMA journal_mode=WAL;
--   PRAGMA synchronous=NORMAL;
--   PRAGMA cache_size=-65536;
--   PRAGMA temp_store=MEMORY;
--   PRAGMA busy_timeout=5000;
--   PRAGMA journal_size_limit=67108864;
--   PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  sha256 TEXT NOT NULL UNIQUE,
  original_path TEXT NOT NULL,
  output_path TEXT,
  display_name TEXT,
  ai_summary TEXT,
  page_count INTEGER NOT NULL DEFAULT 0,
  ocr_engine TEXT,             -- 'tesseract' | 'rapidocr' | null
  ai_provider TEXT,            -- 'openrouter:<model>' | 'ollama:<model>' | 'none'
  status TEXT NOT NULL,        -- queued | rasterizing | ocr | naming | indexing | done | partial_success | error | needs_password
  error_message TEXT,
  ingested_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX idx_documents_status ON documents(status);
CREATE INDEX idx_documents_ingested_at ON documents(ingested_at DESC);

CREATE TABLE IF NOT EXISTS pages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  page_number INTEGER NOT NULL,
  text TEXT NOT NULL DEFAULT '',
  ocr_status TEXT NOT NULL,    -- native_text | ocr_done | ocr_skipped | ocr_failed
  mean_confidence REAL,
  width_px INTEGER,
  height_px INTEGER,
  dpi INTEGER,
  rotation INTEGER NOT NULL DEFAULT 0,
  UNIQUE(document_id, page_number)
);
CREATE INDEX idx_pages_document_id ON pages(document_id, page_number);

-- FTS5 virtual table with content table sync
CREATE VIRTUAL TABLE pages_fts USING fts5(text, content='pages', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
-- Triggers keep pages_fts in sync with pages
CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
  INSERT INTO pages_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, text) VALUES('delete', old.id, old.text);
END;
CREATE TRIGGER pages_au AFTER UPDATE ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, text) VALUES('delete', old.id, old.text);
  INSERT INTO pages_fts(rowid, text) VALUES (new.id, new.text);
END;

-- sqlite-vec: defer creation to phase 5; for phase 0 just attempt to load the extension and log success/failure
-- (Do NOT create the vec0 table yet.)

CREATE TABLE IF NOT EXISTS chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
  char_start INTEGER NOT NULL,
  char_end INTEGER NOT NULL,
  token_count INTEGER NOT NULL,
  text TEXT NOT NULL
);
CREATE INDEX idx_chunks_page ON chunks(page_id);
CREATE INDEX idx_chunks_doc ON chunks(document_id);

CREATE TABLE IF NOT EXISTS jobs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  document_id INTEGER REFERENCES documents(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,           -- 'ingest' | 'reocr' | 'rename' | 'index'
  status TEXT NOT NULL,         -- 'queued' | 'running' | 'done' | 'error' | 'cancelled'
  error_message TEXT,
  started_at INTEGER,
  finished_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_jobs_status ON jobs(status, created_at);

CREATE TABLE IF NOT EXISTS watched_folders (
  path TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL DEFAULT 1,
  added_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
