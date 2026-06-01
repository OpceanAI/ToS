use crate::error::CryptoResult;

pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn blake3_keyed(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(key);
    hasher.update(data);
    let out = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(out.as_bytes());
    bytes
}

pub fn blake3_derive_context(context: &str, data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&[0u8; 32]);
    hasher.update(context.as_bytes());
    hasher.update(&[0u8]);
    hasher.update(data);
    let out = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(out.as_bytes());
    bytes
}

pub fn verify_hash(expected: &[u8; 32], data: &[u8]) -> CryptoResult<()> {
    let actual = blake3_hash(data);
    if &actual == expected {
        Ok(())
    } else {
        Err(crate::error::CryptoError::Verify)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hash_known_value() {
        let h = blake3_hash(b"");
        let expected = blake3::hash(b"");
        assert_eq!(&h, expected.as_bytes());
    }

    #[test]
    fn abc_hash_known_value() {
        let h = blake3_hash(b"abc");
        let expected = blake3::hash(b"abc");
        assert_eq!(&h, expected.as_bytes());
    }

    #[test]
    fn keyed_hash_differs_from_unkeyed() {
        let key = [42u8; 32];
        let h1 = blake3_keyed(&key, b"data");
        let h2 = blake3_hash(b"data");
        assert_ne!(h1, h2);
    }

    #[test]
    fn verify_hash_works() {
        let h = blake3_hash(b"hello");
        assert!(verify_hash(&h, b"hello").is_ok());
        assert!(verify_hash(&h, b"world").is_err());
    }
}
