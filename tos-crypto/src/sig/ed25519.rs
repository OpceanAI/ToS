//! Ed25519 (RFC 8032) implementation of the [`Sign`] trait.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::{Sign, SigId};
use crate::error::{CryptoError, CryptoResult};

/// Ed25519 signature scheme.
pub struct Ed25519Signer;

impl Sign for Ed25519Signer {
    fn id(&self) -> SigId {
        SigId::Ed25519
    }

    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        let mut csprng = rand::rngs::OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let pk = sk.verifying_key();
        Ok((pk.to_bytes().to_vec(), sk.to_bytes().to_vec()))
    }

    fn sign(&self, sk: &[u8], msg: &[u8]) -> CryptoResult<Vec<u8>> {
        let sk_arr: [u8; 32] = sk
            .try_into()
            .map_err(|_| CryptoError::InvalidLength {
                expected: 32,
                actual: sk.len(),
            })?;
        let signing = SigningKey::from_bytes(&sk_arr);
        let sig = signing.sign(msg);
        Ok(sig.to_bytes().to_vec())
    }

    fn verify(&self, pk: &[u8], msg: &[u8], sig: &[u8]) -> CryptoResult<()> {
        let pk_arr: [u8; 32] = pk
            .try_into()
            .map_err(|_| CryptoError::InvalidLength {
                expected: 32,
                actual: pk.len(),
            })?;
        let sig_arr: [u8; 64] = sig
            .try_into()
            .map_err(|_| CryptoError::InvalidLength {
                expected: 64,
                actual: sig.len(),
            })?;
        let vk = VerifyingKey::from_bytes(&pk_arr)
            .map_err(|e| CryptoError::Sign(e.to_string()))?;
        let s = Signature::from_bytes(&sig_arr);
        vk.verify(msg, &s)
            .map_err(|_| CryptoError::Verify)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_constants() {
        assert_eq!(SigId::Ed25519.pk_size(), 32);
        assert_eq!(SigId::Ed25519.sk_size(), 32);
        assert_eq!(SigId::Ed25519.sig_size(), 64);
    }

    #[test]
    fn roundtrip() {
        let s = Ed25519Signer;
        let (pk, sk) = s.generate().unwrap();
        let msg = b"tos v0.2 hello";
        let sig = s.sign(&sk, msg).unwrap();
        assert_eq!(sig.len(), 64);
        s.verify(&pk, msg, &sig).unwrap();
    }

    #[test]
    fn tampered_message_fails() {
        let s = Ed25519Signer;
        let (pk, sk) = s.generate().unwrap();
        let sig = s.sign(&sk, b"original").unwrap();
        assert!(s.verify(&pk, b"tampered", &sig).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let s = Ed25519Signer;
        let (_pk1, sk1) = s.generate().unwrap();
        let (pk2, _sk2) = s.generate().unwrap();
        let sig = s.sign(&sk1, b"x").unwrap();
        assert!(s.verify(&pk2, b"x", &sig).is_err());
    }

    #[test]
    fn sk_length_error() {
        let s = Ed25519Signer;
        assert!(s.sign(&[0u8; 10], b"x").is_err());
    }

    #[test]
    fn pk_length_error() {
        let s = Ed25519Signer;
        assert!(s.verify(&[0u8; 10], b"x", &[0u8; 64]).is_err());
    }

    #[test]
    fn sig_length_error() {
        let s = Ed25519Signer;
        let (pk, _) = s.generate().unwrap();
        assert!(s.verify(&pk, b"x", &[0u8; 32]).is_err());
    }

    #[test]
    fn rfc_8032_vector_1() {
        let s = Ed25519Signer;
        let sk = hex::decode("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
            .unwrap();
        let pk = hex::decode("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
            .unwrap();
        let msg: &[u8] = b"";
        let expected_sig = hex::decode("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b")
            .unwrap();
        let sig = s.sign(&sk, msg).unwrap();
        assert_eq!(sig, expected_sig);
        s.verify(&pk, msg, &sig).unwrap();
    }
}
