use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Emitter, State};

use crate::{
    ai::{self, ollama::OllamaProvider, sanitize, NamingProposal, DEFAULT_OLLAMA_BASE_URL},
    db,
    events::AppEventPayload,
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

#[tauri::command]
pub async fn ai_propose_names(
    app: AppHandle,
    state: State<'_, AppState>,
    document_ids: Vec<i64>,
) -> Result<Vec<NamingProposal>, String> {
    let provider =
        ai::configured_provider(&state.db_path, None).map_err(|error| error.to_string())?;
    let mut proposals = Vec::with_capacity(document_ids.len());
    for document_id in document_ids {
        let input =
            load_naming_input(&state.db_path, document_id).map_err(|error| error.to_string())?;
        let proposal = provider
            .propose_name(&input.text_sample, &input.original_filename)
            .await
            .map_err(|error| error.to_string())?;
        upsert_pending_rename(&state.db_path, document_id, &proposal)
            .map_err(|error| error.to_string())?;
        let _ = app.emit(
            "document:naming_ready",
            AppEventPayload::DocumentNamingReady {
                document_id,
                proposed_name: proposal.display_name.clone(),
            },
        );
        proposals.push(proposal);
    }
    Ok(proposals)
}

#[tauri::command]
pub fn ai_apply_rename(
    document_id: i64,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    apply_rename_at(&state.db_path, document_id, &new_name).map_err(|error| error.to_string())
}

pub fn queue_document_naming(app: AppHandle, state: AppState, document_id: i64) {
    tokio::spawn(async move {
        let result = async {
            let provider = ai::configured_provider(&state.db_path, None)?;
            let input = load_naming_input(&state.db_path, document_id)
                .map_err(|error| ai::AiError::Unavailable(error.to_string()))?;
            let proposal = provider
                .propose_name(&input.text_sample, &input.original_filename)
                .await?;
            upsert_pending_rename(&state.db_path, document_id, &proposal)
                .map_err(|error| ai::AiError::Unavailable(error.to_string()))?;
            Ok::<NamingProposal, ai::AiError>(proposal)
        }
        .await;

        match result {
            Ok(proposal) => {
                let _ = app.emit(
                    "document:naming_ready",
                    AppEventPayload::DocumentNamingReady {
                        document_id,
                        proposed_name: proposal.display_name,
                    },
                );
            }
            Err(_) => {
                let _ = mark_naming_done_without_proposal(&state.db_path, document_id);
                let _ = app.emit(
                    "document:updated",
                    AppEventPayload::DocumentUpdated {
                        document_id,
                        status: "done".to_string(),
                    },
                );
            }
        }
    });
}

pub fn should_queue_naming(db_path: &Path) -> bool {
    ai::ai_naming_enabled(db_path).unwrap_or(false)
        && ai::active_provider_name(db_path).ok().flatten().is_some()
        && ai::configured_provider(db_path, None).is_ok()
}

pub fn apply_rename_at(db_path: &Path, document_id: i64, new_name: &str) -> anyhow::Result<()> {
    let connection = db::open_connection_at(db_path)?;
    let (output_path, original_path): (String, String) = connection.query_row(
        "SELECT output_path, original_path FROM documents WHERE id = ?1",
        params![document_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let source = PathBuf::from(output_path);
    let output_dir = source
        .parent()
        .ok_or_else(|| anyhow::anyhow!("document output path has no parent"))?;
    let desired_display = sanitize::sanitize_display_name(new_name)?;
    let target =
        if source.file_name().and_then(|value| value.to_str()) == Some(desired_display.as_str()) {
            source.clone()
        } else {
            sanitize::sanitize_filename(&desired_display, output_dir)?
        };

    if source != target {
        fs::rename(&source, &target)?;
    }
    let display_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(desired_display.as_str())
        .to_string();
    let pending = connection
        .query_row(
            "SELECT summary, provider, proposed_name FROM pending_renames WHERE document_id = ?1",
            params![document_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let (summary, provider, proposed_name) =
        pending.unwrap_or((None, "none".to_string(), display_name.clone()));
    let user_edit = if proposed_name.eq_ignore_ascii_case(&display_name) {
        None
    } else {
        Some(display_name.clone())
    };
    connection.execute(
        "UPDATE documents
         SET display_name = ?2,
             output_path = ?3,
             ai_summary = COALESCE(?4, ai_summary),
             ai_provider = ?5,
             status = 'done',
             updated_at = ?6
         WHERE id = ?1",
        params![
            document_id,
            display_name,
            target.to_string_lossy().into_owned(),
            summary.clone(),
            provider.clone(),
            now_ts(),
        ],
    )?;
    connection.execute(
        "INSERT INTO pending_renames(document_id, proposed_name, summary, provider, created_at, reviewed, user_edit)
         VALUES(?1, ?2, ?3, ?4, ?5, 1, ?6)
         ON CONFLICT(document_id) DO UPDATE SET
             reviewed = 1,
             user_edit = excluded.user_edit",
        params![document_id, proposed_name, summary, provider, now_ts(), user_edit],
    )?;
    let _ = original_path;
    Ok(())
}

pub fn load_naming_input(db_path: &Path, document_id: i64) -> rusqlite::Result<NamingInput> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let original_path: String = connection.query_row(
        "SELECT original_path FROM documents WHERE id = ?1",
        params![document_id],
        |row| row.get(0),
    )?;
    let original_filename = Path::new(&original_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.pdf")
        .to_string();

    let mut statement = connection
        .prepare("SELECT text FROM pages WHERE document_id = ?1 ORDER BY page_number ASC")?;
    let mut rows = statement.query(params![document_id])?;
    let mut sample = String::new();
    while let Some(row) = rows.next()? {
        let page_text: String = row.get(0)?;
        if !sample.is_empty() {
            sample.push_str("\n\n");
        }
        sample.push_str(&page_text);
        if sample.chars().count() >= 4_000 {
            sample = ai::truncate_for_prompt(&sample, 4_000);
            break;
        }
    }
    Ok(NamingInput {
        original_filename,
        text_sample: sample,
    })
}

fn upsert_pending_rename(
    db_path: &Path,
    document_id: i64,
    proposal: &NamingProposal,
) -> rusqlite::Result<()> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    connection.execute(
        "INSERT INTO pending_renames(document_id, proposed_name, summary, provider, created_at, reviewed)
         VALUES(?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT(document_id) DO UPDATE SET
             proposed_name = excluded.proposed_name,
             summary = excluded.summary,
             provider = excluded.provider,
             created_at = excluded.created_at,
             reviewed = 0,
             user_edit = NULL",
        params![
            document_id,
            proposal.display_name,
            proposal.summary,
            proposal.provider,
            now_ts(),
        ],
    )?;
    connection.execute(
        "UPDATE documents
         SET ai_summary = ?2,
             ai_provider = ?3,
             updated_at = ?4
         WHERE id = ?1",
        params![document_id, proposal.summary, proposal.provider, now_ts()],
    )?;
    Ok(())
}

fn mark_naming_done_without_proposal(db_path: &Path, document_id: i64) -> rusqlite::Result<()> {
    let connection = db::open_connection_at(db_path).map_err(db_error_to_rusqlite)?;
    let original_path: String = connection.query_row(
        "SELECT original_path FROM documents WHERE id = ?1",
        params![document_id],
        |row| row.get(0),
    )?;
    let display_name = ai::fallback_display_name(
        Path::new(&original_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("document.pdf"),
    );
    connection.execute(
        "UPDATE documents
         SET status = 'done',
             display_name = COALESCE(display_name, ?2),
             ai_provider = COALESCE(ai_provider, 'none'),
             updated_at = ?3
         WHERE id = ?1",
        params![document_id, display_name, now_ts()],
    )?;
    Ok(())
}

pub struct NamingInput {
    pub original_filename: String,
    pub text_sample: String,
}

fn db_error_to_rusqlite(error: db::DbError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
