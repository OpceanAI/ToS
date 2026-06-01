use std::path::{Path, PathBuf};
use std::time::SystemTime;

use base64::Engine;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{CryptoError, CryptoResult};
use crate::hash::blake3_hash;
use crate::sign::{
    public_key_bytes, sign, verifying_key_from_bytes, SIGNATURE_SIZE,
};

pub const NODE_ID_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub [u8; NODE_ID_SIZE]);

impl NodeId {
    pub fn as_bytes(&self) -> &[u8; NODE_ID_SIZE] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> CryptoResult<Self> {
        let bytes = hex::decode(s).map_err(|e| CryptoError::Serde(e.to_string()))?;
        if bytes.len() != NODE_ID_SIZE {
            return Err(CryptoError::InvalidLength {
                expected: NODE_ID_SIZE,
                actual: bytes.len(),
            });
        }
        let mut arr = [0u8; NODE_ID_SIZE];
        arr.copy_from_slice(&bytes);
        Ok(NodeId(arr))
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIdentity {
    version: u8,
    public_key: String,
    secret_key: String,
    node_id: String,
    created_at: u64,
    comment: Option<String>,
}

pub struct Identity {
    signing_key: SigningKey,
    node_id: NodeId,
    path: Option<PathBuf>,
    created_at: SystemTime,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("node_id", &self.node_id)
            .field("public_key_hex", &hex::encode(public_key_bytes(&self.signing_key.verifying_key())))
            .field("path", &self.path)
            .finish()
    }
}

impl Identity {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let node_id = compute_node_id(&signing_key);
        Self {
            signing_key,
            node_id,
            path: None,
            created_at: SystemTime::now(),
        }
    }

    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let node_id = compute_node_id(&signing_key);
        Self {
            signing_key,
            node_id,
            path: None,
            created_at: SystemTime::now(),
        }
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    pub async fn load_or_create(path: impl AsRef<Path>) -> CryptoResult<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            Self::load(&path).await
        } else {
            let id = Self::generate();
            id.save(&path).await?;
            Ok(id)
        }
    }

    pub async fn load(path: &Path) -> CryptoResult<Self> {
        let mut file = fs::File::open(path).await?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await?;
        let stored: StoredIdentity = serde_json::from_slice(&buf)
            .map_err(|e| CryptoError::Serde(e.to_string()))?;
        let secret_bytes = base64::engine::general_purpose::STANDARD
            .decode(&stored.secret_key)
            .map_err(|e| CryptoError::Serde(e.to_string()))?;
        if secret_bytes.len() != 32 {
            return Err(CryptoError::InvalidLength {
                expected: 32,
                actual: secret_bytes.len(),
            });
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&secret_bytes);
        let signing_key = SigningKey::from_bytes(&arr);
        let node_id = NodeId::from_hex(&stored.node_id)?;
        let expected_node_id = compute_node_id(&signing_key);
        if node_id != expected_node_id {
            return Err(CryptoError::Serde(
                "node_id mismatch: file may be corrupted or tampered".into(),
            ));
        }
        let created_at = std::time::UNIX_EPOCH
            .checked_add(std::time::Duration::from_secs(stored.created_at))
            .unwrap_or(SystemTime::now());
        Ok(Self {
            signing_key,
            node_id,
            path: Some(path.to_path_buf()),
            created_at,
        })
    }

    pub async fn save(&self, path: &Path) -> CryptoResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }
        let secret = self.signing_key.to_bytes();
        let public = public_key_bytes(&self.signing_key.verifying_key());
        let stored = StoredIdentity {
            version: 1,
            public_key: base64::engine::general_purpose::STANDARD.encode(public),
            secret_key: base64::engine::general_purpose::STANDARD.encode(secret),
            node_id: self.node_id.to_hex(),
            created_at: self
                .created_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            comment: Some("ToS node identity — keep this file secret (0600)".into()),
        };
        let json = serde_json::to_vec_pretty(&stored)
            .map_err(|e| CryptoError::Serde(e.to_string()))?;
        let tmp = path.with_extension("tmp");
        {
            let mut file = fs::File::create(&tmp).await?;
            file.write_all(&json).await?;
            file.sync_all().await?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&tmp, perms)?;
        }
        fs::rename(&tmp, path).await?;
        Ok(())
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        public_key_bytes(&self.signing_key.verifying_key())
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key())
    }

    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_SIZE] {
        sign(&self.signing_key, message)
    }

    pub fn verify(
        public_key: &[u8; 32],
        message: &[u8],
        signature: &[u8],
    ) -> CryptoResult<()> {
        let vk = verifying_key_from_bytes(public_key)?;
        crate::sign::verify(&vk, message, signature)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn created_at(&self) -> SystemTime {
        self.created_at
    }
}

fn compute_node_id(sk: &SigningKey) -> NodeId {
    let pk = public_key_bytes(&sk.verifying_key());
    NodeId(blake3_hash(&pk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_has_consistent_node_id() {
        let id = Identity::generate();
        let pk = id.public_key();
        let expected = blake3_hash(&pk);
        assert_eq!(id.node_id().as_bytes(), &expected);
    }

    #[test]
    fn node_id_hex_roundtrip() {
        let id = Identity::generate();
        let hex = id.node_id().to_hex();
        let parsed = NodeId::from_hex(&hex).unwrap();
        assert_eq!(parsed, id.node_id());
    }

    #[test]
    fn node_id_display_is_hex() {
        let id = Identity::generate();
        let s = id.node_id().to_string();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_and_verify_own() {
        let id = Identity::generate();
        let msg = b"the cake is a lie";
        let sig = id.sign(msg);
        assert!(Identity::verify(&id.public_key(), msg, &sig).is_ok());
    }

    #[test]
    fn sign_and_verify_other_key() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let msg = b"cross-node message";
        let sig = alice.sign(msg);
        assert!(Identity::verify(&bob.public_key(), msg, &sig).is_err());
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let dir = tempdir_in_target();
        let path = dir.join("identity.json");
        let alice = Identity::generate();
        alice.save(&path).await.unwrap();
        let loaded = Identity::load(&path).await.unwrap();
        assert_eq!(loaded.node_id(), alice.node_id());
        assert_eq!(loaded.public_key(), alice.public_key());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn load_or_create_creates_when_missing() {
        let dir = tempdir_in_target();
        let path = dir.join("new_identity.json");
        assert!(!path.exists());
        let id = Identity::load_or_create(&path).await.unwrap();
        assert!(path.exists());
        assert_eq!(id.node_id(), id.node_id());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn load_or_create_preserves_existing() {
        let dir = tempdir_in_target();
        let path = dir.join("existing.json");
        let first = Identity::generate();
        first.save(&path).await.unwrap();
        let loaded = Identity::load_or_create(&path).await.unwrap();
        assert_eq!(loaded.node_id(), first.node_id());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn tempdir_in_target() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "tos-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
