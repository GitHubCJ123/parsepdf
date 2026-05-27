CREATE TABLE IF NOT EXISTS pending_renames (
  document_id INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  proposed_name TEXT NOT NULL,
  summary TEXT,
  provider TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  reviewed INTEGER NOT NULL DEFAULT 0,
  user_edit TEXT
);

ALTER TABLE documents ADD COLUMN ai_naming_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE documents ADD COLUMN deleted_at INTEGER;

CREATE INDEX IF NOT EXISTS idx_pending_renames_reviewed ON pending_renames(reviewed, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_deleted_at ON documents(deleted_at);
