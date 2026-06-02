//! ToS v0.2 crypto layer.
//!
//! Trait-based abstraction over KEM, signature, and AEAD algorithms. The
//! v1.0 modules (identity, sign, exchange, encrypt, hash) remain as
//! ergonomic thin wrappers; v0.2 adds:
//!
//! - [`kex::Kex`]  : uniform KEM/DH interface (X25519, ML-KEM, X-Wing)
//! - [`sig::Sign`] : uniform signature interface (Ed25519, ML-DSA, SLH-DSA)
//! - [`cipher::Cipher`] : uniform AEAD interface
//! - [`kdf`] : HKDF-SHA256
//! - [`registry::AlgorithmSet`] : negotiated algorithm bundle
//!
//! v1.0 callers are unaffected. v0.2 callers use the trait dispatch in
//! `registry::AlgorithmSet` so the algorithm set can be changed at runtime
//! without recompilation.

pub mod cipher;
pub mod error;
pub mod exchange;
pub mod hash;
pub mod identity;
pub mod kdf;
pub mod kex;
pub mod registry;
pub mod sign;
pub mod sig;

pub use cipher::{
    ChaCha20Poly1305Cipher, Cipher, CipherId, NoneCipher, XChaCha20Poly1305Cipher,
};
pub use error::{CryptoError, CryptoResult};
pub use exchange::{derive_session_key, EphemeralKeyPair, SHARED_SECRET_SIZE};
pub use hash::{blake3_derive_context, blake3_hash, blake3_keyed, verify_hash};
pub use identity::{Identity, NodeId, NODE_ID_SIZE};
pub use kdf::{hkdf_sha256, labels as kdf_labels};
pub use kex::{Kex, KexId, MlKem768Kex, X25519Kex, XWingKex};
pub use registry::AlgorithmSet;
pub use sign::{
    public_key_bytes, sign, verify, verifying_key_from_bytes, PUBLIC_KEY_SIZE, SIGNATURE_SIZE,
    SECRET_KEY_SIZE,
};
pub use sig::{Ed25519Signer, MlDsa65Signer, SigId, Sign};
