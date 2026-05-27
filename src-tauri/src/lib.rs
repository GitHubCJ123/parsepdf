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
                    vec![Migration {
                        version: 1,
                        description: "initial schema",
                        sql: include_str!("../migrations/001_initial.sql"),
                        kind: MigrationKind::Up,
                    }],
                )
                .build(),
        )
        .setup(|app| {
            let database = db::prepare_database()
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            let app_state = state::AppState::new(app.handle(), database.path.into()).map_err(
                |error| -> Box<dyn std::error::Error> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, error.to_string()))
                },
            )?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::database::initialize_database,
            commands::process::process_pdf,
            commands::updates::prepare_for_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running PDF-Parser");
}
