use rusqlite::Connection;
use serde::Serialize;
use std::{env, fs, path::Path, path::PathBuf, sync::OnceLock};
use thiserror::Error;

static SQLITE_VEC_REGISTRATION: OnceLock<Result<(), i32>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct AppDatabase {
    pub path: String,
    pub url: String,
    pub sqlite_vec_loaded: bool,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to determine the application data directory")]
    MissingAppData,
    #[error("database IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub fn app_data_dir() -> Result<PathBuf, DbError> {
    if let Some(local_appdata) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local_appdata).join("PDF-Parser"));
    }

    if let Some(appdata) = env::var_os("APPDATA") {
        return Ok(PathBuf::from(appdata).join("PDF-Parser"));
    }

    dirs::data_local_dir()
        .or_else(dirs::config_dir)
        .map(|path| path.join("PDF-Parser"))
        .ok_or(DbError::MissingAppData)
}

pub fn database_path() -> Result<PathBuf, DbError> {
    Ok(app_data_dir()?.join("db").join("app.db"))
}

pub fn log_dir() -> Result<PathBuf, DbError> {
    Ok(app_data_dir()?.join("Logs"))
}

pub fn database_url() -> Result<String, DbError> {
    Ok(database_url_from_path(&database_path()?))
}

pub fn open_connection() -> Result<Connection, DbError> {
    open_connection_at(&database_path()?)
}

pub fn open_connection_at(path: &Path) -> Result<Connection, DbError> {
    let _ = register_sqlite_vec();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    apply_pragmas(&connection)?;
    Ok(connection)
}

pub fn register_sqlite_vec_auto_extension() {
    let _ = register_sqlite_vec();
}

pub fn default_output_dir() -> Result<PathBuf, DbError> {
    let base = dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Documents")))
        .unwrap_or(app_data_dir()?);
    Ok(base.join("PDF-Parser").join("Processed"))
}

pub fn prepare_database() -> Result<AppDatabase, DbError> {
    let path = database_path()?;
    let vec_registered = register_sqlite_vec();
    let connection = open_connection_at(&path)?;

    let sqlite_vec_loaded = match vec_registered {
        Ok(()) => {
            match connection.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0)) {
                Ok(version) => {
                    tracing::info!(version, "sqlite-vec loaded");
                    true
                }
                Err(error) => {
                    tracing::warn!(error = %error, "sqlite-vec verification failed");
                    false
                }
            }
        }
        Err(code) => {
            tracing::warn!(code, "sqlite-vec registration failed");
            false
        }
    };

    Ok(AppDatabase {
        path: path.to_string_lossy().into_owned(),
        url: database_url_from_path(&path),
        sqlite_vec_loaded,
    })
}

fn apply_pragmas(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-65536;
         PRAGMA temp_store=MEMORY;
         PRAGMA busy_timeout=5000;
         PRAGMA journal_size_limit=67108864;
         PRAGMA foreign_keys=ON;",
    )
}

fn database_url_from_path(path: &Path) -> String {
    format!("sqlite:{}", path.to_string_lossy())
}

fn register_sqlite_vec() -> Result<(), i32> {
    type SqliteExtensionInit = unsafe extern "C" fn(
        *mut rusqlite::ffi::sqlite3,
        *mut *const std::ffi::c_char,
        *const rusqlite::ffi::sqlite3_api_routines,
    ) -> i32;

    *SQLITE_VEC_REGISTRATION.get_or_init(|| {
        let result = unsafe {
            let init = std::mem::transmute::<*const (), SqliteExtensionInit>(
                sqlite_vec::sqlite3_vec_init as *const (),
            );
            rusqlite::ffi::sqlite3_auto_extension(Some(init))
        };

        if result == rusqlite::ffi::SQLITE_OK {
            Ok(())
        } else {
            Err(result)
        }
    })
}
