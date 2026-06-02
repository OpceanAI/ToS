//! Digital signature trait layer.
//!
//! Uniform interface for Ed25519 (v1.0) and post-quantum algorithms
//! (ML-DSA-65, SLH-DSA-128s, FROST Ed25519) added in v0.2.

use crate::error::CryptoResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SigId {
    /// Ed25519 (RFC 8032). Classical. 32-byte pk, 64-byte sig.
    Ed25519,
    /// ML-DSA-65 (FIPS 204). Post-quantum, NIST Level 3.
    MlDsa65,
    /// SLH-DSA-128s (FIPS 205). Hash-based PQ, conservative fallback.
    SlhDsa128s,
    /// FROST Ed25519 threshold 3-of-5 (RFC 9591).
    FrostEd25519,
}

impl SigId {
    pub fn name(&self) -> &'static str {
        match self {
            SigId::Ed25519 => "Ed25519",
            SigId::MlDsa65 => "ML-DSA-65",
            SigId::SlhDsa128s => "SLH-DSA-128s",
            SigId::FrostEd25519 => "FROST-Ed25519",
        }
    }

    pub fn is_post_quantum(&self) -> bool {
        matches!(self, SigId::MlDsa65 | SigId::SlhDsa128s)
    }

    pub fn pk_size(&self) -> usize {
        match self {
            SigId::Ed25519 => 32,
            SigId::MlDsa65 => 1952,
            SigId::SlhDsa128s => 32,
            SigId::FrostEd25519 => 32,
        }
    }

    pub fn sk_size(&self) -> usize {
        match self {
            SigId::Ed25519 => 32,
            SigId::MlDsa65 => 32,
            SigId::SlhDsa128s => 64,
            SigId::FrostEd25519 => 32,
        }
    }

    pub fn sig_size(&self) -> usize {
        match self {
            SigId::Ed25519 => 64,
            SigId::MlDsa65 => 3309,
            SigId::SlhDsa128s => 7856,
            SigId::FrostEd25519 => 64,
        }
    }

    pub fn all() -> &'static [SigId] {
        &[
            SigId::Ed25519,
            SigId::MlDsa65,
            SigId::SlhDsa128s,
            SigId::FrostEd25519,
        ]
    }
}

/// Digital signature scheme.
///
/// Sizes live on [`SigId`] (via `pk_size()` etc.) to keep this trait
/// dyn-compatible.
pub trait Sign: Send + Sync + 'static {
    fn id(&self) -> SigId;

    fn name(&self) -> &'static str {
        self.id().name()
    }

    /// Generate a fresh keypair. Returns `(pk, sk)`.
    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)>;

    /// Sign `msg` with `sk`. Returns signature bytes.
    fn sign(&self, sk: &[u8], msg: &[u8]) -> CryptoResult<Vec<u8>>;

    /// Verify `sig` over `msg` under `pk`.
    fn verify(&self, pk: &[u8], msg: &[u8], sig: &[u8]) -> CryptoResult<()>;
}

pub mod ed25519;
pub mod ml_dsa65;

pub use ed25519::Ed25519Signer;
pub use ml_dsa65::MlDsa65Signer;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn sig_id_name_matches() {
        assert_eq!(SigId::Ed25519.name(), "Ed25519");
        assert_eq!(SigId::MlDsa65.name(), "ML-DSA-65");
        assert_eq!(SigId::SlhDsa128s.name(), "SLH-DSA-128s");
    }

    #[test]
    fn sig_id_pq_flag() {
        assert!(!SigId::Ed25519.is_post_quantum());
        assert!(SigId::MlDsa65.is_post_quantum());
        assert!(SigId::SlhDsa128s.is_post_quantum());
    }

    #[test]
    fn sign_trait_is_object_safe() {
        let s: Arc<dyn Sign> = Arc::new(Ed25519Signer);
        assert_eq!(s.id(), SigId::Ed25519);
    }
}
