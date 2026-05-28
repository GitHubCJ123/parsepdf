pub mod ai;
mod commands;
pub mod db;
pub mod events;
pub mod jobs;
pub mod logging;
pub mod ocr;
pub mod rag;
pub mod search;
pub mod state;
pub mod watcher;

use tauri::Manager;
use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = db::log_dir()
        .ok()
        .and_then(|path| logging::install_tracing_subscriber(&path).ok());
    db::register_sqlite_vec_auto_extension();
    let database_url = db::database_url().expect("failed to resolve PDF-Parser database path");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations(
                    &database_url,
                    vec![
                        Migration {
                            version: 1,
                            description: "initial schema",
                            sql: include_str!("../migrations/001_initial.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 3,
                            description: "phase 2 ai naming and library",
                            sql: include_str!("../migrations/003_phase2.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 4,
                            description: "phase 3 full text search",
                            sql: include_str!("../migrations/004_phase3.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 5,
                            description: "phase 5 rag chat",
                            sql: include_str!("../migrations/005_phase5.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 6,
                            description: "phase 4 folder watcher and jobs",
                            sql: include_str!("../migrations/006_phase4.sql"),
                            kind: MigrationKind::Up,
                        },
                    ],
                )
                .build(),
        )
        .setup(|app| {
            let database = db::prepare_database()
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            let app_state = state::AppState::new(app.handle(), database.path.into()).map_err(
                |error| -> Box<dyn std::error::Error> {
                    Box::new(std::io::Error::other(error.to_string()))
                },
            )?;
            let job_manager = jobs::JobManager::new(app.handle().clone(), app_state.clone());
            let watcher_service = watcher::WatcherService::new(
                app.handle().clone(),
                &app_state.db_path,
                job_manager.ingest_sender(),
            )
            .map_err(|error| -> Box<dyn std::error::Error> {
                Box::new(std::io::Error::other(error.to_string()))
            })?;

            let startup_jobs = job_manager.clone();
            let startup_watcher = watcher_service.clone();
            let startup_db_path = app_state.db_path.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = startup_jobs.recover_jobs().await {
                    tracing::warn!(error = %error, "job recovery failed");
                }
                if let Err(error) = startup_watcher.startup(&startup_db_path).await {
                    tracing::warn!(error = %error, "watcher startup failed");
                }
            });

            app.manage(app_state);
            app.manage(job_manager);
            app.manage(watcher_service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_paths,
            commands::app::log_tail,
            commands::app::log_save_selection,
            commands::ai::ai_apply_rename,
            commands::ai::ai_health_check,
            commands::ai::ai_list_models,
            commands::ai::ai_propose_names,
            commands::database::initialize_database,
            commands::engines::ocr_install_engine,
            commands::engines::ocr_list_engines,
            commands::engines::ocr_remove_engine,
            commands::engines::ocr_set_default,
            commands::folders::watcher_add_folder,
            commands::folders::watcher_list_folders,
            commands::folders::watcher_remove_folder,
            commands::folders::watcher_scan_now,
            commands::folders::watcher_set_enabled,
            commands::jobs::jobs_cancel,
            commands::jobs::jobs_cancel_all,
            commands::jobs::jobs_clear_completed,
            commands::jobs::jobs_list,
            commands::jobs::jobs_pause_all,
            commands::jobs::jobs_resume_all,
            commands::jobs::jobs_retry,
            commands::library::library_delete,
            commands::library::library_get,
            commands::library::library_list,
            commands::library::library_open_external,
            commands::library::library_pending_renames,
            commands::library::library_skip_rename,
            commands::process::process_pdf,
            commands::debug::debug_dump_state,
            commands::debug::debug_reset_library,
            commands::chat::chat_get_thread,
            commands::chat::chat_list_threads,
            commands::chat::chat_send,
            commands::chat::chat_status,
            commands::search::search,
            commands::search::search_document,
            commands::backfill::search_rebuild_index,
            commands::updates::prepare_for_update,
            ai::secrets::secrets_delete,
            ai::secrets::secrets_get,
            ai::secrets::secrets_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running PDF-Parser");
}
