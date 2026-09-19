//! Encrypted local vault. Process injection logs disclosure, not every downstream use.
use anyhow::{Context, Result, bail};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::os::fd::AsRawFd;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Contents {
    secrets: BTreeMap<String, String>,
    audit: Vec<Value>,
}
pub struct Vault {
    _lock: std::fs::File,
    path: PathBuf,
    key: aead::LessSafeKey,
    contents: Contents,
}
impl Vault {
    pub fn open_default() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let dir = PathBuf::from(std::env::var_os("HOME").context("home unavailable")?)
                .join("Library/Application Support/DOT Terminal/vault");
            std::fs::create_dir_all(&dir)?;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            let path = dir.join("vault.bin");
            let key = match security_framework::passwords::get_generic_password(
                "org.dotprotocol.terminal.vault.v1",
                "master",
            ) {
                Ok(key) => key,
                Err(e) if e.code() == -25300 && !path.exists() => {
                    let mut key = vec![0; 32];
                    SystemRandom::new()
                        .fill(&mut key)
                        .map_err(|_| anyhow::anyhow!("randomness unavailable"))?;
                    security_framework::passwords::set_generic_password(
                        "org.dotprotocol.terminal.vault.v1",
                        "master",
                        &key,
                    )?;
                    key
                }
                Err(e) => return Err(e.into()),
            };
            Self::open(path, &key)
        }
        #[cfg(not(target_os = "macos"))]
        {
            bail!("OS key provider is currently implemented for macOS only")
        }
    }
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn open(path: PathBuf, key: &[u8]) -> Result<Self> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path.with_extension("lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            bail!("vault is open in another DOT process")
        }
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, key)
                .map_err(|_| anyhow::anyhow!("invalid vault key"))?,
        );
        let contents = if path.exists() {
            let m = std::fs::symlink_metadata(&path)?;
            if !m.is_file()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o077 != 0
                || m.len() > 8 * 1024 * 1024
            {
                bail!("unsafe vault file")
            }
            let mut data = std::fs::read(&path)?;
            if data.len() < 12 {
                bail!("damaged vault")
            }
            let nonce = aead::Nonce::try_assume_unique_for_key(&data[..12])
                .map_err(|_| anyhow::anyhow!("damaged nonce"))?;
            let plain = key
                .open_in_place(nonce, aead::Aad::from(b"DOT-VAULT-V1"), &mut data[12..])
                .map_err(|_| anyhow::anyhow!("vault authentication failed"))?;
            serde_json::from_slice(plain)?
        } else {
            Contents::default()
        };
        let mut vault = Self {
            _lock: lock,
            path,
            key,
            contents,
        };
        vault.event("opened", json!({"authority":"local desktop owner"}))?;
        vault.save()?;
        Ok(vault)
    }
    fn event(&mut self, action: &str, details: Value) -> Result<()> {
        if self.contents.audit.len() >= 10000 {
            bail!("audit capacity reached; export before continuing")
        }
        self.contents.audit.push(json!({"sequence":self.contents.audit.len()+1,"at":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),"action":action,"details":details}));
        Ok(())
    }
    fn update<T>(&mut self, change: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let previous = self.contents.clone();
        let result = change(self).and_then(|value| self.save().map(|()| value));
        if result.is_err() {
            self.contents = previous;
        }
        result
    }
    fn save(&self) -> Result<()> {
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| anyhow::anyhow!("randomness unavailable"))?;
        let mut encrypted = serde_json::to_vec(&self.contents)?;
        if encrypted.len() + 12 + aead::AES_256_GCM.tag_len() > 8 * 1024 * 1024 {
            bail!("vault storage capacity reached")
        }
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(b"DOT-VAULT-V1"),
                &mut encrypted,
            )
            .map_err(|_| anyhow::anyhow!("vault encryption failed"))?;
        let temp = self
            .path
            .with_extension(format!("{}.tmp", hex::encode(nonce)));
        let result = (|| -> Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&temp)?;
            f.write_all(&nonce)?;
            f.write_all(&encrypted)?;
            f.sync_all()?;
            std::fs::rename(&temp, &self.path)?;
            std::fs::File::open(self.path.parent().unwrap())?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
    pub fn list(&self) -> Value {
        json!({"secrets":self.contents.secrets.keys().collect::<Vec<_>>(),"audit":self.contents.audit,"protection":"AES-256-GCM at rest; master key in macOS Keychain","limitation":"Environment injection reveals plaintext to the child process. Audit records disclosure, not every later use. Local owner or malware with equivalent access can defeat local controls."})
    }
    pub fn put(&mut self, name: String, value: String) -> Result<()> {
        if name.is_empty()
            || name.len() > 128
            || !name.bytes().all(|x| x.is_ascii_alphanumeric() || x == b'_')
            || value.len() > 65536
        {
            bail!("invalid secret")
        }
        if self.contents.secrets.len() >= 128 && !self.contents.secrets.contains_key(&name) {
            bail!("vault full")
        }
        self.update(|vault| {
            vault.event("stored", json!({"secret":name}))?;
            vault.contents.secrets.insert(name, value);
            Ok(())
        })
    }
    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.update(|vault| {
            vault.event("deleted", json!({"secret":name}))?;
            vault.contents.secrets.remove(name);
            Ok(())
        })
    }
    pub fn release(
        &mut self,
        names: &[String],
        command: &Path,
        args: &[String],
    ) -> Result<BTreeMap<String, String>> {
        if names.len() > 128 {
            bail!("too many secrets")
        }
        let mut values = BTreeMap::new();
        for name in names {
            values.insert(
                name.clone(),
                self.contents
                    .secrets
                    .get(name)
                    .context("secret not found")?
                    .clone(),
            );
        }
        let executable = std::fs::canonicalize(command)?;
        let bytes = std::fs::read(&executable)?;
        let digest = hex::encode(ring::digest::digest(&ring::digest::SHA256, &bytes));
        // Argument values can themselves be secrets. Store only count and executable identity.
        self.update(|vault| {
            vault.event("released_to_process_launch",json!({"secrets":names,"executable":executable,"sha256":digest,"argument_count":args.len(),"authority":"local desktop owner","mode":"plaintext environment compatibility"}))?;
            Ok(values)
        })
    }

    pub fn launched(&mut self, session: &str) -> Result<()> {
        self.update(|vault| vault.event("session_started", json!({"session":session})))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_roundtrip_tamper_and_release_audit() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vault");
        let mut v = Vault::open(p.clone(), &[7; 32]).unwrap();
        v.put("SYNTHETIC".into(), "never-store-me-in-plaintext".into())
            .unwrap();
        assert!(!String::from_utf8_lossy(&std::fs::read(&p).unwrap()).contains("never-store"));
        drop(v);
        let mut v = Vault::open(p.clone(), &[7; 32]).unwrap();
        let keys = v
            .release(&["SYNTHETIC".into()], Path::new("/bin/sh"), &[])
            .unwrap();
        assert_eq!(keys["SYNTHETIC"], "never-store-me-in-plaintext");
        assert_eq!(
            v.list()["audit"].as_array().unwrap().last().unwrap()["action"],
            "released_to_process_launch"
        );
        drop(v);
        assert!(Vault::open(p.clone(), &[8; 32]).is_err());
        let mut bytes = std::fs::read(&p).unwrap();
        bytes[15] ^= 1;
        std::fs::write(&p, bytes).unwrap();
        assert!(Vault::open(p, &[7; 32]).is_err());
    }
    #[test]
    fn storage_limit_preserves_previous_file_and_in_memory_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault");
        let mut vault = Vault::open(path.clone(), &[3; 32]).unwrap();
        vault.put("KEEP".into(), "retained".into()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let audit = vault.contents.audit.len();
        assert!(
            vault
                .update(|v| {
                    v.contents
                        .secrets
                        .insert("OVERSIZED".into(), "x".repeat(8 * 1024 * 1024));
                    v.event("test", json!({}))
                })
                .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert!(!vault.contents.secrets.contains_key("OVERSIZED"));
        assert_eq!(vault.contents.secrets["KEEP"], "retained");
        assert_eq!(vault.contents.audit.len(), audit);
    }
}
