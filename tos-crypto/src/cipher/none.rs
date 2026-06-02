//! `None` cipher — passthrough, no authentication. Debug only.

use super::{Cipher, CipherId};
use crate::error::CryptoResult;

/// Plaintext passthrough. `KEY_SIZE`, `NONCE_SIZE`, and `TAG_SIZE` are zero;
/// the cipher does no transformation.
pub struct NoneCipher;

impl Cipher for NoneCipher {
    fn id(&self) -> CipherId {
        CipherId::None
    }

    fn encrypt(
        &self,
        _key: &[u8],
        _nonce: &[u8],
        _aad: &[u8],
        pt: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        Ok(pt.to_vec())
    }

    fn decrypt(
        &self,
        _key: &[u8],
        _nonce: &[u8],
        _aad: &[u8],
        ct: &[u8],
    ) -> CryptoResult<Vec<u8>> {
        Ok(ct.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_roundtrip() {
        let c = NoneCipher;
        let pt = b"plaintext data, no protection";
        let ct = c.encrypt(&[], &[], &[], pt).unwrap();
        assert_eq!(ct, pt);
        let pt2 = c.decrypt(&[], &[], &[], &ct).unwrap();
        assert_eq!(pt2, pt);
    }

    #[test]
    fn sizes_are_zero() {
        assert_eq!(CipherId::None.key_size(), 0);
        assert_eq!(CipherId::None.nonce_size(), 0);
        assert_eq!(CipherId::None.tag_size(), 0);
    }

    #[test]
    fn tampering_is_not_detected() {
        let c = NoneCipher;
        let ct = c.encrypt(&[], &[], &[], b"original").unwrap();
        let mut tampered = ct.clone();
        tampered[0] ^= 0xFF;
        let pt = c.decrypt(&[], &[], &[], &tampered).unwrap();
        assert_ne!(pt, b"original");
    }
}
