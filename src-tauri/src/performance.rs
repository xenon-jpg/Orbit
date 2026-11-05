use crate::{
    error::{Error, Result},
    model::Performance,
    vault::atomic_write,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// These are rendering settings, not executable code or arbitrary user flags.
pub fn flags(profile: &Performance) -> Map<String, Value> {
    let mut map = Map::new();
    let mut put = |key: &str, value: &str| {
        map.insert(key.into(), Value::String(value.into()));
    };
    if profile.texture_quality > 0 {
        put("DFFlagTextureQualityOverrideEnabled", "True");
        put(
            "DFIntTextureQualityOverride",
            &(profile.texture_quality - 1).to_string(),
        );
    }
    if profile.disable_grass {
        put("FIntFRMMinGrassDistance", "0");
        put("FIntFRMMaxGrassDistance", "0");
    }
    if profile.gray_sky {
        put("FFlagDebugSkyGray", "True");
    }
    if profile.disable_shadows {
        put("DFIntDebugFRMQualityLevelOverride", "1");
    }
    map
}
pub fn merge(original: Option<&[u8]>, profile: &Performance) -> Result<Vec<u8>> {
    let mut value = match original {
        Some(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice::<Value>(bytes)
            .map_err(|_| {
                Error::Invalid(
                    "Existing ClientAppSettings.json is not valid JSON; it was not changed.",
                )
            })?,
        None => Value::Object(Map::new()),
        _ => return Err(Error::Invalid("Existing client settings are too large.")),
    };
    let object = value.as_object_mut().ok_or(Error::Invalid(
        "Existing client settings must be a JSON object.",
    ))?;
    for (key, value) in flags(profile) {
        object.insert(key, value);
    }
    serde_json::to_vec_pretty(&value).map_err(|_| Error::Internal)
}

pub struct Transaction {
    pub file: PathBuf,
    pub original: Option<Vec<u8>>,
    pub applied: Vec<u8>,
    pub fingerprint: String,
    journal: PathBuf,
}
impl Transaction {
    pub fn open(client_dir: &Path, backup_dir: &Path, profile: &Performance) -> Result<Self> {
        let file = client_dir
            .join("ClientSettings")
            .join("ClientAppSettings.json");
        if let Some(existing) = std::fs::symlink_metadata(&file).ok() {
            if existing.file_type().is_symlink() || !existing.is_file() {
                return Err(Error::Invalid("Client settings must be a regular file."));
            }
        }
        let original = match std::fs::read(&file) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Error::Internal),
        };
        let applied = merge(original.as_deref(), profile)?;
        let mut fingerprint = format!("{profile:?}");
        fingerprint = format!("{:x}", md5_like(fingerprint.as_bytes()));
        let journal = backup_dir.join("performance-transaction.json");
        let tx = Self {
            file,
            original,
            applied,
            fingerprint,
            journal,
        };
        // Persist the undo record before touching Roblox's settings.
        atomic_write(
            &tx.journal,
            &serde_json::to_vec(&serde_json::json!({
                "file": tx.file, "original": tx.original, "applied": tx.applied
            }))?,
        )?;
        atomic_write(&tx.file, &tx.applied)?;
        Ok(tx)
    }
    pub fn restore(&self) -> Result<()> {
        // Never overwrite a newer version installer or a user's subsequent edits.
        let current = std::fs::read(&self.file).ok();
        if current.as_deref() != Some(&self.applied) {
            let _ = std::fs::remove_file(&self.journal);
            return Ok(());
        }
        if let Some(original) = &self.original {
            atomic_write(&self.file, original)?;
        } else {
            std::fs::remove_file(&self.file)?;
        }
        std::fs::remove_file(&self.journal)?;
        Ok(())
    }
}
/// Recover only an exact, unfinished AltDeck write after a crash.
pub fn recover(backup_dir: &Path) -> Result<()> {
    let path = backup_dir.join("performance-transaction.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(Error::Internal),
    };
    if bytes.len() > 3 * 1024 * 1024 {
        return Err(Error::Invalid("Invalid performance recovery journal."));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let file = PathBuf::from(value["file"].as_str().ok_or(Error::BadVault)?);
    let applied = bytes_field(&value["applied"])?;
    if std::fs::read(&file).ok().as_deref() != Some(&applied) {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    if value["original"].is_null() {
        let _ = std::fs::remove_file(file);
    } else {
        atomic_write(&file, &bytes_field(&value["original"])?)?;
    }
    std::fs::remove_file(path)?;
    Ok(())
}
fn bytes_field(value: &Value) -> Result<Vec<u8>> {
    value
        .as_array()
        .ok_or(Error::BadVault)?
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(Error::BadVault)
        })
        .collect()
}
fn md5_like(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, hash)
}
fn hash(hash: u64, byte: &u8) -> u64 {
    (hash ^ *byte as u64).wrapping_mul(0x100000001b3)
}

/// Apply the documented client frame cap without depending on an XML parser.
/// Call only while no managed client is running; do not race Roblox's own writer.
pub fn frame_cap(fps: u32) -> Result<()> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or(Error::WindowsRequired)?;
    let file = PathBuf::from(local)
        .join("Roblox")
        .join("GlobalBasicSettings_13.xml");
    if !file.exists() {
        return Ok(());
    }
    let bytes = std::fs::read_to_string(&file)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("Roblox settings file is too large."));
    }
    let re =
        regex::Regex::new(r#"<int name="FramerateCap">\d+</int>"#).map_err(|_| Error::Internal)?;
    let replacement = format!("<int name=\"FramerateCap\">{}</int>", fps.clamp(10, 240));
    let result = if re.is_match(&bytes) {
        re.replace(&bytes, replacement.as_str()).into_owned()
    } else {
        bytes.replace(
            "</Properties>",
            &format!("\t{replacement}\n\t\t</Properties>"),
        )
    };
    atomic_write(&file, result.as_bytes())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_unrelated_flags_and_generates_only_allowlisted_keys() {
        let original = br#"{"FFlagUserSetting":true,"DFIntTextureQualityOverride":"3"}"#;
        let output = merge(
            Some(original),
            &Performance {
                texture_quality: 1,
                disable_grass: true,
                ..Default::default()
            },
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["FFlagUserSetting"], true);
        assert_eq!(value["DFIntTextureQualityOverride"], "0");
        assert_eq!(value["FIntFRMMaxGrassDistance"], "0");
        assert!(merge(Some(b"not json"), &Performance::default()).is_err());
    }
    #[test]
    fn transaction_restores_original() {
        let dir = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let client = dir.path().join("version");
        std::fs::create_dir(&client).unwrap();
        let tx = Transaction::open(
            &client,
            backups.path(),
            &Performance {
                gray_sky: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(std::fs::read_to_string(&tx.file)
            .unwrap()
            .contains("FFlagDebugSkyGray"));
        tx.restore().unwrap();
        assert!(!tx.file.exists());
    }
}
