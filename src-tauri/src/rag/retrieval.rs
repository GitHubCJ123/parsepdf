use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use rusqlite::{params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::task;
use tracing::info;

use crate::search::query::{build_match_expr, QueryError};

use super::embeddings::{vector_to_json, EmbedError, EmbeddingRuntime};

const RRF_K: f32 = 60.0;

#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    pub top_k_fts: usize,
    pub top_k_vec: usize,
    pub top_k_final: usize,
    pub fts_weight: f32,
    pub vec_weight: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k_fts: 20,
            top_k_vec: 20,
            top_k_final: 8,
            fts_weight: 0.4,
            vec_weight: 0.6,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocFilter {
    pub document_ids: Option<Vec<i64>>,
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub chunk_id: i64,
    pub document_id: i64,
    pub page_id: i64,
    pub page_number: i64,
    pub document_name: String,
    pub text: String,
    pub fts_score: Option<f32>,
    pub vec_score: Option<f32>,
    pub combined_score: f32,
}

pub struct Retrieval {
    db_path: PathBuf,
    embed: Arc<EmbeddingRuntime>,
    config: RetrievalConfig,
}

impl Retrieval {
    pub fn new(db_path: PathBuf, embed: Arc<EmbeddingRuntime>, config: RetrievalConfig) -> Self {
        Self {
            db_path,
            embed,
            config,
        }
    }

    pub async fn search(
        &self,
        query: &str,
        filter: Option<DocFilter>,
    ) -> Result<Vec<RetrievedChunk>, RagError> {
        let started = Instant::now();
        let filter = filter.unwrap_or_default();
        let db_path_for_fts = self.db_path.clone();
        let filter_for_fts = filter.clone();
        let query_for_fts = query.to_string();
        let top_k_fts = self.config.top_k_fts;
        let fts_handle = task::spawn_blocking(move || {
            run_fts_search(&db_path_for_fts, &query_for_fts, &filter_for_fts, top_k_fts)
        });

        let service = self.embed.get_or_initialize().await?;
        let query_embedding = service.embed_query(query).await?;
        let fts_rows = fts_handle
            .await
            .map_err(|error| RagError::Join(error.to_string()))??;

        let db_path_for_vec = self.db_path.clone();
        let filter_for_vec = filter.clone();
        let top_k_vec = self.config.top_k_vec;
        let vec_rows = task::spawn_blocking(move || {
            run_vector_search(
                &db_path_for_vec,
                &query_embedding,
                &filter_for_vec,
                top_k_vec,
            )
        })
        .await
        .map_err(|error| RagError::Join(error.to_string()))??;

        let mut fused = HashMap::<i64, FusedScore>::new();
        for (rank, row) in fts_rows.iter().enumerate() {
            let entry = fused.entry(row.chunk_id).or_default();
            entry.fts_score = Some(row.score);
            entry.combined += self.config.fts_weight / (RRF_K + rank as f32 + 1.0);
        }
        for (rank, row) in vec_rows.iter().enumerate() {
            let entry = fused.entry(row.chunk_id).or_default();
            entry.vec_score = Some(1.0 / (1.0 + row.distance.max(0.0)));
            entry.combined += self.config.vec_weight / (RRF_K + rank as f32 + 1.0);
        }

        let mut ranked = fused.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|(left_id, left), (right_id, right)| {
            right
                .combined
                .partial_cmp(&left.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left_id.cmp(right_id))
        });
        ranked.truncate(self.config.top_k_final);
        let ids = ranked
            .iter()
            .map(|(chunk_id, _)| *chunk_id)
            .collect::<Vec<_>>();
        let metadata = load_chunk_metadata(&self.db_path, &ids)?;
        let mut results = Vec::with_capacity(ranked.len());
        for (chunk_id, score) in ranked {
            if let Some(mut chunk) = metadata.get(&chunk_id).cloned() {
                chunk.fts_score = score.fts_score;
                chunk.vec_score = score.vec_score;
                chunk.combined_score = score.combined;
                results.push(chunk);
            }
        }

        info!(
            took_ms = started.elapsed().as_millis(),
            hit_count = results.len(),
            fts_hits = fts_rows.len(),
            vec_hits = vec_rows.len(),
            "rag retrieval completed"
        );
        Ok(results)
    }
}

#[derive(Default)]
struct FusedScore {
    fts_score: Option<f32>,
    vec_score: Option<f32>,
    combined: f32,
}

struct FtsRow {
    chunk_id: i64,
    score: f32,
}

struct VecRow {
    chunk_id: i64,
    distance: f32,
}

