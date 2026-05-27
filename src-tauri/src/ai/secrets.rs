use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use argon2::{Algorithm, Argon2, Params, Version};
use rand::{rngs::OsRng, RngCore};
use tauri_plugin_stronghold::stronghold::Stronghold;
use thiserror::Error;

use crate::db;

const STRONGHOLD_FOLDER: &str = "stronghold";
const SALT_FILE: &str = "salt.bin";
const DPAPI_FILE: &str = "machine_secret.dpapi";
const VAULT_FILE: &str = "vault.snapshot";
const CLIENT_ID: &[u8] = b"pdf-parser-secrets";
const KEY_BYTES: usize = 32;

static STRONGHOLDS: OnceLock<Mutex<HashMap<PathBuf, Stronghold>>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keychain unavailable")]
    KeychainUnavailable,
    #[error("secret key is not allowed")]
    InvalidKey,
    #[error("secret value is not valid utf-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database directory error: {0}")]
    Db(#[from] db::DbError),
    #[error("Stronghold error: {0}")]
    Stronghold(#[from] tauri_plugin_stronghold::stronghold::Error),
    #[error("Stronghold client error: {0}")]
    StrongholdClient(#[from] iota_stronghold::ClientError),
    #[error("Stronghold memory error: {0}")]
    StrongholdMemory(#[from] iota_stronghold::MemoryError),
    #[error("KDF error")]
    Kdf,
}

impl SecretError {
    fn public_message(&self) -> String {
        match self {
            SecretError::KeychainUnavailable => "keychain unavailable".to_string(),
            _ => self.to_string(),
        }
    }
}

#[tauri::command]
pub fn secrets_set(key: String, value: String) -> Result<(), String> {
    set_secret(&key, &value).map_err(|error| error.public_message())
}

#[tauri::command]
pub fn secrets_get(key: String) -> Result<Option<String>, String> {
    get_secret(&key).map_err(|error| error.public_message())
}

#[tauri::command]
pub fn secrets_delete(key: String) -> Result<(), String> {
    delete_secret(&key).map_err(|error| error.public_message())
}

pub fn set_secret(key: &str, value: &str) -> Result<(), SecretError> {
    validate_key(key)?;
    with_store(None, |stronghold| {
        {
            let client = load_or_create_client(stronghold)?;
            client
                .store()
                .insert(key.as_bytes().to_vec(), value.as_bytes().to_vec(), None)?;
        }
        stronghold.save()?;
        Ok(())
    })
}

pub fn get_secret(key: &str) -> Result<Option<String>, SecretError> {
    validate_key(key)?;
    with_store(None, |stronghold| {
        let client = load_or_create_client(stronghold)?;
        let value = client.store().get(key.as_bytes())?;
        value.map(String::from_utf8).transpose().map_err(Into::into)
    })
}

pub fn delete_secret(key: &str) -> Result<(), SecretError> {
    validate_key(key)?;
    with_store(None, |stronghold| {
        {
            let client = load_or_create_client(stronghold)?;
            let _ = client.store().delete(key.as_bytes())?;
        }
        stronghold.save()?;
        Ok(())
    })
}

fn with_store<T>(
    base_dir: Option<&Path>,
    f: impl FnOnce(&Stronghold) -> Result<T, SecretError>,
) -> Result<T, SecretError> {
    let paths = StrongholdPaths::resolve(base_dir)?;
    fs::create_dir_all(&paths.dir)?;
    let mut stores = STRONGHOLDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| SecretError::KeychainUnavailable)?;
    if !stores.contains_key(&paths.vault) {
        let salt = load_or_create_random(&paths.salt, KEY_BYTES)?;
        let machine_secret = load_or_create_machine_secret(&paths.dpapi_blob)?;
        let password = derive_password(&salt, &machine_secret)?;
        let stronghold = Stronghold::new(&paths.vault, password)
            .map_err(|_| SecretError::KeychainUnavailable)?;
        stores.insert(paths.vault.clone(), stronghold);
    }
    let stronghold = stores
        .get(&paths.vault)
        .ok_or(SecretError::KeychainUnavailable)?;
    f(stronghold)
}

fn load_or_create_client(stronghold: &Stronghold) -> Result<iota_stronghold::Client, SecretError> {
    if let Ok(client) = stronghold.get_client(CLIENT_ID) {
        return Ok(client);
    }
    match stronghold.load_client(CLIENT_ID) {
        Ok(client) => Ok(client),
        Err(_) => stronghold.create_client(CLIENT_ID).map_err(Into::into),
    }
}

fn validate_key(key: &str) -> Result<(), SecretError> {
    let allowed =
        matches!(key, "openrouter.api_key" | "ollama.base_url") || key.starts_with("test.");
    if allowed {
        Ok(())
    } else {
        Err(SecretError::InvalidKey)
    }
}

fn derive_password(salt: &[u8], machine_secret: &[u8]) -> Result<Vec<u8>, SecretError> {
    let params = Params::new(19_456, 2, 1, Some(KEY_BYTES)).map_err(|_| SecretError::Kdf)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = vec![0_u8; KEY_BYTES];
    argon2
        .hash_password_into(machine_secret, salt, &mut output)
        .map_err(|_| SecretError::Kdf)?;
    Ok(output)
}

fn load_or_create_random(path: &Path, bytes: usize) -> Result<Vec<u8>, SecretError> {
    if path.exists() {
        return Ok(fs::read(path)?);
    }
    let mut value = vec![0_u8; bytes];
    OsRng.fill_bytes(&mut value);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &value)?;
    Ok(value)
}

fn load_or_create_machine_secret(path: &Path) -> Result<Vec<u8>, SecretError> {
    if path.exists() {
        return unprotect_data(&fs::read(path)?);
    }
    let mut secret = vec![0_u8; KEY_BYTES];
    OsRng.fill_bytes(&mut secret);
    let protected = protect_data(&secret)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, protected)?;
    Ok(secret)
}

