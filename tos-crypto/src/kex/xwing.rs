//! X-Wing KEM (hybrid X25519 + ML-KEM-768) implementation of the [`Kex`] trait.
//!
//! Implementation of X-Wing (draft-irtf-cfrg-xwing-kem-06) directly on top
//! of `ml-kem` 0.3 and `x25519-dalek` 2.0. The `x-wing` crate cannot be used
//! here because its pinned `ml-kem` 0.3.0-rc.0 uses an incompatible
//! `crypto-common` version than our ml-kem 0.3.2. This implementation
//! follows the spec verbatim.
//!
//! Layout:
//! - decapsulation key (`sk`): 32 bytes (random seed)
//! - encapsulation key (`pk`): 1216 bytes = ML-KEM-768 pk (1184) || X25519 pk (32)
//! - ciphertext: 1120 bytes = ML-KEM-768 ct (1088) || X25519 ephemeral pk (32)
//! - shared secret: 32 bytes from SHA3-256 combiner

use getrandom::SysRng;
use ml_kem::{
    array::Array,
    kem::{Decapsulate as _, KeyExport as _},
    DecapsulationKey, EncapsulationKey, MlKem768,
};
use rand_core::UnwrapErr;
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::{Digest, Sha3_256, Shake256};
use x25519_dalek::{PublicKey, StaticSecret};

use super::{check_len, Kex, KexId};
use crate::error::CryptoResult;

const XWING_LABEL: &[u8; 6] = b"\\.//^\\";
const SK_SIZE: usize = 32;
const X25519_PK_SIZE: usize = 32;
const MLKEM_PK_SIZE: usize = 1184;
const MLKEM_CT_SIZE: usize = 1088;
const X25519_CT_SIZE: usize = 32;
const SS_SIZE: usize = 32;

pub struct XWingKex;

fn expand_key(
    sk: &[u8; SK_SIZE],
) -> (
    DecapsulationKey<MlKem768>,
    StaticSecret,
    EncapsulationKey<MlKem768>,
    PublicKey,
) {
    let mut shaker = Shake256::default();
    shaker.update(sk);
    let mut expanded = shaker.finalize_xof();
    let mut seed_bytes = [0u8; 64];
    expanded.read(&mut seed_bytes);
    let seed: Array<u8, _> = seed_bytes.into();
    let sk_m = DecapsulationKey::<MlKem768>::from_seed(seed);
    let pk_m = sk_m.encapsulation_key().clone();

    let mut x_bytes = [0u8; 32];
    expanded.read(&mut x_bytes);
    let sk_x = StaticSecret::from(x_bytes);
    let pk_x = PublicKey::from(&sk_x);

    (sk_m, sk_x, pk_m, pk_x)
}

impl Kex for XWingKex {
    fn id(&self) -> KexId {
        KexId::XWing
    }

    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        let mut rng = UnwrapErr(SysRng);
        let mut sk_seed = [0u8; SK_SIZE];
        getrandom::fill(&mut sk_seed)
            .map_err(|e| crate::error::CryptoError::Keygen(format!("x-wing seed: {e}")))?;
        let _ = &mut rng;
        let (_, _sk_x, pk_m, pk_x) = expand_key(&sk_seed);

        let mut pk = Vec::with_capacity(XWingKex.id().pk_size());
        let pk_m_bytes = pk_m.to_bytes();
        pk.extend_from_slice(&pk_m_bytes);
        pk.extend_from_slice(pk_x.as_bytes());

