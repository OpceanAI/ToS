//! Key Encapsulation Mechanism (KEM) / Diffie-Hellman trait layer.
//!
//! Provides a uniform interface for key agreement algorithms. Backed by
//! `x25519-dalek` for X25519 ECDH (v1.0) and prepared for post-quantum
//! algorithms (ML-KEM-768, X-Wing) added in v0.2.

use crate::error::{CryptoError, CryptoResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KexId {
    /// X25519 ECDH (RFC 7748). Classical; not post-quantum.
    X25519,
    /// ML-KEM-512 (FIPS 203). Post-quantum, NIST Level 1.
    MlKem512,
    /// ML-KEM-768 (FIPS 203). Post-quantum, NIST Level 3. Default PQ.
    MlKem768,
    /// ML-KEM-1024 (FIPS 203). Post-quantum, NIST Level 5.
    MlKem1024,
    /// X-Wing hybrid (X25519 + ML-KEM-768). Best of both worlds.
    XWing,
}

impl KexId {
    pub fn name(&self) -> &'static str {
        match self {
            KexId::X25519 => "X25519",
            KexId::MlKem512 => "ML-KEM-512",
            KexId::MlKem768 => "ML-KEM-768",
            KexId::MlKem1024 => "ML-KEM-1024",
            KexId::XWing => "X-Wing",
        }
    }

    pub fn is_post_quantum(&self) -> bool {
        !matches!(self, KexId::X25519)
    }

    pub fn pk_size(&self) -> usize {
        match self {
            KexId::X25519 => 32,
            KexId::MlKem512 => 800,
            KexId::MlKem768 => 1184,
            KexId::MlKem1024 => 1568,
            KexId::XWing => 1216,
        }
    }

    pub fn sk_size(&self) -> usize {
        match self {
            KexId::X25519 => 32,
            KexId::MlKem512 => 1632,
            KexId::MlKem768 => 2400,
            KexId::MlKem1024 => 3168,
            KexId::XWing => 32,
        }
    }

    pub fn ct_size(&self) -> usize {
        match self {
            KexId::X25519 => 32,
            KexId::MlKem512 => 768,
            KexId::MlKem768 => 1088,
            KexId::MlKem1024 => 1568,
            KexId::XWing => 1120,
        }
    }

    pub fn ss_size(&self) -> usize {
        match self {
            KexId::X25519 => 32,
            KexId::MlKem512 => 32,
            KexId::MlKem768 => 32,
            KexId::MlKem1024 => 32,
            KexId::XWing => 32,
        }
    }

    pub fn all() -> &'static [KexId] {
        &[
            KexId::X25519,
            KexId::MlKem512,
            KexId::MlKem768,
            KexId::MlKem1024,
            KexId::XWing,
        ]
    }
}

/// Key Encapsulation / Diffie-Hellman algorithm.
///
/// Implementations are stateless and `Send + Sync`; safe to share via
/// `Arc<dyn Kex>`. The same instance can serve concurrent callers.
///
/// Sizes live on [`KexId`] (via `pk_size()` etc.) to keep this trait
/// dyn-compatible.
pub trait Kex: Send + Sync + 'static {
    fn id(&self) -> KexId;

    fn name(&self) -> &'static str {
        self.id().name()
    }

    /// Generate a fresh keypair. Returns `(pk, sk)`.
    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)>;

    /// Encapsulate against a remote public key. Returns `(ct, ss)`.
    ///
    /// For pure DH (X25519), an ephemeral keypair is generated and the
    /// ciphertext is the ephemeral public key.
    /// For KEM (ML-KEM, X-Wing), the native encapsulation is used.
    fn encapsulate(&self, remote_pk: &[u8]) -> CryptoResult<(Vec<u8>, Vec<u8>)>;

    /// Decapsulate the ciphertext using the local secret key. Returns `ss`.
    fn decapsulate(&self, sk: &[u8], ct: &[u8]) -> CryptoResult<Vec<u8>>;
}

/// Validate a buffer length against an expected constant.
pub(crate) fn check_len(name: &str, expected: usize, actual: usize) -> CryptoResult<()> {
    if actual != expected {
        Err(CryptoError::InvalidLength {
            expected,
            actual,
        })
        .map_err(|e| {
            tracing::debug!(target: "tos.crypto.kex", "{}: {}", name, e);
            e
        })
    } else {
        Ok(())
    }
}

pub mod x25519;

pub use x25519::X25519Kex;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn kex_id_name_matches() {
        assert_eq!(KexId::X25519.name(), "X25519");
        assert_eq!(KexId::MlKem768.name(), "ML-KEM-768");
        assert_eq!(KexId::XWing.name(), "X-Wing");
    }

    #[test]
    fn kex_id_pq_flag() {
        assert!(!KexId::X25519.is_post_quantum());
        assert!(KexId::MlKem768.is_post_quantum());
        assert!(KexId::XWing.is_post_quantum());
    }

    #[test]
    fn kex_id_all_includes_classic_and_pq() {
        let all = KexId::all();
        assert!(all.contains(&KexId::X25519));
        assert!(all.contains(&KexId::MlKem768));
    }

    #[test]
    fn kex_trait_is_object_safe() {
        let kex: Arc<dyn Kex> = Arc::new(X25519Kex);
        let (pk, sk) = kex.generate().expect("generate");
        assert_eq!(pk.len(), KexId::X25519.pk_size());
        assert_eq!(sk.len(), KexId::X25519.sk_size());
    }
}
