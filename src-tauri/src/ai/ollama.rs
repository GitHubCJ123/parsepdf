use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    build_user_prompt, proposal_from_model_response, AiError, AiProvider,
    ChatMessage as AiChatMessage, ChatResponse as AiChatResponse, NamingProposal,
    DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL, NAMING_SYSTEM_PROMPT,
};

const TIMEOUT_SECONDS: u32 = 60;
const STREAM_TIMEOUT_SECONDS: u32 = 180;

#[derive(Clone)]
pub struct OllamaProvider {
    base_url: String,
    model: Option<String>,
    client: reqwest::Client,
    stream_client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: String, model: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS as u64))
            .build()
            .expect("Ollama client should build");
        let stream_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(STREAM_TIMEOUT_SECONDS as u64))
            .build()
            .expect("Ollama stream client should build");
        Self {
            base_url: normalize_base_url(&base_url),
            model,
            client,
            stream_client,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, AiError> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(map_transport_error)?;
        if !response.status().is_success() {
            return Err(AiError::Unavailable(format!(
                "Ollama returned {}",
                response.status()
            )));
        }
        let tags = response
            .json::<TagsResponse>()
            .await
            .map_err(map_transport_error)?;
        Ok(tags.models.into_iter().map(|model| model.name).collect())
    }

    async fn resolve_model(&self) -> Result<String, AiError> {
        let models = self.list_models().await?;
        if models.is_empty() {
            return Err(AiError::Unavailable(
                "Ollama has no installed models".to_string(),
            ));
        }
        if let Some(model) = self.model.as_ref().filter(|value| !value.trim().is_empty()) {
            if models.iter().any(|installed| installed == model) {
                return Ok(model.clone());
            }
            return Err(AiError::ModelNotFound(model.clone()));
        }
        if models
            .iter()
            .any(|installed| installed == DEFAULT_OLLAMA_MODEL)
        {
            Ok(DEFAULT_OLLAMA_MODEL.to_string())
        } else {
            Ok(models[0].clone())
        }
    }
}

#[async_trait::async_trait]
impl AiProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    async fn propose_name(
        &self,
        text_sample: &str,
        original_filename: &str,
    ) -> Result<NamingProposal, AiError> {
        let model = self.resolve_model().await?;
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&json!({
                "model": model,
                "stream": false,
                "messages": [
                    { "role": "system", "content": NAMING_SYSTEM_PROMPT },
                    { "role": "user", "content": build_user_prompt(original_filename, text_sample) }
                ]
            }))
            .send()
            .await
            .map_err(map_transport_error)?;

        match response.status() {
            StatusCode::OK => {
                let chat = response
                    .json::<OllamaChatResponse>()
                    .await
                    .map_err(map_transport_error)?;
                let content = chat.message.content.ok_or_else(|| {
                    AiError::InvalidResponse("missing assistant content".to_string())
                })?;
                Ok(proposal_from_model_response(
                    &content,
                    original_filename,
                    format!("ollama:{model}"),
                    model,
                    None,
                ))
            }
            StatusCode::NOT_FOUND => Err(AiError::ModelNotFound(model)),
            status => Err(AiError::Unavailable(format!("Ollama returned {status}"))),
        }
    }

    async fn stream_chat(
        &self,
        messages: Vec<AiChatMessage>,
        token_callback: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<AiChatResponse, AiError> {
        let model = self.resolve_model().await?;
        let response = self
            .stream_client
            .post(format!("{}/api/chat", self.base_url))
            .json(&json!({
                "model": model,
                "stream": true,
                "messages": messages,
            }))
            .send()
            .await
            .map_err(map_stream_transport_error)?;

        match response.status() {
            StatusCode::OK => stream_ollama_response(response, &model, token_callback).await,
            StatusCode::NOT_FOUND => Err(AiError::ModelNotFound(model)),
            status => Err(AiError::Unavailable(format!("Ollama returned {status}"))),
        }
    }

    async fn health_check(&self) -> Result<(), AiError> {
        self.resolve_model().await.map(|_| ())
    }
}

async fn stream_ollama_response(
    response: reqwest::Response,
    model: &str,
    token_callback: Box<dyn Fn(String) + Send + Sync>,
) -> Result<AiChatResponse, AiError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut tokens_in = None;
    let mut tokens_out = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_stream_transport_error)?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim_end_matches('\r').trim().to_string();
            buffer.drain(..=newline);
            if line.is_empty() {
                continue;
            }
            let chunk = serde_json::from_str::<OllamaStreamChunk>(&line)
                .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
            if let Some(message) = chunk.message {
                if let Some(delta) = message.content.filter(|value| !value.is_empty()) {
                    token_callback(delta.clone());
                    content.push_str(&delta);
                }
            }
            if chunk.done.unwrap_or(false) {
                tokens_in = chunk.prompt_eval_count;
                tokens_out = chunk.eval_count;
                return Ok(AiChatResponse {
                    content,
                    provider: format!("ollama:{model}"),
                    model: model.to_string(),
                    tokens_in,
                    tokens_out,
                });
            }
        }
    }

    if !buffer.trim().is_empty() {
        let chunk = serde_json::from_str::<OllamaStreamChunk>(buffer.trim())
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        if let Some(message) = chunk.message {
            if let Some(delta) = message.content.filter(|value| !value.is_empty()) {
                token_callback(delta.clone());
                content.push_str(&delta);
            }
        }
        tokens_in = chunk.prompt_eval_count;
        tokens_out = chunk.eval_count;
    }

    Ok(AiChatResponse {
        content,
        provider: format!("ollama:{model}"),
        model: model.to_string(),
        tokens_in,
        tokens_out,
    })
}

fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        DEFAULT_OLLAMA_BASE_URL.to_string()
    } else {
        trimmed.to_string()
    }
}

fn map_transport_error(error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(TIMEOUT_SECONDS)
    } else if error.is_connect() {
        AiError::NotConfigured
    } else {
        AiError::Transport(error)
    }
}

fn map_stream_transport_error(error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(STREAM_TIMEOUT_SECONDS)
    } else if error.is_connect() {
        AiError::NotConfigured
    } else {
        AiError::Transport(error)
    }
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<ModelTag>,
}

#[derive(Debug, Deserialize)]
struct ModelTag {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    message: Option<OllamaMessage>,
    done: Option<bool>,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct OllamaModelInfo {
    pub name: String,
}
