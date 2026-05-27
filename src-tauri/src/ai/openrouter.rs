use std::time::Duration;

use reqwest::{header, StatusCode};
use serde::Deserialize;
use serde_json::json;

use super::{
    build_user_prompt, proposal_from_model_response, AiError, AiProvider, NamingProposal,
    DEFAULT_OPENROUTER_MODEL, NAMING_SYSTEM_PROMPT,
};

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const TIMEOUT_SECONDS: u32 = 30;
const MAX_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct OpenRouterProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenRouterProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS as u64))
            .build()
            .expect("OpenRouter client should build");
        Self {
            api_key,
            model: if model.trim().is_empty() {
                DEFAULT_OPENROUTER_MODEL.to_string()
            } else {
                model
            },
            client,
        }
    }

    async fn send_chat(
        &self,
        text_sample: &str,
        original_filename: &str,
    ) -> Result<ChatResponse, AiError> {
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
                        .json::<ChatResponse>()
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

fn map_transport_error(error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(TIMEOUT_SECONDS)
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
struct ChatResponse {
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
    total_tokens: Option<u64>,
}
