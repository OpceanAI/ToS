//! X25519 ECDH (RFC 7748) implementation of the [`Kex`] trait.
//!
//! Wraps `x25519-dalek` to provide a uniform key agreement interface. In v0.2
//! this serves as the classical half of the X-Wing hybrid; in v1.0 it is the
//! only KEX algorithm.

use rand::rngs::OsRng;
use rand::RngCore;
use x25519_dalek::{PublicKey, StaticSecret};

use super::{check_len, Kex, KexId};
use crate::error::CryptoResult;

/// X25519 ECDH key agreement.
///
/// Sizes per RFC 7748:
/// - Public key: 32 bytes
/// - Secret key: 32 bytes
/// - Ciphertext (ephemeral pk): 32 bytes
/// - Shared secret: 32 bytes
pub struct X25519Kex;

impl Kex for X25519Kex {
    fn id(&self) -> KexId {
        KexId::X25519
    }

    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        let mut sk_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut sk_bytes);
        let secret = StaticSecret::from(sk_bytes);
        let public = PublicKey::from(&secret);
        Ok((public.to_bytes().to_vec(), sk_bytes.to_vec()))
    }

    fn encapsulate(&self, remote_pk: &[u8]) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        check_len("x25519 remote_pk", KexId::X25519.pk_size(), remote_pk.len())?;
        let mut ephemeral = [0u8; 32];
        OsRng.fill_bytes(&mut ephemeral);
        let eph_secret = StaticSecret::from(ephemeral);
        let eph_public = PublicKey::from(&eph_secret);

        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(remote_pk);
        let remote = PublicKey::from(pk_arr);
        let shared = eph_secret.diffie_hellman(&remote);

        Ok((eph_public.to_bytes().to_vec(), shared.to_bytes().to_vec()))
    }

    fn decapsulate(&self, sk: &[u8], ct: &[u8]) -> CryptoResult<Vec<u8>> {
        check_len("x25519 sk", KexId::X25519.sk_size(), sk.len())?;
        check_len("x25519 ct", KexId::X25519.ct_size(), ct.len())?;
        let mut sk_arr = [0u8; 32];
        sk_arr.copy_from_slice(sk);
        let mut ct_arr = [0u8; 32];
        ct_arr.copy_from_slice(ct);

        let secret = StaticSecret::from(sk_arr);
        let ephemeral = PublicKey::from(ct_arr);
        let shared = secret.diffie_hellman(&ephemeral);
        Ok(shared.to_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_constants() {
        assert_eq!(KexId::X25519.pk_size(), 32);
        assert_eq!(KexId::X25519.sk_size(), 32);
        assert_eq!(KexId::X25519.ct_size(), 32);
        assert_eq!(KexId::X25519.ss_size(), 32);
    }

    #[test]
    fn roundtrip_symmetric() {
        let kex = X25519Kex;
        let (alice_pk, alice_sk) = kex.generate().unwrap();
        let (bob_pk, bob_sk) = kex.generate().unwrap();

        let (alice_ct, alice_ss) = kex.encapsulate(&bob_pk).unwrap();
        let bob_ss = kex.decapsulate(&bob_sk, &alice_ct).unwrap();
        assert_eq!(alice_ss, bob_ss);

        let (bob_ct, bob_ss) = kex.encapsulate(&alice_pk).unwrap();
        let alice_ss = kex.decapsulate(&alice_sk, &bob_ct).unwrap();
        assert_eq!(alice_ss, bob_ss);
    }

    #[test]
    fn encapsulate_uses_ephemeral_keypair() {
        let kex = X25519Kex;
        let (bob_pk, _) = kex.generate().unwrap();
        let (ct1, ss1) = kex.encapsulate(&bob_pk).unwrap();
        let (ct2, ss2) = kex.encapsulate(&bob_pk).unwrap();
        assert_ne!(ct1, ct2, "two encapsulations must yield different ciphertexts");
        assert_ne!(ss1, ss2, "fresh ephemeral keypairs must yield different secrets");
    }

    #[test]
    fn invalid_pk_length_errors() {
        let kex = X25519Kex;
        assert!(kex.encapsulate(&[0u8; 10]).is_err());
    }

    #[test]
    fn invalid_sk_length_errors() {
        let kex = X25519Kex;
        assert!(kex.decapsulate(&[0u8; 10], &[0u8; 32]).is_err());
    }

    #[test]
    fn invalid_ct_length_errors() {
        let kex = X25519Kex;
        assert!(kex.decapsulate(&[0u8; 32], &[0u8; 10]).is_err());
    }

    #[test]
    fn rfc_7748_vector_1() {
        let alice_sk = hex::decode("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
            .unwrap();
        let bob_pk = hex::decode("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
            .unwrap();
        let expected_ss =
            hex::decode("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742")
                .unwrap();

        let _kex = X25519Kex;
        let mut sk = [0u8; 32];
        sk.copy_from_slice(&alice_sk);
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&bob_pk);

        let secret = x25519_dalek::StaticSecret::from(sk);
        let remote = x25519_dalek::PublicKey::from(pk);
        let ss = secret.diffie_hellman(&remote);
        assert_eq!(ss.to_bytes().to_vec(), expected_ss);
    }
}
