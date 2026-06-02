//! ML-KEM-768 (FIPS 203) implementation of the [`Kex`] trait.
//!
//! Default post-quantum KEM for v0.2. NIST security level 3.
//! Sizes: pk=1184, sk(seed)=64, ct=1088, ss=32.

use getrandom::SysRng;
use ml_kem::{
    array::Array,
    kem::{Decapsulate as _, Encapsulate as _, KeyExport as _},
    DecapsulationKey, EncapsulationKey, Kem, MlKem768,
};
use rand_core::UnwrapErr;

use super::{check_len, Kex, KexId};
use crate::error::CryptoResult;

pub struct MlKem768Kex;

impl Kex for MlKem768Kex {
    fn id(&self) -> KexId {
        KexId::MlKem768
    }

    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        let mut rng = UnwrapErr(SysRng);
        let (dk, ek) = <MlKem768 as Kem>::generate_keypair_from_rng(&mut rng);
        let ek_bytes = ek.to_bytes();
        let seed = dk
            .to_seed()
            .ok_or_else(|| crate::error::CryptoError::Keygen("ml-kem-768 to_seed".into()))?;
        Ok((ek_bytes.to_vec(), seed.to_vec()))
    }

    fn encapsulate(&self, remote_pk: &[u8]) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        check_len("ml-kem-768 pk", KexId::MlKem768.pk_size(), remote_pk.len())?;
        let pk_arr: [u8; 1184] = remote_pk
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("ml-kem-768 pk len".into()))?;
        let ek_arr: Array<u8, _> = pk_arr.into();
        let ek = EncapsulationKey::<MlKem768>::new(&ek_arr)
            .map_err(|_| crate::error::CryptoError::Keygen("ml-kem-768 pk invalid".into()))?;
        let mut rng = UnwrapErr(SysRng);
        let (ct, ss) = ek.encapsulate_with_rng(&mut rng);
        Ok((ct.to_vec(), ss.to_vec()))
    }

    fn decapsulate(&self, sk: &[u8], ct: &[u8]) -> CryptoResult<Vec<u8>> {
        check_len("ml-kem-768 sk", KexId::MlKem768.sk_size(), sk.len())?;
        check_len("ml-kem-768 ct", KexId::MlKem768.ct_size(), ct.len())?;
        let seed_arr: [u8; 64] = sk
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("ml-kem-768 sk len".into()))?;
        let seed: Array<u8, _> = seed_arr.into();
        let dk = DecapsulationKey::<MlKem768>::from_seed(seed);
        let ct_arr: [u8; 1088] = ct
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("ml-kem-768 ct len".into()))?;
        let ct_obj: ml_kem::Ciphertext<MlKem768> = ct_arr.into();
        let ss = dk.decapsulate(&ct_obj);
        Ok(ss.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_constants() {
        assert_eq!(KexId::MlKem768.pk_size(), 1184);
        assert_eq!(KexId::MlKem768.sk_size(), 64);
        assert_eq!(KexId::MlKem768.ct_size(), 1088);
        assert_eq!(KexId::MlKem768.ss_size(), 32);
    }

    #[test]
    fn roundtrip_symmetric() {
        let kex = MlKem768Kex;
        let (_alice_pk, alice_sk) = kex.generate().unwrap();
        let (bob_pk, bob_sk) = kex.generate().unwrap();

        let (ct_alice, alice_ss) = kex.encapsulate(&bob_pk).unwrap();
        let bob_ss = kex.decapsulate(&bob_sk, &ct_alice).unwrap();
        assert_eq!(alice_ss, bob_ss);

        let (ct_bob, bob_ss2) = kex.encapsulate(&_alice_pk).unwrap();
        let alice_ss2 = kex.decapsulate(&alice_sk, &ct_bob).unwrap();
        assert_eq!(bob_ss2, alice_ss2);
    }

    #[test]
    fn encapsulation_uses_fresh_randomness() {
        let kex = MlKem768Kex;
        let (bob_pk, _) = kex.generate().unwrap();
        let (ct1, ss1) = kex.encapsulate(&bob_pk).unwrap();
        let (ct2, ss2) = kex.encapsulate(&bob_pk).unwrap();
        assert_ne!(ct1, ct2);
        assert_ne!(ss1, ss2);
    }

    #[test]
    fn invalid_pk_length_errors() {
        let kex = MlKem768Kex;
        assert!(kex.encapsulate(&[0u8; 100]).is_err());
    }

    #[test]
    fn invalid_sk_length_errors() {
        let kex = MlKem768Kex;
        assert!(kex.decapsulate(&[0u8; 10], &[0u8; 1088]).is_err());
    }

    #[test]
    fn invalid_ct_length_errors() {
        let kex = MlKem768Kex;
        assert!(kex.decapsulate(&[0u8; 64], &[0u8; 100]).is_err());
    }
}
