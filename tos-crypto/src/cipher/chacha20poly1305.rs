//! ChaCha20-Poly1305 AEAD (RFC 8439) implementation of the [`Cipher`] trait.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use super::{check_len, Cipher, CipherId};
use crate::error::{CryptoError, CryptoResult};

/// ChaCha20-Poly1305 (RFC 8439).
/// - KEY_SIZE: 32 bytes
/// - NONCE_SIZE: 12 bytes (96-bit IETF nonce)
/// - TAG_SIZE: 16 bytes (Poly1305 authentication tag)
pub struct ChaCha20Poly1305Cipher;

impl Cipher for ChaCha20Poly1305Cipher {
    fn id(&self) -> CipherId {
        CipherId::ChaCha20Poly1305
    }

    fn encrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        aad: &[u8],
        pt: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        check_len("chacha20poly1305 key", CipherId::ChaCha20Poly1305.key_size(), key.len())?;
        check_len("chacha20poly1305 nonce", CipherId::ChaCha20Poly1305.nonce_size(), nonce.len())?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .encrypt(
                Nonce::from_slice(nonce),
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
        check_len("chacha20poly1305 key", CipherId::ChaCha20Poly1305.key_size(), key.len())?;
        check_len("chacha20poly1305 nonce", CipherId::ChaCha20Poly1305.nonce_size(), nonce.len())?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
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
        assert_eq!(CipherId::ChaCha20Poly1305.key_size(), 32);
        assert_eq!(CipherId::ChaCha20Poly1305.nonce_size(), 12);
        assert_eq!(CipherId::ChaCha20Poly1305.tag_size(), 16);
    }

    #[test]
    fn roundtrip() {
        let c = ChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [1u8; 12];
        let aad = b"tos-stream-batch-42";
        let pt = b"the quick brown fox jumps over the lazy dog";

        let ct = c.encrypt(&key, &nonce, aad, pt).unwrap();
        assert_eq!(ct.len(), pt.len() + CipherId::ChaCha20Poly1305.tag_size());
        let pt2 = c.decrypt(&key, &nonce, aad, &ct).unwrap();
        assert_eq!(pt2, pt);
    }

    #[test]
    fn aad_mismatch_fails() {
        let c = ChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [1u8; 12];
        let ct = c.encrypt(&key, &nonce, b"context-a", b"hello").unwrap();
        assert!(c.decrypt(&key, &nonce, b"context-b", &ct).is_err());
    }

    #[test]
    fn nonce_mismatch_fails() {
        let c = ChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let ct = c.encrypt(&key, &[1u8; 12], &[], b"hello").unwrap();
        assert!(c.decrypt(&key, &[2u8; 12], &[], &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let c = ChaCha20Poly1305Cipher;
        let key = [7u8; 32];
        let nonce = [1u8; 12];
        let mut ct = c.encrypt(&key, &nonce, &[], b"hello").unwrap();
        ct[0] ^= 0xFF;
        assert!(c.decrypt(&key, &nonce, &[], &ct).is_err());
    }

    #[test]
    fn key_length_error() {
        let c = ChaCha20Poly1305Cipher;
        assert!(c.encrypt(&[0u8; 10], &[0u8; 12], &[], b"x").is_err());
    }

    #[test]
    fn nonce_length_error() {
        let c = ChaCha20Poly1305Cipher;
        assert!(c.encrypt(&[0u8; 32], &[0u8; 5], &[], b"x").is_err());
    }

    #[test]
    fn rfc_8439_vector_1_encrypt_decrypt_roundtrip() {
        let c = ChaCha20Poly1305Cipher;
        let key = [
            0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
            0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
            0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
            0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
        ];
        let nonce = [
            0x07, 0x00, 0x00, 0x00, 0x40, 0x41, 0x42, 0x43,
            0x44, 0x45, 0x46, 0x47,
        ];
        let aad = [
            0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3,
            0xc4, 0xc5, 0xc6, 0xc7,
        ];
        let pt = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let ct = c.encrypt(&key, &nonce, &aad, pt).unwrap();
        let pt2 = c.decrypt(&key, &nonce, &aad, &ct).unwrap();
        assert_eq!(pt2, pt);
    }
}
