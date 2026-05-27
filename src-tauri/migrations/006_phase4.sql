ALTER TABLE watched_folders ADD COLUMN recursive INTEGER NOT NULL DEFAULT 1;
ALTER TABLE watched_folders ADD COLUMN last_error TEXT;
ALTER TABLE watched_folders ADD COLUMN last_scan_at INTEGER;

ALTER TABLE jobs ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE jobs ADD COLUMN engine TEXT;

CREATE INDEX IF NOT EXISTS idx_jobs_origin ON jobs(origin, created_at);
