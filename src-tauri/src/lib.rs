pub mod ai;
mod commands;
pub mod db;
pub mod events;
pub mod ocr;
pub mod state;

use tauri::Manager;
use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
    let database_url = db::database_url().expect("failed to resolve PDF-Parser database path");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
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
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ai::ai_apply_rename,
            commands::ai::ai_health_check,
            commands::ai::ai_list_models,
            commands::ai::ai_propose_names,
            commands::database::initialize_database,
            commands::engines::ocr_install_engine,
            commands::engines::ocr_list_engines,
            commands::engines::ocr_remove_engine,
            commands::engines::ocr_set_default,
            commands::library::library_delete,
            commands::library::library_get,
            commands::library::library_list,
            commands::library::library_open_external,
            commands::library::library_pending_renames,
            commands::library::library_skip_rename,
            commands::process::process_pdf,
            commands::updates::prepare_for_update,
            ai::secrets::secrets_delete,
            ai::secrets::secrets_get,
            ai::secrets::secrets_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running PDF-Parser");
}
