use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{header, StatusCode};
use serde::Deserialize;
use serde_json::json;

use super::{
    build_user_prompt, proposal_from_model_response, AiError, AiProvider,
    ChatMessage as AiChatMessage, ChatResponse as AiChatResponse, NamingProposal,
    DEFAULT_OPENROUTER_MODEL, NAMING_SYSTEM_PROMPT,
};

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const TIMEOUT_SECONDS: u32 = 30;
const STREAM_TIMEOUT_SECONDS: u32 = 120;
const MAX_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct OpenRouterProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
    stream_client: reqwest::Client,
}

impl OpenRouterProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS as u64))
            .build()
            .expect("OpenRouter client should build");
        let stream_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(STREAM_TIMEOUT_SECONDS as u64))
            .build()
            .expect("OpenRouter stream client should build");
        Self {
            api_key,
            model: if model.trim().is_empty() {
                DEFAULT_OPENROUTER_MODEL.to_string()
            } else {
                model
            },
            client,
            stream_client,
        }
    }

    async fn send_chat(
        &self,
        text_sample: &str,
        original_filename: &str,
    ) -> Result<OpenRouterChatResponse, AiError> {
        let body = json!({
            "model": self.model,
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": NAMING_SYSTEM_PROMPT },
                { "role": "user", "content": build_user_prompt(original_filename, text_sample) }
            ]
        });

        let mut last_error: Option<AiError> = None;
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .client
                .post(OPENROUTER_URL)
                .header(header::AUTHORIZATION, format!("Bearer {}", self.api_key))
                .header("HTTP-Referer", "app://pdf-parser")
                .header("X-Title", "PDF-Parser")
                .json(&body)
                .send()
                .await
                .map_err(map_transport_error)?;

            match response.status() {
                StatusCode::OK => {
                    return response
                        .json::<OpenRouterChatResponse>()
                        .await
                        .map_err(map_transport_error);
                }
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    return Err(AiError::InvalidKey)
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after =
                        retry_after_seconds(&response).unwrap_or(2_u32.pow(attempt as u32));
                    last_error = Some(AiError::RateLimited(retry_after));
                    if attempt + 1 < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_secs(retry_after.min(10) as u64)).await;
                    }
                }
                StatusCode::NOT_FOUND => return Err(AiError::ModelNotFound(self.model.clone())),
                status if status.is_server_error() => {
                    let message = format!("OpenRouter returned {status}");
                    last_error = Some(AiError::Unavailable(message));
                    if attempt + 1 < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt as u32))).await;
                    }
                }
                status => {
                    return Err(AiError::Unavailable(format!(
                        "OpenRouter returned {status}"
                    )))
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| AiError::Unavailable("OpenRouter request failed".to_string())))
    }
}

