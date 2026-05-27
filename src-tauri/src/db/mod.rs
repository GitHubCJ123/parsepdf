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
    #[error("failed to determine the Windows APPDATA directory")]
    MissingAppData,
    #[error("database IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub fn app_data_dir() -> Result<PathBuf, DbError> {
    if let Some(appdata) = env::var_os("APPDATA") {
        return Ok(PathBuf::from(appdata).join("PDF-Parser"));
    }

    dirs::config_dir()
        .map(|path| path.join("PDF-Parser"))
        .ok_or(DbError::MissingAppData)
}

pub fn database_path() -> Result<PathBuf, DbError> {
    Ok(app_data_dir()?.join("db").join("app.db"))
}

pub fn database_url() -> Result<String, DbError> {
    Ok(database_url_from_path(&database_path()?))
}

pub fn prepare_database() -> Result<AppDatabase, DbError> {
    let path = database_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let vec_registered = register_sqlite_vec();
    let connection = Connection::open(&path)?;
    apply_pragmas(&connection)?;

    let sqlite_vec_loaded = match vec_registered {
        Ok(()) => match connection.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0)) {
            Ok(version) => {
                eprintln!("[db] sqlite-vec loaded ({version})");
                true
            }
            Err(error) => {
                eprintln!("[db] sqlite-vec verification failed: {error}");
                false
            }
        },
        Err(code) => {
            eprintln!("[db] sqlite-vec registration failed with code {code}");
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
         PRAGMA busy_timeout=5000;
         PRAGMA journal_size_limit=67108864;
         PRAGMA foreign_keys=ON;",
    )
}

fn database_url_from_path(path: &Path) -> String {
    format!("sqlite:{}", path.to_string_lossy())
}

fn register_sqlite_vec() -> Result<(), i32> {
    *SQLITE_VEC_REGISTRATION.get_or_init(|| {
        let result = unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )))
        };

        if result == rusqlite::ffi::SQLITE_OK {
            Ok(())
        } else {
            Err(result)
        }
    })
}
