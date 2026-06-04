// Encrypted credentials store.
//
// Credentials are serialized as JSON and encrypted with simple-encrypt
// (AES-GCM-256). The encryption key is derived from a stable per-machine
// identifier, so the encrypted file is only usable on the machine that
// wrote it. There is no passphrase.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

// App-specific salt mixed into key derivation. Changing this invalidates
// every previously saved credentials file.
const KEY_SALT: &[u8] = b"mobicontrol-api-tester:v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
    pub password: String,
    pub fqdn: String,
}

pub fn save(creds: &Credentials, path: &Path) -> Result<()> {
    let key = machine_key()?;
    let json = serde_json::to_vec(creds).context("Failed to serialize credentials")?;
    let encrypted = simple_encrypt::encrypt_bytes(&json, &key)
        .map_err(|e| anyhow!("Failed to encrypt credentials: {e:?}"))?;
    std::fs::write(path, encrypted)
        .with_context(|| format!("Failed to write credentials file: {}", path.display()))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Credentials> {
    let key = machine_key()?;
    let encrypted = std::fs::read(path)
        .with_context(|| format!("Failed to read credentials file: {}", path.display()))?;
    let decrypted = simple_encrypt::decrypt_bytes(&encrypted, &key).map_err(|e| {
        anyhow!(
            "Failed to decrypt credentials. The file is only readable on the \
             machine that wrote it. ({e:?})"
        )
    })?;
    let creds: Credentials = serde_json::from_slice(&decrypted)
        .context("Failed to parse credentials JSON after decryption")?;
    Ok(creds)
}

// --- Key derivation ---------------------------------------------------------

/// Derive a 32-byte AES key from a stable per-machine identifier.
fn machine_key() -> Result<[u8; 32]> {
    let id = machine_id().context("Failed to obtain a machine identifier")?;
    let mut hasher = Sha256::new();
    hasher.update(KEY_SALT);
    hasher.update(id.as_bytes());
    Ok(hasher.finalize().into())
}

#[cfg(target_os = "linux")]
fn machine_id() -> Result<String> {
    // /etc/machine-id is the systemd canonical location.
    // /var/lib/dbus/machine-id is the older D-Bus fallback (often a symlink
    // to the first on modern distros, but read it anyway in case).
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(s) = std::fs::read_to_string(path) {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
    }
    Err(anyhow!(
        "Could not read /etc/machine-id or /var/lib/dbus/machine-id"
    ))
}

#[cfg(target_os = "windows")]
fn machine_id() -> Result<String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    // Force the 64-bit view so a 32-bit build won't be silently redirected
    // to WOW6432Node (where MachineGuid does not exist).
    let key = hklm
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .context("Failed to open HKLM\\SOFTWARE\\Microsoft\\Cryptography")?;
    let guid: String = key
        .get_value("MachineGuid")
        .context("Failed to read MachineGuid")?;
    Ok(guid)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn machine_id() -> Result<String> {
    Err(anyhow!(
        "Machine-bound encryption is not implemented for this OS"
    ))
}
