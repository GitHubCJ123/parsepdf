use std::{
    env,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{watch, OnceCell};
use tracing::{info, warn};

const MODEL_ID: &str = "BAAI/bge-small-en-v1.5";
const EMBEDDING_DIM: usize = 384;
const EMBEDDING_BATCH_SIZE: usize = 64;
const MODEL_MAX_LENGTH: usize = 512;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingRuntimeStatus {
    pub state: String,
    pub model_id: &'static str,
    pub dim: usize,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct EmbeddingRuntime {
    cache_dir: PathBuf,
    service: Arc<OnceCell<Arc<EmbeddingService>>>,
    status: watch::Sender<EmbeddingRuntimeStatus>,
}

impl EmbeddingRuntime {
    pub fn new(cache_dir: PathBuf) -> Self {
        let (status, _) = watch::channel(EmbeddingRuntimeStatus {
            state: "idle".to_string(),
            model_id: MODEL_ID,
            dim: EMBEDDING_DIM,
            error: None,
        });
        Self {
            cache_dir,
            service: Arc::new(OnceCell::new()),
            status,
        }
    }

    pub fn from_default_cache() -> Result<Self, EmbedError> {
        Ok(Self::new(default_cache_dir()?))
    }

    pub fn prewarm(&self) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = runtime.get_or_initialize().await {
                warn!(error = %error, "embedding model prewarm failed");
            }
        });
    }

    pub async fn get_or_initialize(&self) -> Result<Arc<EmbeddingService>, EmbedError> {
        if let Some(service) = self.service.get() {
            return Ok(service.clone());
        }

        let cache_dir = self.cache_dir.clone();
        let status = self.status.clone();
        let service = self
            .service
            .get_or_try_init(|| async move {
                let _ = status.send(EmbeddingRuntimeStatus {
                    state: "initializing".to_string(),
                    model_id: MODEL_ID,
                    dim: EMBEDDING_DIM,
                    error: None,
                });
                match EmbeddingService::new(cache_dir).await {
                    Ok(service) => {
                        let _ = status.send(EmbeddingRuntimeStatus {
                            state: "ready".to_string(),
                            model_id: MODEL_ID,
                            dim: EMBEDDING_DIM,
                            error: None,
                        });
                        Ok(Arc::new(service))
                    }
                    Err(error) => {
                        let _ = status.send(EmbeddingRuntimeStatus {
                            state: "error".to_string(),
                            model_id: MODEL_ID,
                            dim: EMBEDDING_DIM,
                            error: Some(error.to_string()),
                        });
                        Err(error)
                    }
                }
            })
            .await?;
        Ok(service.clone())
    }

    pub fn status(&self) -> EmbeddingRuntimeStatus {
        self.status.borrow().clone()
    }
}

pub struct EmbeddingService {
    model: Arc<Mutex<TextEmbedding>>,
    pub model_id: &'static str,
    pub dim: usize,
    pub batch_size: usize,
}

impl EmbeddingService {
    pub async fn new(cache_dir: PathBuf) -> Result<Self, EmbedError> {
        std::fs::create_dir_all(&cache_dir)?;
        let started = Instant::now();
        let model = tokio::task::spawn_blocking(move || {
            let options = InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir)
                .with_max_length(MODEL_MAX_LENGTH)
                .with_show_download_progress(false);
            TextEmbedding::try_new(options).map_err(|error| EmbedError::Model(error.to_string()))
        })
        .await
        .map_err(|error| EmbedError::Join(error.to_string()))??;
        info!(
            took_ms = started.elapsed().as_millis(),
            model_id = MODEL_ID,
            "embedding model initialized"
        );
        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            model_id: MODEL_ID,
            dim: EMBEDDING_DIM,
            batch_size: EMBEDDING_BATCH_SIZE,
        })
    }

    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let model = self.model.clone();
        let batch_size = self.batch_size;
        let dim = self.dim;
        tokio::task::spawn_blocking(move || {
            let mut model = model
                .lock()
                .map_err(|_| EmbedError::Model("embedding model lock poisoned".to_string()))?;
            let mut embeddings = Vec::with_capacity(texts.len());
            for batch in texts.chunks(batch_size) {
                let mut batch_embeddings = model
                    .embed(batch, Some(batch_size))
                    .map_err(|error| EmbedError::Model(error.to_string()))?;
                for embedding in batch_embeddings.iter_mut() {
                    normalize_l2(embedding);
                    if embedding.len() != dim {
                        return Err(EmbedError::Dimension {
                            expected: dim,
                            actual: embedding.len(),
                        });
                    }
                }
                embeddings.extend(batch_embeddings);
            }
            Ok(embeddings)
        })
        .await
        .map_err(|error| EmbedError::Join(error.to_string()))?
    }

    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>, EmbedError> {
        let embeddings = self.embed_batch(vec![query.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::Model("embedding query returned no vector".to_string()))
    }
}

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("failed to resolve the local data directory for the embedding cache")]
    MissingLocalAppData,
    #[error("embedding IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("embedding model error: {0}")]
    Model(String),
    #[error("embedding task join error: {0}")]
    Join(String),
    #[error("embedding dimension mismatch: expected {expected}, got {actual}")]
    Dimension { expected: usize, actual: usize },
}

pub fn default_cache_dir() -> Result<PathBuf, EmbedError> {
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local_app_data)
            .join("PDF-Parser")
            .join("engines")
            .join("embeddings"));
    }
    dirs::data_local_dir()
        .map(|path| path.join("PDF-Parser").join("engines").join("embeddings"))
        .ok_or(EmbedError::MissingLocalAppData)
}

pub fn normalize_l2(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for value in values {
            *value /= norm;
        }
    }
}

pub fn vector_to_json(values: &[f32]) -> String {
    let mut output = String::with_capacity(values.len() * 10 + 2);
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&format!("{value:.8}"));
    }
    output.push(']');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_handles_empty_vectors() {
        let mut values = Vec::<f32>::new();
        normalize_l2(&mut values);
        assert!(values.is_empty());
    }

    #[test]
    fn vector_to_json_serializes_floats() {
        assert_eq!(vector_to_json(&[0.5, -1.0]), "[0.50000000,-1.00000000]");
    }

    #[tokio::test]
    #[ignore]
    async fn embeds_one_hundred_chunks_at_target_throughput() {
        let service = EmbeddingService::new(default_cache_dir().unwrap())
            .await
            .unwrap();
        let texts = (0..100)
            .map(|index| {
                format!(
                    "This is a benchmark document chunk number {index}. It contains stable text."
                )
            })
            .collect::<Vec<_>>();
        let started = Instant::now();
        let embeddings = service.embed_batch(texts).await.unwrap();
        assert_eq!(embeddings.len(), 100);
        let pages_per_minute = 100.0 / started.elapsed().as_secs_f64() * 60.0;
        assert!(
            pages_per_minute >= 300.0,
            "throughput was {pages_per_minute:.1} chunks/min"
        );
    }
}
