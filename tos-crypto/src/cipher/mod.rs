//! Authenticated Encryption with Associated Data (AEAD) trait layer.
//!
//! Uniform interface over `chacha20poly1305`, `aes-gcm`, and the like. A
//! `None` cipher is provided for plaintext mode (debugging only).

use crate::error::{CryptoError, CryptoResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CipherId {
    /// No encryption. Ciphertext == plaintext. Tag == 0 bytes.
    None,
    /// ChaCha20-Poly1305 (RFC 7539 / RFC 8439). 12-byte nonce, 16-byte tag.
    ChaCha20Poly1305,
    /// XChaCha20-Poly1305 (draft-irtf-cfrg-xchacha). 24-byte nonce.
    XChaCha20Poly1305,
    /// AES-256-GCM (NIST SP 800-38D). 12-byte nonce, 16-byte tag.
    Aes256Gcm,
    /// AEGIS-256 (NIST 2024 lightweight AEAD portfolio).
    Aegis256,
    /// HPKE (RFC 9180) using the negotiated KEM (e.g. X-Wing).
    HpkeXwing,
}

impl CipherId {
    pub fn name(&self) -> &'static str {
        match self {
            CipherId::None => "None",
            CipherId::ChaCha20Poly1305 => "ChaCha20-Poly1305",
            CipherId::XChaCha20Poly1305 => "XChaCha20-Poly1305",
            CipherId::Aes256Gcm => "AES-256-GCM",
            CipherId::Aegis256 => "AEGIS-256",
            CipherId::HpkeXwing => "HPKE-XWing",
        }
    }

    pub fn is_encrypting(&self) -> bool {
        !matches!(self, CipherId::None)
    }

    pub fn key_size(&self) -> usize {
        match self {
            CipherId::None => 0,
            CipherId::ChaCha20Poly1305 => 32,
            CipherId::XChaCha20Poly1305 => 32,
            CipherId::Aes256Gcm => 32,
            CipherId::Aegis256 => 32,
            CipherId::HpkeXwing => 32,
        }
    }

    pub fn nonce_size(&self) -> usize {
        match self {
            CipherId::None => 0,
            CipherId::ChaCha20Poly1305 => 12,
            CipherId::XChaCha20Poly1305 => 24,
            CipherId::Aes256Gcm => 12,
            CipherId::Aegis256 => 32,
            CipherId::HpkeXwing => 12,
        }
    }

    pub fn tag_size(&self) -> usize {
        match self {
            CipherId::None => 0,
            CipherId::ChaCha20Poly1305
            | CipherId::XChaCha20Poly1305
            | CipherId::Aes256Gcm
            | CipherId::Aegis256
            | CipherId::HpkeXwing => 16,
        }
    }

    pub fn all() -> &'static [CipherId] {
        &[
            CipherId::None,
            CipherId::ChaCha20Poly1305,
            CipherId::XChaCha20Poly1305,
            CipherId::Aes256Gcm,
            CipherId::Aegis256,
            CipherId::HpkeXwing,
        ]
    }
}

/// AEAD algorithm.
///
/// Sizes live on [`CipherId`] (via `key_size()` etc.) to keep this trait
/// dyn-compatible.
pub trait Cipher: Send + Sync + 'static {
    fn id(&self) -> CipherId;

    fn name(&self) -> &'static str {
        self.id().name()
    }

    /// Encrypt `pt` under `key` + `nonce` with associated data `aad`.
    /// Returns ciphertext (with authentication tag appended).
    fn encrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        aad: &[u8],
        pt: &[u8],
    ) -> CryptoResult<Vec<u8>>;

    /// Decrypt `ct` under `key` + `nonce` with associated data `aad`.
    /// Returns plaintext. Errors on tag mismatch (AEAD authentication).
    fn decrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        aad: &[u8],
        ct: &[u8],
    ) -> CryptoResult<Vec<u8>>;
}

pub(crate) fn check_len(_name: &str, expected: usize, actual: usize) -> CryptoResult<()> {
    if actual != expected {
        Err(CryptoError::InvalidLength {
            expected,
            actual,
        })
    } else {
        Ok(())
    }
}

pub mod chacha20poly1305;
pub mod none;
pub mod xchacha20poly1305;

pub use chacha20poly1305::ChaCha20Poly1305Cipher;
pub use none::NoneCipher;
pub use xchacha20poly1305::XChaCha20Poly1305Cipher;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn cipher_id_name_matches() {
        assert_eq!(CipherId::None.name(), "None");
        assert_eq!(CipherId::ChaCha20Poly1305.name(), "ChaCha20-Poly1305");
        assert_eq!(CipherId::XChaCha20Poly1305.name(), "XChaCha20-Poly1305");
    }

    #[test]
    fn cipher_id_encrypting_flag() {
        assert!(!CipherId::None.is_encrypting());
        assert!(CipherId::ChaCha20Poly1305.is_encrypting());
        assert!(CipherId::Aes256Gcm.is_encrypting());
    }

    #[test]
    fn cipher_trait_is_object_safe() {
        let c: Arc<dyn Cipher> = Arc::new(ChaCha20Poly1305Cipher);
        assert_eq!(c.id(), CipherId::ChaCha20Poly1305);
    }
}