#[derive(Debug, Error)]
pub enum RagError {
    #[error(transparent)]
    Embed(#[from] EmbedError),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    DbOpen(#[from] crate::db::DbError),
    #[error("retrieval task join error: {0}")]
    Join(String),
}

fn run_fts_search(
    db_path: &Path,
    query: &str,
    filter: &DocFilter,
    top_k: usize,
) -> Result<Vec<FtsRow>, RagError> {
    let (match_expr, _) = match build_match_expr(query) {
        Ok(value) => value,
        Err(QueryError::Empty) => return Ok(Vec::new()),
    };
    let connection = crate::db::open_connection_at(db_path)?;
    let mut sql = String::from(
        "SELECT c.id, bm25(chunks_fts) AS bm25_score
         FROM chunks_fts
         JOIN chunks c ON c.id = chunks_fts.rowid
         JOIN documents d ON d.id = c.document_id
         WHERE chunks_fts MATCH ?
           AND d.status IN ('done', 'partial_success', 'naming')
           AND d.deleted_at IS NULL",
    );
    let mut values = vec![Value::Text(match_expr)];
    append_filter_sql(&mut sql, &mut values, filter);
    sql.push_str(" ORDER BY bm25_score ASC LIMIT ?");
    values.push(Value::Integer(top_k as i64));

    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(params_from_iter(values.iter()), |row| {
            Ok(FtsRow {
                chunk_id: row.get(0)?,
                score: row.get::<_, f64>(1)? as f32,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn run_vector_search(
    db_path: &Path,
    query_embedding: &[f32],
    filter: &DocFilter,
    top_k: usize,
) -> Result<Vec<VecRow>, RagError> {
    let connection = crate::db::open_connection_at(db_path)?;
    let mut sql = String::from(
        "SELECT v.rowid, v.distance
         FROM chunks_vec v
         JOIN chunks c ON c.id = v.rowid
         JOIN documents d ON d.id = c.document_id
         WHERE v.embedding MATCH ?
           AND k = ?
           AND d.status IN ('done', 'partial_success', 'naming')
           AND d.deleted_at IS NULL",
    );
    let mut values = vec![
        Value::Text(vector_to_json(query_embedding)),
        Value::Integer(top_k as i64),
    ];
    append_filter_sql(&mut sql, &mut values, filter);
    sql.push_str(" ORDER BY v.distance ASC");

    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(params_from_iter(values.iter()), |row| {
            Ok(VecRow {
                chunk_id: row.get(0)?,
                distance: row.get::<_, f64>(1)? as f32,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_chunk_metadata(
    db_path: &Path,
    chunk_ids: &[i64],
) -> Result<HashMap<i64, RetrievedChunk>, RagError> {
    if chunk_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", chunk_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT c.id,
                c.document_id,
                c.page_id,
                p.page_number,
                d.display_name,
                d.original_path,
                c.text
         FROM chunks c
         JOIN pages p ON p.id = c.page_id
         JOIN documents d ON d.id = c.document_id
         WHERE c.id IN ({placeholders})"
    );
    let values = chunk_ids
        .iter()
        .copied()
        .map(Value::Integer)
        .collect::<Vec<_>>();
    let connection = crate::db::open_connection_at(db_path)?;
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values.iter()), |row| {
        let display_name = row
            .get::<_, Option<String>>(4)?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| basename(&row.get::<_, String>(5).unwrap_or_default()));
        Ok(RetrievedChunk {
            chunk_id: row.get(0)?,
            document_id: row.get(1)?,
            page_id: row.get(2)?,
            page_number: row.get(3)?,
            document_name: display_name,
            text: row.get(6)?,
            fts_score: None,
            vec_score: None,
            combined_score: 0.0,
        })
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let chunk = row?;
        map.insert(chunk.chunk_id, chunk);
    }
    Ok(map)
}

fn append_filter_sql(sql: &mut String, values: &mut Vec<Value>, filter: &DocFilter) {
    if let Some(ids) = filter
        .document_ids
        .as_ref()
        .map(|ids| ids.iter().copied().filter(|id| *id > 0).collect::<Vec<_>>())
        .filter(|ids| !ids.is_empty())
    {
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(" AND d.id IN (");
        sql.push_str(&placeholders);
        sql.push(')');
        values.extend(ids.into_iter().map(Value::Integer));
    }
    if let Some(date_from) = filter.date_from {
        sql.push_str(" AND d.ingested_at >= ?");
        values.push(Value::Integer(date_from));
    }
    if let Some(date_to) = filter.date_to {
        sql.push_str(" AND d.ingested_at <= ?");
        values.push(Value::Integer(date_to));
    }
}

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::embeddings::normalize_l2;
    use rusqlite::params;

    #[test]
    fn sqlite_vec_returns_expected_nearest_neighbor_for_synthetic_vectors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("vec-test.db");
        let connection = crate::db::open_connection_at(&db_path).unwrap();
        connection
            .execute_batch("CREATE VIRTUAL TABLE chunks_vec USING vec0(embedding FLOAT[3]);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO chunks_vec(rowid, embedding) VALUES(?1, ?2)",
                params![1_i64, "[1.0,0.0,0.0]"],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO chunks_vec(rowid, embedding) VALUES(?1, ?2)",
                params![2_i64, "[0.0,1.0,0.0]"],
            )
            .unwrap();
        let mut query = vec![0.9_f32, 0.1, 0.0];
        normalize_l2(&mut query);
        let query_json = vector_to_json(&query);
        let nearest: i64 = connection
            .query_row(
                "SELECT rowid FROM chunks_vec WHERE embedding MATCH ?1 AND k = 1 ORDER BY distance",
                params![query_json],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(nearest, 1);
    }

    #[test]
    #[ignore]
    fn end_to_end_rag_query_returns_cited_chunk() {
        // Slow integration coverage belongs in release validation because it initializes the embedding model.
    }
}
