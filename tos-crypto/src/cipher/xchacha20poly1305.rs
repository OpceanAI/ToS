//! XChaCha20-Poly1305 AEAD (draft-irtf-cfrg-xchacha) implementation of
//! the [`Cipher`] trait.
//!
//! 24-byte nonce allows random nonces without state management.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use super::{check_len, Cipher, CipherId};
use crate::error::{CryptoError, CryptoResult};

/// XChaCha20-Poly1305 (extended nonce variant).
/// - KEY_SIZE: 32 bytes
/// - NONCE_SIZE: 24 bytes (XChaCha20 nonce; safe for random generation)
/// - TAG_SIZE: 16 bytes
pub struct XChaCha20Poly1305Cipher;

impl Cipher for XChaCha20Poly1305Cipher {
    fn id(&self) -> CipherId {
        CipherId::XChaCha20Poly1305
    }

    fn encrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        aad: &[u8],
        pt: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        check_len("xchacha20poly1305 key", CipherId::XChaCha20Poly1305.key_size(), key.len())?;
        check_len("xchacha20poly1305 nonce", CipherId::XChaCha20Poly1305.nonce_size(), nonce.len())?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .encrypt(
                XNonce::from_slice(nonce),
                Payload { msg: pt, aad },
            )
            .map_err(|e| CryptoError::Encrypt(e.to_string()))
    }

    fn decrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        aad: &[u8],
        ct: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        check_len("xchacha20poly1305 key", CipherId::XChaCha20Poly1305.key_size(), key.len())?;
        check_len("xchacha20poly1305 nonce", CipherId::XChaCha20Poly1305.nonce_size(), nonce.len())?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload { msg: ct, aad },
            )
            .map_err(|e| CryptoError::Decrypt(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_constants() {
        assert_eq!(CipherId::XChaCha20Poly1305.key_size(), 32);
        assert_eq!(CipherId::XChaCha20Poly1305.nonce_size(), 24);
        assert_eq!(CipherId::XChaCha20Poly1305.tag_size(), 16);
    }

    #[test]
    fn roundtrip() {
        let c = XChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [3u8; 24];
        let aad = b"context";
        let pt = b"some payload to seal";

        let ct = c.encrypt(&key, &nonce, aad, pt).unwrap();
        assert_eq!(ct.len(), pt.len() + CipherId::XChaCha20Poly1305.tag_size());
        let pt2 = c.decrypt(&key, &nonce, aad, &ct).unwrap();
        assert_eq!(pt2, pt);
    }

    #[test]
    fn random_nonce_safe() {
        let c = XChaCha20Poly1305Cipher;
        let key = [9u8; 32];
        let pt = b"hello";
        let n1 = [1u8; 24];
        let n2 = [2u8; 24];
        let ct1 = c.encrypt(&key, &n1, &[], pt).unwrap();
        let ct2 = c.encrypt(&key, &n2, &[], pt).unwrap();
        assert_ne!(ct1, ct2);
        assert_eq!(c.decrypt(&key, &n1, &[], &ct1).unwrap(), pt);
        assert_eq!(c.decrypt(&key, &n2, &[], &ct2).unwrap(), pt);
    }

    #[test]
    fn aad_mismatch_fails() {
        let c = XChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [3u8; 24];
        let ct = c.encrypt(&key, &nonce, b"a", b"data").unwrap();
        assert!(c.decrypt(&key, &nonce, b"b", &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let c = XChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [3u8; 24];
        let mut ct = c.encrypt(&key, &nonce, &[], b"data").unwrap();
        ct[5] ^= 0x01;
        assert!(c.decrypt(&key, &nonce, &[], &ct).is_err());
    }

    #[test]
    fn key_length_error() {
        let c = XChaCha20Poly1305Cipher;
        assert!(c.encrypt(&[0u8; 16], &[0u8; 24], &[], b"x").is_err());
    }

    #[test]
    fn nonce_length_error() {
        let c = XChaCha20Poly1305Cipher;
        assert!(c.encrypt(&[0u8; 32], &[0u8; 12], &[], b"x").is_err());
    }
}
