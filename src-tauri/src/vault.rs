use crate::{
    error::{Error, Result},
    model::{Secret, VaultData},
};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

const FORMAT: u32 = 1;
const CONTEXT: &[u8] = b"AltDeck/vault/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WrappedKey {
    kind: String,
    #[serde(default)]
    salt: String,
    nonce: String,
    ciphertext: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct VaultFile {
    format: u32,
    keys: Vec<WrappedKey>,
    nonce: String,
    ciphertext: String,
}
pub struct Unlocked {
    pub data: VaultData,
    key: Zeroizing<[u8; 32]>,
}
pub struct Vault {
    path: PathBuf,
    inner: Mutex<Option<Unlocked>>,
    last_activity: Mutex<Instant>,
    write_lock: Mutex<()>,
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}
fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<(String, String)> {
    let nonce = random_bytes::<12>();
    let cipher = Aes256Gcm::new(key.into());
    let bytes = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Internal)?;
    Ok((B64.encode(nonce), B64.encode(bytes)))
}
fn decrypt(
    key: &[u8; 32],
    nonce: &str,
    ciphertext: &str,
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    let nonce = B64.decode(nonce).map_err(|_| Error::BadVault)?;
    let bytes = B64.decode(ciphertext).map_err(|_| Error::BadVault)?;
    if nonce.len() != 12 || bytes.len() < 16 || bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::BadVault);
    }
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(&nonce), Payload { msg: &bytes, aad })
        .map(Zeroizing::new)
        .map_err(|_| Error::BadVault)
}
fn password_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    if password.len() < 12 || password.len() > 256 {
        return Err(Error::Invalid(
            "Master passwords must contain 12 to 256 bytes.",
        ));
    }
    let params = Params::new(65_536, 3, 4, Some(32)).map_err(|_| Error::Internal)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::Internal)?;
    Ok(key)
}
fn wrap(kind: &str, credential: Option<&str>, key: &[u8; 32]) -> Result<WrappedKey> {
    let salt = random_bytes::<16>();
    // DPAPI protects the data-encryption key directly. Password wraps use Argon2id + AES-GCM.
    let (nonce, ciphertext) = match credential {
        Some(password) => {
            let wrapping_key = password_key(password, &salt)?;
            encrypt(&wrapping_key, key, CONTEXT)?
        }
        None => (
            String::new(),
            B64.encode(Zeroizing::new(crate::platform::protect(key)?)),
        ),
    };
    Ok(WrappedKey {
        kind: kind.into(),
        salt: B64.encode(salt),
        nonce,
        ciphertext,
    })
}
fn unwrap(wrapped: &WrappedKey, credential: Option<&str>) -> Result<Zeroizing<[u8; 32]>> {
    if wrapped.kind == "windows" {
        if !matches!(wrapped.ciphertext.len(), 1..=4096) {
            return Err(Error::BadVault);
        }
        let blob = B64
            .decode(&wrapped.ciphertext)
            .map_err(|_| Error::BadVault)?;
        return crate::platform::unprotect(&blob);
    }
    if !["password", "recovery"].contains(&wrapped.kind.as_str()) {
        return Err(Error::BadVault);
    }
    let salt = B64.decode(&wrapped.salt).map_err(|_| Error::BadVault)?;
    if salt.len() != 16 {
        return Err(Error::BadVault);
    }
    let key = password_key(credential.ok_or(Error::BadPassword)?, &salt)
        .map_err(|_| Error::BadPassword)?;
    let plain = decrypt(&key, &wrapped.nonce, &wrapped.ciphertext, CONTEXT)
        .map_err(|_| Error::BadPassword)?;
    plain
        .as_slice()
        .try_into()
        .map(|k: [u8; 32]| Zeroizing::new(k))
        .map_err(|_| Error::BadVault)
}