#[async_trait::async_trait]
impl AiProvider for OpenRouterProvider {
    fn id(&self) -> &'static str {
        "openrouter"
    }

    async fn propose_name(
        &self,
        text_sample: &str,
        original_filename: &str,
    ) -> Result<NamingProposal, AiError> {
        let response = self.send_chat(text_sample, original_filename).await?;
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .ok_or_else(|| AiError::InvalidResponse("missing assistant content".to_string()))?;
        Ok(proposal_from_model_response(
            content,
            original_filename,
            format!("openrouter:{}", self.model),
            self.model.clone(),
            response.usage.and_then(|usage| {
                usage
                    .total_tokens
                    .and_then(|value| u32::try_from(value).ok())
            }),
        ))
    }

    async fn stream_chat(
        &self,
        messages: Vec<AiChatMessage>,
        // OpenRouter has no unified "thinking" toggle; reasoning models stream it
        // inline. Accepted for trait compatibility and ignored.
        _think: Option<bool>,
        token_callback: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<AiChatResponse, AiError> {
        let body = json!({
            "model": self.model,
            "temperature": 0.2,
            "stream": true,
            "stream_options": { "include_usage": true },
            "messages": messages,
        });
        let response = self
            .stream_client
            .post(OPENROUTER_URL)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "app://pdf-parser")
            .header("X-Title", "PDF-Parser")
            .json(&body)
            .send()
            .await
            .map_err(map_stream_transport_error)?;

        match response.status() {
            StatusCode::OK => {
                stream_openrouter_response(response, &self.model, token_callback).await
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AiError::InvalidKey),
            StatusCode::TOO_MANY_REQUESTS => Err(AiError::RateLimited(
                retry_after_seconds(&response).unwrap_or(60),
            )),
            StatusCode::NOT_FOUND => Err(AiError::ModelNotFound(self.model.clone())),
            status => Err(AiError::Unavailable(format!(
                "OpenRouter returned {status}"
            ))),
        }
    }

    async fn health_check(&self) -> Result<(), AiError> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": "Reply with JSON only." },
                { "role": "user", "content": "{\"ping\":true}" }
            ],
            "max_tokens": 8,
            "temperature": 0,
            "response_format": { "type": "json_object" }
        });
        let response = self
            .client
            .post(OPENROUTER_URL)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "app://pdf-parser")
            .header("X-Title", "PDF-Parser")
            .json(&body)
            .send()
            .await
            .map_err(map_transport_error)?;

        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AiError::InvalidKey),
            StatusCode::TOO_MANY_REQUESTS => Err(AiError::RateLimited(
                retry_after_seconds(&response).unwrap_or(60),
            )),
            StatusCode::NOT_FOUND => Err(AiError::ModelNotFound(self.model.clone())),
            status if status.is_server_error() => Err(AiError::Unavailable(format!(
                "OpenRouter returned {status}"
            ))),
            status => Err(AiError::Unavailable(format!(
                "OpenRouter returned {status}"
            ))),
        }
    }
}

async fn stream_openrouter_response(
    response: reqwest::Response,
    model: &str,
    token_callback: Box<dyn Fn(String) + Send + Sync>,
) -> Result<AiChatResponse, AiError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut usage: Option<Usage> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_stream_transport_error)?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim_end_matches('\r').trim().to_string();
            buffer.drain(..=newline);
            if !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                return Ok(chat_response(model, content, usage));
            }
            if data.is_empty() {
                continue;
            }
            let chunk = serde_json::from_str::<OpenRouterStreamChunk>(data)
                .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
            if let Some(next_usage) = chunk.usage {
                usage = Some(next_usage);
            }
            for choice in chunk.choices {
                if let Some(delta) = choice.delta.content.filter(|value| !value.is_empty()) {
                    token_callback(delta.clone());
                    content.push_str(&delta);
                }
            }
        }
    }

    if !buffer.trim().is_empty() {
        let line = buffer.trim();
        if let Some(data) = line.strip_prefix("data:").map(str::trim) {
            if data != "[DONE]" && !data.is_empty() {
                let chunk = serde_json::from_str::<OpenRouterStreamChunk>(data)
                    .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
                if let Some(next_usage) = chunk.usage {
                    usage = Some(next_usage);
                }
                for choice in chunk.choices {
                    if let Some(delta) = choice.delta.content.filter(|value| !value.is_empty()) {
                        token_callback(delta.clone());
                        content.push_str(&delta);
                    }
                }
            }
        }
    }

    Ok(chat_response(model, content, usage))
}

fn chat_response(model: &str, content: String, usage: Option<Usage>) -> AiChatResponse {
    AiChatResponse {
        content,
        provider: format!("openrouter:{model}"),
        model: model.to_string(),
        tokens_in: usage
            .as_ref()
            .and_then(|usage| usage.prompt_tokens)
            .and_then(|value| u32::try_from(value).ok()),
        tokens_out: usage
            .as_ref()
            .and_then(|usage| usage.completion_tokens)
            .and_then(|value| u32::try_from(value).ok()),
        thinking: None,
    }
}

fn map_transport_error(error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(TIMEOUT_SECONDS)
    } else {
        AiError::Transport(error)
    }
}

fn map_stream_transport_error(error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(STREAM_TIMEOUT_SECONDS)
    } else {
        AiError::Transport(error)
    }
}

fn retry_after_seconds(response: &reqwest::Response) -> Option<u32> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok())
}

#[derive(Debug, Deserialize)]
struct OpenRouterChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChunk {
    choices: Vec<StreamChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}
