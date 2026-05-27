pub mod ai;
pub mod db;
pub mod events;
pub mod ocr;
mod commands;

use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let database_url = db::database_url().expect("failed to resolve PDF-Parser database path");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
        .setup(|_app| {
            db::prepare_database()
                .map(|_| ())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
        })
        .invoke_handler(tauri::generate_handler![commands::database::initialize_database])
        .run(tauri::generate_context!())
        .expect("error while running PDF-Parser");
}
