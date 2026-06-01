use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::error::{CryptoError, CryptoResult};

pub const SIGNATURE_SIZE: usize = 64;
pub const PUBLIC_KEY_SIZE: usize = 32;
pub const SECRET_KEY_SIZE: usize = 32;

pub fn sign(signing_key: &SigningKey, message: &[u8]) -> [u8; SIGNATURE_SIZE] {
    let sig: Signature = signing_key.sign(message);
    sig.to_bytes()
}

pub fn verify(verifying_key: &VerifyingKey, message: &[u8], signature: &[u8]) -> CryptoResult<()> {
    if signature.len() != SIGNATURE_SIZE {
        return Err(CryptoError::InvalidLength {
            expected: SIGNATURE_SIZE,
            actual: signature.len(),
        });
    }
    let sig = Signature::from_slice(signature)
        .map_err(|_| CryptoError::InvalidLength {
            expected: SIGNATURE_SIZE,
            actual: signature.len(),
        })?;
    verifying_key
        .verify(message, &sig)
        .map_err(|_| CryptoError::Verify)
}

pub fn public_key_bytes(key: &VerifyingKey) -> [u8; PUBLIC_KEY_SIZE] {
    key.to_bytes()
}

pub fn verifying_key_from_bytes(bytes: &[u8]) -> CryptoResult<VerifyingKey> {
    if bytes.len() != PUBLIC_KEY_SIZE {
        return Err(CryptoError::InvalidLength {
            expected: PUBLIC_KEY_SIZE,
            actual: bytes.len(),
        });
    }
    let mut arr = [0u8; PUBLIC_KEY_SIZE];
    arr.copy_from_slice(bytes);
    VerifyingKey::from_bytes(&arr).map_err(|e| CryptoError::Sign(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn sign_verify_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let msg = b"tos protocol message";
        let sig = sign(&key, msg);
        let vk = key.verifying_key();
        assert!(verify(&vk, msg, &sig).is_ok());
    }

    #[test]
    fn tampered_message_fails() {
        let key = SigningKey::generate(&mut OsRng);
        let sig = sign(&key, b"original");
        let vk = key.verifying_key();
        assert!(verify(&vk, b"tampered", &sig).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let key_a = SigningKey::generate(&mut OsRng);
        let key_b = SigningKey::generate(&mut OsRng);
        let sig = sign(&key_a, b"msg");
        assert!(verify(&key_b.verifying_key(), b"msg", &sig).is_err());
    }

    #[test]
    fn invalid_signature_length() {
        let key = SigningKey::generate(&mut OsRng);
        let result = verify(&key.verifying_key(), b"msg", &[0u8; 10]);
        assert!(matches!(result, Err(CryptoError::InvalidLength { .. })));
    }

    #[test]
    fn verifying_key_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let bytes = public_key_bytes(&key.verifying_key());
        let parsed = verifying_key_from_bytes(&bytes).unwrap();
        assert_eq!(public_key_bytes(&parsed), bytes);
    }
}
