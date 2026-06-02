//! Algorithm registry: the negotiated bundle of `Kex + Sign + Cipher` that two
//! peers agree on at session start (Hello/HelloAck handshake).
//!
//! v0.2 introduces two presets:
//! - `v0_2`:    X25519 + Ed25519 + ChaCha20-Poly1305 (default, broad compat)
//! - `v0_2_pqc`:X-Wing + ML-DSA-65 + ChaCha20-Poly1305 (hybrid post-quantum)

use serde::{Deserialize, Serialize};

use crate::cipher::{Cipher, CipherId, ChaCha20Poly1305Cipher, XChaCha20Poly1305Cipher};
use crate::kex::{Kex, KexId, MlKem768Kex, X25519Kex, XWingKex};
use crate::sig::{Ed25519Signer, Sign, SigId};

/// A bundle of algorithm identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlgorithmSet {
    pub kex: KexId,
    pub sig: SigId,
    pub cipher: CipherId,
}

impl AlgorithmSet {
    /// v0.2 default: X25519 + Ed25519 + ChaCha20-Poly1305.
    pub const fn v0_2() -> Self {
        Self {
            kex: KexId::X25519,
            sig: SigId::Ed25519,
            cipher: CipherId::ChaCha20Poly1305,
        }
    }

    /// v0.2 post-quantum: X-Wing + ML-DSA-65 + ChaCha20-Poly1305.
    /// `KexId::XWing` and `SigId::MlDsa65` need feature-gated impls;
    /// this constant is reserved for the v0.2 PQC rollout.
    pub const fn v0_2_pqc() -> Self {
        Self {
            kex: KexId::XWing,
            sig: SigId::MlDsa65,
            cipher: CipherId::ChaCha20Poly1305,
        }
    }

    pub fn is_post_quantum(&self) -> bool {
        self.kex.is_post_quantum() || self.sig.is_post_quantum()
    }

    pub fn cipher_impl(&self) -> Box<dyn Cipher> {
        match self.cipher {
            CipherId::ChaCha20Poly1305 => Box::new(ChaCha20Poly1305Cipher),
            CipherId::XChaCha20Poly1305 => Box::new(XChaCha20Poly1305Cipher),
            CipherId::None => Box::new(crate::cipher::NoneCipher),
            other => panic!("cipher {:?} not yet implemented in v0.2 Phase 1", other),
        }
    }

    pub fn kex_impl(&self) -> Box<dyn Kex> {
        match self.kex {
            KexId::X25519 => Box::new(X25519Kex),
            KexId::MlKem768 => Box::new(MlKem768Kex),
            KexId::XWing => Box::new(XWingKex),
            other => panic!("kex {:?} not yet implemented in v0.2 Phase 2", other),
        }
    }

    pub fn sign_impl(&self) -> Box<dyn Sign> {
        match self.sig {
            SigId::Ed25519 => Box::new(Ed25519Signer),
            other => panic!("sig {:?} not yet implemented in v0.2 Phase 1", other),
        }
    }
}

impl Default for AlgorithmSet {
    fn default() -> Self {
        Self::v0_2()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_v0_2_classical() {
        let s = AlgorithmSet::default();
        assert_eq!(s, AlgorithmSet::v0_2());
        assert!(!s.is_post_quantum());
    }

    #[test]
    fn v0_2_pqc_is_post_quantum() {
        let s = AlgorithmSet::v0_2_pqc();
        assert!(s.is_post_quantum());
        assert_eq!(s.kex, KexId::XWing);
        assert_eq!(s.sig, SigId::MlDsa65);
    }

    #[test]
    fn serde_roundtrip() {
        let s = AlgorithmSet::v0_2();
        let json = serde_json::to_string(&s).unwrap();
        let s2: AlgorithmSet = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn v0_2_pqc_serde() {
        let s = AlgorithmSet::v0_2_pqc();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("x-wing"));
        assert!(json.contains("ml-dsa65"));
        let s2: AlgorithmSet = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn factory_dispatch() {
        let s = AlgorithmSet::v0_2();
        assert_eq!(s.cipher_impl().id(), CipherId::ChaCha20Poly1305);
        assert_eq!(s.kex_impl().id(), KexId::X25519);
        assert_eq!(s.sign_impl().id(), SigId::Ed25519);
    }
}
