use tauri::State;

use crate::{
    ai::{self, ollama::OllamaProvider, DEFAULT_OLLAMA_BASE_URL},
    state::AppState,
};

#[tauri::command]
pub async fn ai_health_check(provider: String, state: State<'_, AppState>) -> Result<bool, String> {
    let provider = provider.to_lowercase();
    let configured = ai::configured_provider(&state.db_path, Some(&provider));
    let provider = match configured {
        Ok(provider) => provider,
        Err(ai::AiError::NotConfigured) => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    match provider.health_check().await {
        Ok(()) => Ok(true),
        Err(ai::AiError::NotConfigured) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn ai_list_models(
    provider: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    match provider.to_lowercase().as_str() {
        "openrouter" => Ok(vec![
            ai::DEFAULT_OPENROUTER_MODEL.to_string(),
            "anthropic/claude-3.5-haiku".to_string(),
            "google/gemini-flash-1.5".to_string(),
            "meta-llama/llama-3.1-8b-instruct".to_string(),
        ]),
        "ollama" => {
            let base_url = ai::secrets::get_secret("ollama.base_url")
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_string());
            let model =
                ai::setting(&state.db_path, "ollama.model").map_err(|error| error.to_string())?;
            OllamaProvider::new(base_url, model)
                .list_models()
                .await
                .map_err(|error| error.to_string())
        }
        _ => Ok(Vec::new()),
    }
}