        Ok((pk, sk_seed.to_vec()))
    }

    fn encapsulate(&self, remote_pk: &[u8]) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        check_len("x-wing pk", KexId::XWing.pk_size(), remote_pk.len())?;
        let (m_pk_bytes, x_pk_bytes) = remote_pk.split_at(MLKEM_PK_SIZE);
        let m_pk_arr: [u8; MLKEM_PK_SIZE] = m_pk_bytes
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("x-wing ml-kem pk len".into()))?;
        let m_ek_bytes: Array<u8, _> = m_pk_arr.into();
        let pk_m = EncapsulationKey::<MlKem768>::new(&m_ek_bytes)
            .map_err(|_| crate::error::CryptoError::Keygen("x-wing ml-kem pk invalid".into()))?;

        let mut x_remote = [0u8; X25519_PK_SIZE];
        x_remote.copy_from_slice(x_pk_bytes);
        let pk_x = PublicKey::from(x_remote);

        let mut m_random = [0u8; 32];
        getrandom::fill(&mut m_random)
            .map_err(|e| crate::error::CryptoError::Keygen(format!("x-wing ml-kem rnd: {e}")))?;
        let m_random_arr: Array<u8, _> = m_random.into();
        let (ct_m, ss_m) = pk_m.encapsulate_deterministic(&m_random_arr);

        let mut x_random = [0u8; 32];
        getrandom::fill(&mut x_random)
            .map_err(|e| crate::error::CryptoError::Keygen(format!("x-wing x25519 rnd: {e}")))?;
        let eph_secret = StaticSecret::from(x_random);
        let ct_x = PublicKey::from(&eph_secret);
        let ss_x = eph_secret.diffie_hellman(&pk_x);

        let ss_m_std: &[u8; 32] = (&ss_m).into();
        let ss = combiner(ss_m_std, &ss_x, &ct_x, &pk_x);
        let mut ct = Vec::with_capacity(KexId::XWing.ct_size());
        ct.extend_from_slice(ct_m.as_slice());
        ct.extend_from_slice(ct_x.as_bytes());

        Ok((ct, ss.to_vec()))
    }

    fn decapsulate(&self, sk: &[u8], ct: &[u8]) -> CryptoResult<Vec<u8>> {
        check_len("x-wing sk", KexId::XWing.sk_size(), sk.len())?;
        check_len("x-wing ct", KexId::XWing.ct_size(), ct.len())?;
        let sk_arr: [u8; SK_SIZE] = sk
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("x-wing sk len".into()))?;
        let (sk_m, sk_x, _pk_m, pk_x) = expand_key(&sk_arr);

        let (ct_m_bytes, ct_x_bytes) = ct.split_at(MLKEM_CT_SIZE);
        let ct_m_arr: [u8; MLKEM_CT_SIZE] = ct_m_bytes
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("x-wing ct m len".into()))?;
        let ct_m_obj: ml_kem::Ciphertext<MlKem768> = ct_m_arr.into();
        let ss_m = sk_m.decapsulate(&ct_m_obj);

        let ct_x_arr: [u8; X25519_CT_SIZE] = ct_x_bytes
            .try_into()
            .map_err(|_| crate::error::CryptoError::Keygen("x-wing ct x len".into()))?;
        let ct_x = PublicKey::from(ct_x_arr);
        let ss_x = sk_x.diffie_hellman(&ct_x);

        let ss_m_std: &[u8; 32] = (&ss_m).into();
        let ss = combiner(ss_m_std, &ss_x, &ct_x, &pk_x);
        Ok(ss.to_vec())
    }
}

fn combiner(
    ss_m_bytes: &[u8; 32],
    ss_x: &x25519_dalek::SharedSecret,
    ct_x: &PublicKey,
    pk_x: &PublicKey,
) -> [u8; SS_SIZE] {
    let mut hasher = Sha3_256::new();
    sha3::digest::Update::update(&mut hasher, ss_m_bytes);
    sha3::digest::Update::update(&mut hasher, ss_x.as_bytes());
    sha3::digest::Update::update(&mut hasher, ct_x.as_bytes());
    sha3::digest::Update::update(&mut hasher, pk_x.as_bytes());
    sha3::digest::Update::update(&mut hasher, XWING_LABEL);
    let out = hasher.finalize();
    let mut arr = [0u8; SS_SIZE];
    arr.copy_from_slice(&out);
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_constants() {
        assert_eq!(KexId::XWing.pk_size(), 1216);
        assert_eq!(KexId::XWing.sk_size(), 32);
        assert_eq!(KexId::XWing.ct_size(), 1120);
        assert_eq!(KexId::XWing.ss_size(), 32);
    }

    #[test]
    fn roundtrip_symmetric() {
        let kex = XWingKex;
        let (alice_pk, alice_sk) = kex.generate().unwrap();
        let (bob_pk, bob_sk) = kex.generate().unwrap();

        let (ct_alice, alice_ss) = kex.encapsulate(&bob_pk).unwrap();
        let bob_ss = kex.decapsulate(&bob_sk, &ct_alice).unwrap();
        assert_eq!(alice_ss, bob_ss);

        let (ct_bob, bob_ss2) = kex.encapsulate(&alice_pk).unwrap();
        let alice_ss2 = kex.decapsulate(&alice_sk, &ct_bob).unwrap();
        assert_eq!(bob_ss2, alice_ss2);
    }

    #[test]
    fn ss_size_is_32() {
        let kex = XWingKex;
        let (bob_pk, _) = kex.generate().unwrap();
        let (_ct, ss) = kex.encapsulate(&bob_pk).unwrap();
        assert_eq!(ss.len(), 32);
    }

    #[test]
    fn ct_size_is_1120() {
        let kex = XWingKex;
        let (bob_pk, _) = kex.generate().unwrap();
        let (ct, _) = kex.encapsulate(&bob_pk).unwrap();
        assert_eq!(ct.len(), 1120);
    }

    #[test]
    fn invalid_pk_length_errors() {
        let kex = XWingKex;
        assert!(kex.encapsulate(&[0u8; 100]).is_err());
    }

    #[test]
    fn invalid_sk_length_errors() {
        let kex = XWingKex;
        assert!(kex.decapsulate(&[0u8; 10], &[0u8; 1120]).is_err());
    }

    #[test]
    fn invalid_ct_length_errors() {
        let kex = XWingKex;
        assert!(kex.decapsulate(&[0u8; 32], &[0u8; 100]).is_err());
    }
}