impl Vault {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            inner: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
            write_lock: Mutex::new(()),
        }
    }
    fn read_file(&self) -> Result<VaultFile> {
        let bytes = fs::read(&self.path)?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(Error::BadVault);
        }
        let file: VaultFile = serde_json::from_slice(&bytes)?;
        if file.format != FORMAT || file.keys.is_empty() || file.keys.len() > 3 {
            return Err(Error::BadVault);
        }
        Ok(file)
    }
    async fn write(&self, state: &Unlocked, keys: &[WrappedKey]) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let plain = Zeroizing::new(serde_json::to_vec(&state.data)?);
        let (nonce, ciphertext) = encrypt(&state.key, &plain, CONTEXT)?;
        let file = VaultFile {
            format: FORMAT,
            keys: keys.to_vec(),
            nonce,
            ciphertext,
        };
        atomic_write(&self.path, &serde_json::to_vec_pretty(&file)?)
    }
    pub async fn initialize(&self, password: Option<&str>) -> Result<()> {
        let mut state = self.inner.lock().await;
        if state.is_some() {
            return Err(Error::AlreadyUnlocked);
        }
        if self.path.exists() {
            let file = self.read_file()?;
            let wrapped = if password.is_some() {
                file.keys
                    .iter()
                    .find(|k| k.kind == "password")
                    .ok_or(Error::BadPassword)?
            } else {
                file.keys
                    .iter()
                    .find(|k| k.kind == "windows")
                    .ok_or(Error::BadPassword)?
            };
            let key = unwrap(wrapped, password)?;
            let plain = decrypt(&key, &file.nonce, &file.ciphertext, CONTEXT)?;
            let data: VaultData = serde_json::from_slice(&plain)?;
            *state = Some(Unlocked { data, key });
        } else {
            let key = Zeroizing::new(random_bytes::<32>());
            let wrapped = wrap(
                if password.is_some() {
                    "password"
                } else {
                    "windows"
                },
                password,
                &key,
            )?;
            let unlocked = Unlocked {
                data: VaultData::default(),
                key,
            };
            self.write(&unlocked, &[wrapped]).await?;
            *state = Some(unlocked);
        }
        *self.last_activity.lock().await = Instant::now();
        Ok(())
    }
    pub async fn recover(&self, code: &str, new_password: &str) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Err(Error::AlreadyUnlocked);
        }
        let file = self.read_file()?;
        let wrapped = file
            .keys
            .iter()
            .find(|k| k.kind == "recovery")
            .ok_or(Error::BadPassword)?;
        let normalized = code.replace(['-', ' '], "").to_ascii_uppercase();
        let key = unwrap(wrapped, Some(&normalized))?;
        let plain = decrypt(&key, &file.nonce, &file.ciphertext, CONTEXT)?;
        let mut data: VaultData = serde_json::from_slice(&plain)?;
        data.tracked.clear(); // Recovery must not reattach to stale process IDs.
        let keys = vec![wrap("password", Some(new_password), &key)?];
        let unlocked = Unlocked { data, key };
        self.write(&unlocked, &keys).await?;
        *guard = Some(unlocked);
        *self.last_activity.lock().await = Instant::now();
        Ok(())
    }
    pub async fn lock(&self) {
        let mut state = self.inner.lock().await;
        *state = None;
    }
    pub async fn touch(&self) {
        *self.last_activity.lock().await = Instant::now();
    }
    pub async fn lock_if_idle(&self, minutes: u32) -> bool {
        if self.last_activity.lock().await.elapsed()
            >= Duration::from_secs(u64::from(minutes.max(1)) * 60)
        {
            let had_data = self.inner.lock().await.take().is_some();
            return had_data;
        }
        false
    }
    pub async fn status(&self) -> Result<crate::model::VaultStatus> {
        let exists = self.path.exists();
        let (mode, has_recovery) = if exists {
            let file = self.read_file()?;
            (
                if file.keys.iter().any(|k| k.kind == "password") {
                    "password"
                } else {
                    "windows"
                }
                .into(),
                file.keys.iter().any(|k| k.kind == "recovery"),
            )
        } else if cfg!(windows) {
            ("windows".into(), false)
        } else {
            ("password".into(), false)
        };
        Ok(crate::model::VaultStatus {
            exists,
            unlocked: self.inner.lock().await.is_some(),
            mode,
            has_recovery,
            hello_available: crate::platform::hello_available(),
            platform: std::env::consts::OS.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        })
    }
    pub async fn background_data(&self) -> Result<VaultData> {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|s| s.data.clone())
            .ok_or(Error::Locked)
    }
    pub async fn snapshot_data(&self) -> Result<VaultData> {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|s| s.data.clone())
            .ok_or(Error::Locked)
    }
    pub async fn with<R>(&self, f: impl FnOnce(&mut VaultData) -> Result<R>) -> Result<R> {
        self.touch().await;
        let mut guard = self.inner.lock().await;
        let state = guard.as_mut().ok_or(Error::Locked)?;
        let result = f(&mut state.data)?;
        let keys = self.read_file()?.keys;
        self.write(state, &keys).await?;
        Ok(result)
    }
    pub async fn read<R>(&self, f: impl FnOnce(&VaultData) -> Result<R>) -> Result<R> {
        self.touch().await;
        let guard = self.inner.lock().await;
        f(&guard.as_ref().ok_or(Error::Locked)?.data)
    }
    pub async fn cookie(&self, id: &str) -> Result<Secret> {
        // Background operations do not postpone an idle lock.
        self.inner
            .lock()
            .await
            .as_ref()
            .and_then(|s| s.data.secrets.iter().find(|a| a.id == id))
            .map(|a| a.cookie.clone())
            .ok_or(Error::Locked)
    }
    pub async fn replace_cookie(&self, id: &str, cookie: Secret) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let state = guard.as_mut().ok_or(Error::Locked)?;
        let secret = state
            .data
            .secrets
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)?;
        if secret.cookie == cookie {
            return Ok(());
        }
        secret.cookie = cookie;
        let keys = self.read_file()?.keys;
        self.write(state, &keys).await
    }
    pub async fn set_password(
        &self,
        current: Option<&str>,
        new_password: &str,
        recovery: bool,
    ) -> Result<Option<String>> {
        self.touch().await;
        let mut guard = self.inner.lock().await;
        let state = guard.as_mut().ok_or(Error::Locked)?;
        let mut file = self.read_file()?;
        if let Some(wrapped) = file.keys.iter().find(|k| k.kind == "password") {
            unwrap(wrapped, current)?;
        }
        password_key(new_password, &[0; 16])?;
        let mut keys = vec![wrap("password", Some(new_password), &state.key)?];
        let code = if recovery {
            let entropy = random_bytes::<20>();
            let code: String = entropy.iter().map(|b| format!("{b:02X}")).collect();
            keys.push(wrap("recovery", Some(&code), &state.key)?);
            Some(code)
        } else {
            None
        };
        self.write(state, &keys).await?;
        file.keys = keys;
        Ok(code)
    }
    pub async fn remove_password(&self, current: &str) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let state = guard.as_mut().ok_or(Error::Locked)?;
        let file = self.read_file()?;
        let wrapped = file
            .keys
            .iter()
            .find(|k| k.kind == "password")
            .ok_or(Error::BadPassword)?;
        unwrap(wrapped, Some(current))?;
        let keys = vec![wrap("windows", None, &state.key)?];
        self.write(state, &keys).await
    }
    pub async fn verify_reveal(&self, password: Option<&str>) -> Result<()> {
        let file = self.read_file()?;
        if let Some(wrapped) = file.keys.iter().find(|k| k.kind == "password") {
            unwrap(wrapped, password)?;
        } else {
            crate::platform::verify_hello().await?;
        }
        Ok(())
    }
    pub async fn reveal(&self, id: &str, password: Option<&str>) -> Result<String> {
        self.verify_reveal(password).await?;
        Ok(self.cookie(id).await?.expose().to_owned())
    }
    pub async fn export(&self, destination: &Path) -> Result<()> {
        let data = self.read_file()?;
        if destination == self.path {
            return Err(Error::Invalid("Choose a different backup filename."));
        }
        atomic_write(destination, &serde_json::to_vec_pretty(&data)?)
    }
}

