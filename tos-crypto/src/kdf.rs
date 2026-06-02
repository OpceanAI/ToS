//! HKDF-SHA256 (RFC 5869) key derivation.
//!
//! Used to derive session keys from the raw shared secret produced by a KEM,
//! e.g. X25519 ECDH, X-Wing hybrid, or ML-KEM. Inputs:
//! - `ikm` (input keying material): typically the KEM shared secret
//! - `salt`: optional; defaults to zero-length
//! - `info`: domain-separation context (e.g. "tos/v0.2/stream-init")
//! - `length`: output length in bytes (e.g. 32 for a single ChaCha20 key)
//!
//! Example: derive both a ChaCha20 key and a nonce seed in one call:
//! ```ignore
//! let okm = hkdf_sha256(&shared_secret, Some(b"tos"), b"session-42", 64)?;
//! let key  = &okm[0..32];
//! let seed = &okm[32..64];
//! ```

use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{CryptoError, CryptoResult};

/// HKDF-SHA256. Returns `okm` of the requested length.
pub fn hkdf_sha256(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
    length: usize,
) -> CryptoResult<Vec<u8>> {
    if length == 0 {
        return Err(CryptoError::InvalidLength {
            expected: 1,
            actual: 0,
        });
    }
    if length > 255 * 32 {
        return Err(CryptoError::InvalidLength {
            expected: u16::MAX as usize,
            actual: length,
        });
    }
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut okm = vec![0u8; length];
    hk.expand(info, &mut okm)
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;
    Ok(okm)
}

/// Domain-separation labels used across ToS v0.2.
pub mod labels {
    pub const SESSION_STREAM: &[u8] = b"tos/v0.2/session/stream";
    pub const SESSION_BATCH: &[u8] = b"tos/v0.2/session/batch";
    pub const AUTH_PROOF: &[u8] = b"tos/v0.2/auth/proof";
    pub const KEY_WRAP: &[u8] = b"tos/v0.2/key/wrap";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_5869_test_vector_1() {
        let ikm = hex::decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b").unwrap();
        let salt = hex::decode("000102030405060708090a0b0c").unwrap();
        let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap();
        let expected = hex::decode(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        )
        .unwrap();
        let okm = hkdf_sha256(&ikm, Some(&salt), &info, 42).unwrap();
        assert_eq!(okm, expected);
    }

    #[test]
    fn rfc_5869_test_vector_3_no_salt() {
        let ikm = hex::decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b").unwrap();
        let info: &[u8] = b"";
        let expected = hex::decode(
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8",
        )
        .unwrap();
        let okm = hkdf_sha256(&ikm, None, info, 42).unwrap();
        assert_eq!(okm, expected);
    }

    #[test]
    fn zero_length_errors() {
        assert!(hkdf_sha256(b"ikm", None, b"info", 0).is_err());
    }

    #[test]
    fn derive_two_keys() {
        let ikm = b"shared-secret";
        let okm = hkdf_sha256(ikm, Some(b"tos"), b"multi-key", 64).unwrap();
        assert_eq!(okm.len(), 64);
        let k1 = &okm[0..32];
        let k2 = &okm[32..64];
        assert_ne!(k1, k2);
    }

    #[test]
    fn deterministic() {
        let okm1 = hkdf_sha256(b"key", Some(b"salt"), b"ctx", 32).unwrap();
        let okm2 = hkdf_sha256(b"key", Some(b"salt"), b"ctx", 32).unwrap();
        assert_eq!(okm1, okm2);
    }

    #[test]
    fn labels_are_distinct() {
        let ikm = b"ik";
        let a = hkdf_sha256(ikm, Some(b"s"), labels::SESSION_STREAM, 32).unwrap();
        let b = hkdf_sha256(ikm, Some(b"s"), labels::SESSION_BATCH, 32).unwrap();
        assert_ne!(a, b);
    }
}
