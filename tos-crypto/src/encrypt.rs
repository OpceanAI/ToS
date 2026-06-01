use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::error::{CryptoError, CryptoResult};

pub const KEY_SIZE: usize = 32;
pub const NONCE_SIZE: usize = 12;
pub const XNONCE_SIZE: usize = 24;
pub const TAG_SIZE: usize = 16;

pub fn encrypt(key: &[u8; KEY_SIZE], plaintext: &[u8], aad: &[u8]) -> CryptoResult<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;
    let mut out = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; KEY_SIZE], blob: &[u8], aad: &[u8]) -> CryptoResult<Vec<u8>> {
    if blob.len() < NONCE_SIZE + TAG_SIZE {
        return Err(CryptoError::InvalidLength {
            expected: NONCE_SIZE + TAG_SIZE,
            actual: blob.len(),
        });
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_SIZE);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Decrypt(e.to_string()))
}

pub fn encrypt_x(key: &[u8; KEY_SIZE], plaintext: &[u8], aad: &[u8]) -> CryptoResult<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce_bytes = [0u8; XNONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;
    let mut out = Vec::with_capacity(XNONCE_SIZE + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt_x(key: &[u8; KEY_SIZE], blob: &[u8], aad: &[u8]) -> CryptoResult<Vec<u8>> {
    if blob.len() < XNONCE_SIZE + TAG_SIZE {
        return Err(CryptoError::InvalidLength {
            expected: XNONCE_SIZE + TAG_SIZE,
            actual: blob.len(),
        });
    }
    let (nonce_bytes, ciphertext) = blob.split_at(XNONCE_SIZE);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Decrypt(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [7u8; 32];
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let aad = b"tos-stream-batch-42";
        let ct = encrypt(&key, plaintext, aad).unwrap();
        let pt = decrypt(&key, &ct, aad).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn different_aad_fails() {
        let key = [7u8; 32];
        let plaintext = b"hello";
        let ct = encrypt(&key, plaintext, b"context-a").unwrap();
        let result = decrypt(&key, &ct, b"context-b");
        assert!(result.is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [7u8; 32];
        let plaintext = b"hello";
        let mut ct = encrypt(&key, plaintext, b"").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0xFF;
        let result = decrypt(&key, &ct, b"");
        assert!(result.is_err());
    }

    #[test]
    fn xchacha_roundtrip() {
        let key = [9u8; 32];
        let plaintext = b"long-lived session key data";
        let ct = encrypt_x(&key, plaintext, b"aad").unwrap();
        let pt = decrypt_x(&key, &ct, b"aad").unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn short_ciphertext_errors() {
        let key = [9u8; 32];
        let result = decrypt(&key, &[0u8; 5], b"");
        assert!(result.is_err());
    }

    #[test]
    fn unique_nonces() {
        let key = [1u8; 32];
        let pt = b"same plaintext";
        let c1 = encrypt(&key, pt, b"").unwrap();
        let c2 = encrypt(&key, pt, b"").unwrap();
        assert_ne!(c1, c2);
    }
}