struct StrongholdPaths {
    dir: PathBuf,
    salt: PathBuf,
    dpapi_blob: PathBuf,
    vault: PathBuf,
}

impl StrongholdPaths {
    fn resolve(base_dir: Option<&Path>) -> Result<Self, SecretError> {
        let dir = base_dir
            .map(Path::to_path_buf)
            .unwrap_or(db::app_data_dir()?.join(STRONGHOLD_FOLDER));
        Ok(Self {
            salt: dir.join(SALT_FILE),
            dpapi_blob: dir.join(DPAPI_FILE),
            vault: dir.join(VAULT_FILE),
            dir,
        })
    }
}

#[cfg(windows)]
fn protect_data(data: &[u8]) -> Result<Vec<u8>, SecretError> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(SecretError::KeychainUnavailable);
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(bytes)
}

#[cfg(windows)]
fn unprotect_data(data: &[u8]) -> Result<Vec<u8>, SecretError> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(SecretError::KeychainUnavailable);
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect_data(_data: &[u8]) -> Result<Vec<u8>, SecretError> {
    Err(SecretError::KeychainUnavailable)
}

#[cfg(not(windows))]
fn unprotect_data(_data: &[u8]) -> Result<Vec<u8>, SecretError> {
    Err(SecretError::KeychainUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{distributions::Alphanumeric, Rng};

    #[test]
    #[cfg(windows)]
    fn stronghold_secret_round_trip() {
        iota_stronghold::engine::snapshot::try_set_encrypt_work_factor(0).unwrap();
        let random = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect::<String>();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("phase2-secret-tests")
            .join(random);
        fs::create_dir_all(&dir).unwrap();

        with_store(Some(&dir), |stronghold| {
            {
                let client = load_or_create_client(stronghold)?;
                client.store().insert(
                    b"test.roundtrip".to_vec(),
                    b"secret-value".to_vec(),
                    None,
                )?;
            }
            stronghold.save()?;
            Ok(())
        })
        .unwrap();

        let value = with_store(Some(&dir), |stronghold| {
            let client = load_or_create_client(stronghold)?;
            client
                .store()
                .get(b"test.roundtrip")?
                .ok_or(SecretError::InvalidKey)
        })
        .unwrap();
        assert_eq!(String::from_utf8(value).unwrap(), "secret-value");

        with_store(Some(&dir), |stronghold| {
            {
                let client = load_or_create_client(stronghold)?;
                let _ = client.store().delete(b"test.roundtrip")?;
            }
            stronghold.save()?;
            Ok(())
        })
        .unwrap();

        let deleted = with_store(Some(&dir), |stronghold| {
            let client = load_or_create_client(stronghold)?;
            client.store().get(b"test.roundtrip").map_err(Into::into)
        })
        .unwrap();
        assert!(deleted.is_none());
    }
}