/// Write and flush a sibling temporary file before replacing the previous vault.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or(Error::Invalid("Invalid vault directory."))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|_| Error::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_round_trip_and_tamper_detection() {
        let key = random_bytes::<32>();
        let wrapped = wrap("password", Some("correct horse battery staple"), &key).unwrap();
        assert_eq!(
            unwrap(&wrapped, Some("correct horse battery staple"))
                .unwrap()
                .as_ref(),
            &key
        );
        assert!(matches!(
            unwrap(&wrapped, Some("wrong password 123")),
            Err(Error::BadPassword)
        ));
        let mut tampered = wrapped.clone();
        tampered.ciphertext.push('A');
        assert!(unwrap(&tampered, Some("correct horse battery staple")).is_err());
        let (nonce, ciphertext) = encrypt(&key, b"private data", CONTEXT).unwrap();
        assert!(decrypt(&key, &nonce, &ciphertext, b"wrong context").is_err());
        assert_eq!(
            decrypt(&key, &nonce, &ciphertext, CONTEXT)
                .unwrap()
                .as_slice(),
            b"private data"
        );
    }
    #[test]
    fn secrets_never_appear_in_debug_output() {
        let secret = Secret::new("super-secret-cookie".into());
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
    }
    #[tokio::test]
    async fn encrypted_file_round_trip_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let vault = Vault::new(path.clone());
        vault
            .initialize(Some("a long test password"))
            .await
            .unwrap();
        vault
            .with(|d| {
                d.settings.language = "en".into();
                Ok(())
            })
            .await
            .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("\"accounts\""));
        assert!(!raw.contains("\"cookie\""));
        vault.lock().await;
        assert!(matches!(vault.snapshot_data().await, Err(Error::Locked)));
        vault
            .initialize(Some("a long test password"))
            .await
            .unwrap();
        assert_eq!(vault.snapshot_data().await.unwrap().settings.language, "en");
        vault.lock().await;
        assert!(vault.initialize(Some("incorrect password")).await.is_err());
    }
    #[tokio::test]
    async fn background_reads_do_not_postpone_idle_lock() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().join("vault.json"));
        vault
            .initialize(Some("background test password"))
            .await
            .unwrap();
        *vault.last_activity.lock().await = Instant::now() - Duration::from_secs(120);
        assert!(vault.background_data().await.is_ok());
        assert!(vault.snapshot_data().await.is_ok());
        assert!(vault.cookie("missing").await.is_err());
        assert!(vault.lock_if_idle(1).await);
        assert!(matches!(vault.background_data().await, Err(Error::Locked)));
    }
    #[tokio::test]
    async fn recovery_rotates_password_and_keeps_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().join("vault.json"));
        vault
            .initialize(Some("initial password 123"))
            .await
            .unwrap();
        let code = vault
            .set_password(Some("initial password 123"), "initial password 123", true)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(code.len(), 40);
        assert_eq!(code, code.replace(['-', ' '], "").to_ascii_uppercase());
        vault.lock().await;
        vault
            .recover(&code, "replacement password 123")
            .await
            .unwrap();
        vault.lock().await;
        vault
            .initialize(Some("replacement password 123"))
            .await
            .unwrap();
        assert!(!vault.status().await.unwrap().has_recovery);
    }
}
