pub mod query;

use std::{path::Path, time::Instant};

use query::{build_match_expr, QueryError};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
const START_MARK: &str = "\u{1}";
const END_MARK: &str = "\u{2}";

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub document_id: i64,
    pub display_name: String,
    pub page_number: i64,
    pub page_id: i64,
    pub snippet_html: String,
    pub bm25_score: f32,
    pub document_ingested_at: i64,
    pub ocr_engine: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_hits: i64,
    pub took_ms: u32,
    pub query_warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub engine: Option<String>,
    #[serde(default)]
    pub sort: SearchSort,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchSort {
    #[default]
    #[serde(alias = "Relevance")]
    Relevance,
    #[serde(alias = "NewestFirst")]
    NewestFirst,
    #[serde(alias = "OldestFirst")]
    OldestFirst,
}

#[derive(Debug, Clone, Serialize)]
pub struct RebuildReport {
    pub documents: i64,
    pub pages: i64,
    pub took_ms: u32,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error("invalid OCR engine filter: {0}")]
    InvalidEngine(String),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    DbOpen(#[from] crate::db::DbError),
}

pub fn search_document_db(
    db_path: &Path,
    document_id: i64,
    query: &str,
) -> Result<Vec<SearchHit>, SearchError> {
    let (match_expr, _) = build_match_expr(query)?;
    let connection = crate::db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(
        "SELECT
            d.id AS document_id,
            d.display_name,
            d.original_path,
            p.id AS page_id,
            p.page_number,
            snippet(pages_fts, 0, ?3, ?4, '…', 16) AS snippet_html,
            bm25(pages_fts) AS bm25_score,
            d.ingested_at,
            d.ocr_engine
         FROM pages_fts
         JOIN pages p ON p.id = pages_fts.rowid
         JOIN documents d ON d.id = p.document_id
         WHERE pages_fts MATCH ?1
           AND d.id = ?2
           AND d.status IN ('done', 'partial_success')
           AND d.deleted_at IS NULL
         ORDER BY p.page_number ASC
         LIMIT 100",
    )?;
    let hits = statement
        .query_map(
            params![match_expr, document_id, START_MARK, END_MARK],
            |row| {
                let display_name = row
                    .get::<_, Option<String>>(1)?
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| basename(&row.get::<_, String>(2).unwrap_or_default()));
                let snippet: String = row.get(5)?;
                let score = row.get::<_, f64>(6)? as f32;
                Ok(SearchHit {
                    document_id: row.get(0)?,
                    display_name,
                    page_id: row.get(3)?,
                    page_number: row.get(4)?,
                    snippet_html: escape_snippet(&snippet),
                    bm25_score: score,
                    document_ingested_at: row.get(7)?,
                    ocr_engine: row.get(8)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hits)
}

pub fn search_db(db_path: &Path, query: SearchQuery) -> Result<SearchResult, SearchError> {
    let (match_expr, query_warnings) = build_match_expr(&query.q)?;
    let limit = normalize_limit(query.limit);
    let offset = i64::from(query.offset);
    let engine = normalize_engine(query.engine)?;
    let order_by = match query.sort {
        SearchSort::Relevance => "bm25_score ASC",
        SearchSort::NewestFirst => "d.ingested_at DESC, bm25_score ASC",
        SearchSort::OldestFirst => "d.ingested_at ASC, bm25_score ASC",
    };

    let started = Instant::now();
    let connection = crate::db::open_connection_at(db_path)?;

    let count_sql = "SELECT COUNT(*)
         FROM pages_fts
         JOIN pages p ON p.id = pages_fts.rowid
         JOIN documents d ON d.id = p.document_id
         WHERE pages_fts MATCH ?1
           AND (?2 IS NULL OR d.ingested_at >= ?2)
           AND (?3 IS NULL OR d.ingested_at <= ?3)
           AND (?4 IS NULL OR d.ocr_engine = ?4)
           AND d.status IN ('done', 'partial_success')
           AND d.deleted_at IS NULL";
    let total_hits = connection.query_row(
        count_sql,
        params![match_expr, query.date_from, query.date_to, engine],
        |row| row.get::<_, i64>(0),
    )?;

    let sql = format!(
        "SELECT
            d.id AS document_id,
            d.display_name,
            d.original_path,
            p.id AS page_id,
            p.page_number,
            snippet(pages_fts, 0, ?7, ?8, '…', 16) AS snippet_html,
            bm25(pages_fts) AS bm25_score,
            d.ingested_at,
            d.ocr_engine
         FROM pages_fts
         JOIN pages p ON p.id = pages_fts.rowid
         JOIN documents d ON d.id = p.document_id
         WHERE pages_fts MATCH ?1
           AND (?2 IS NULL OR d.ingested_at >= ?2)
           AND (?3 IS NULL OR d.ingested_at <= ?3)
           AND (?4 IS NULL OR d.ocr_engine = ?4)
           AND d.status IN ('done', 'partial_success')
           AND d.deleted_at IS NULL
         ORDER BY {order_by}
         LIMIT ?5 OFFSET ?6"
    );

    let mut statement = connection.prepare(&sql)?;
    let hits = statement
        .query_map(
            params![
                match_expr,
                query.date_from,
                query.date_to,
                engine,
                i64::from(limit),
                offset,
                START_MARK,
                END_MARK
            ],
            |row| {
                let display_name = row
                    .get::<_, Option<String>>(1)?
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| basename(&row.get::<_, String>(2).unwrap_or_default()));
                let snippet: String = row.get(5)?;
                let score = row.get::<_, f64>(6)? as f32;
                Ok(SearchHit {
                    document_id: row.get(0)?,
                    display_name,
                    page_id: row.get(3)?,
                    page_number: row.get(4)?,
                    snippet_html: escape_snippet(&snippet),
                    bm25_score: score,
                    document_ingested_at: row.get(7)?,
                    ocr_engine: row.get(8)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    let took_ms = elapsed_ms(started);
    info!(
        took_ms,
        total_hits,
        returned_hits = hits.len(),
        sort = ?query.sort,
        "search query completed"
    );

    Ok(SearchResult {
        hits,
        total_hits,
        took_ms,
        query_warnings,
    })
}

pub fn rebuild_index(db_path: &Path) -> Result<RebuildReport, SearchError> {
    let started = Instant::now();
    let mut connection = crate::db::open_connection_at(db_path)?;
    let transaction = connection.transaction()?;
    transaction.execute("INSERT INTO pages_fts(pages_fts) VALUES('rebuild')", [])?;
    transaction.execute("INSERT INTO pages_fts(pages_fts) VALUES('optimize')", [])?;

    let documents = transaction.query_row(
        "SELECT COUNT(*) FROM documents
         WHERE status IN ('done', 'partial_success') AND deleted_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let pages = transaction.query_row(
        "SELECT COUNT(*)
         FROM pages p
         JOIN documents d ON d.id = p.document_id
         WHERE d.status IN ('done', 'partial_success') AND d.deleted_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    transaction.commit()?;

    let took_ms = elapsed_ms(started);
    info!(took_ms, documents, pages, "search index rebuilt");
    Ok(RebuildReport {
        documents,
        pages,
        took_ms,
    })
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}

fn normalize_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

fn normalize_engine(engine: Option<String>) -> Result<Option<String>, SearchError> {
    match engine.map(|value| value.trim().to_string()) {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if value == "tesseract" || value == "rapidocr" => Ok(Some(value)),
        Some(value) => Err(SearchError::InvalidEngine(value)),
        None => Ok(None),
    }
}

fn escape_snippet(snippet: &str) -> String {
    let mut escaped = escape_html(snippet);
    escaped = escaped.replace(START_MARK, "<mark>");
    escaped.replace(END_MARK, "</mark>")
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn escapes_snippet_text_but_preserves_marks() {
        let snippet = format!("a < b & {START_MARK}needle{END_MARK} \"quoted\"");
        assert_eq!(
            escape_snippet(&snippet),
            "a &lt; b &amp; <mark>needle</mark> &quot;quoted&quot;"
        );
    }

    #[test]
    fn search_handles_thousand_pages_quickly() {
        let (temp_dir, db_path) = fixture_db();
        seed_pages(&db_path, 1_000, "needleunique");

        let result = search_db(
            &db_path,
            SearchQuery {
                q: "needleunique".to_string(),
                limit: 50,
                offset: 0,
                date_from: None,
                date_to: None,
                engine: None,
                sort: SearchSort::Relevance,
            },
        )
        .unwrap();

        tracing::info!(
            took_ms = result.took_ms,
            "1000-page FTS5 search benchmark completed"
        );
        assert_eq!(result.total_hits, 1_000);
        assert_eq!(result.hits.len(), 50);
        assert!(
            result.took_ms < 200,
            "search took {} ms in {}",
            result.took_ms,
            temp_dir.path().display()
        );
    }

    #[test]
    fn rebuild_index_reports_indexed_counts() {
        let (_temp_dir, db_path) = fixture_db();
        seed_pages(&db_path, 3, "rebuildtoken");

        let report = rebuild_index(&db_path).unwrap();

        assert_eq!(report.documents, 1);
        assert_eq!(report.pages, 3);
    }

    fn fixture_db() -> (TempDir, std::path::PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("search-test.db");
        let connection = crate::db::open_connection_at(&db_path).unwrap();
        connection
            .execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../../migrations/003_phase2.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../../migrations/004_phase3.sql"))
            .unwrap();
        (temp_dir, db_path)
    }

    fn seed_pages(db_path: &Path, count: usize, term: &str) {
        let mut connection = crate::db::open_connection_at(db_path).unwrap();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute(
                "INSERT INTO documents(
                    sha256, original_path, output_path, display_name, page_count,
                    ocr_engine, ai_provider, status, ingested_at, updated_at
                 ) VALUES(?1, ?2, ?3, ?4, ?5, 'tesseract', 'none', 'done', 1, 1)",
                params![
                    format!("sha-{count}"),
                    "C:\\Docs\\sample.pdf",
                    "C:\\Docs\\out.pdf",
                    "Sample.pdf",
                    count as i64
                ],
            )
            .unwrap();
        let document_id = transaction.last_insert_rowid();
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO pages(document_id, page_number, text, ocr_status)
                     VALUES(?1, ?2, ?3, 'ocr_done')",
                )
                .unwrap();
            for index in 0..count {
                statement
                    .execute(params![
                        document_id,
                        (index + 1) as i64,
                        format!(
                            "Synthetic page {} contains {term} and invoice text.",
                            index + 1
                        )
                    ])
                    .unwrap();
            }
        }
        transaction.commit().unwrap();
    }
}
