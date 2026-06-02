//! ML-DSA-65 (FIPS 204) implementation of the [`Sign`] trait.
//!
//! Default post-quantum signature for v0.2. NIST security level 3.
//! Sizes: pk=1952, sk=32 (seed, FIPS 204 allows the 32-byte seed as the
//! canonical sk), sig=3309.

use getrandom::SysRng;
use ml_dsa::{
    EncodedSignature, EncodedVerifyingKey, Generate, KeyExport, MlDsa65, Signature, SigningKey,
    VerifyingKey,
};
use rand_core::UnwrapErr;

use super::{Sign, SigId};
use crate::error::{CryptoError, CryptoResult};

pub struct MlDsa65Signer;

impl Sign for MlDsa65Signer {
    fn id(&self) -> SigId {
        SigId::MlDsa65
    }

    fn generate(&self) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
        let mut rng = UnwrapErr(SysRng);
        let sk = SigningKey::<MlDsa65>::generate_from_rng(&mut rng);
        let vk: &VerifyingKey<MlDsa65> = sk.as_ref();
        let pk_bytes = vk.encode().to_vec();
        let sk_bytes = sk.to_bytes();
        Ok((pk_bytes, sk_bytes.to_vec()))
    }

    fn sign(&self, sk: &[u8], msg: &[u8]) -> CryptoResult<Vec<u8>> {
        check_len("ml-dsa-65 sk", SigId::MlDsa65.sk_size(), sk.len())?;
        let sk_arr: &ml_dsa::Seed = sk
            .try_into()
            .map_err(|_| CryptoError::Sign("ml-dsa-65 sk seed".into()))?;
        let signing = SigningKey::<MlDsa65>::from_seed(sk_arr);
        let sig: Signature<MlDsa65> = signing
            .expanded_key()
            .sign_deterministic(msg, b"")
            .map_err(|e| CryptoError::Sign(format!("ml-dsa-65 sign: {e}")))?;
        Ok(sig.encode().to_vec())
    }

    fn verify(&self, pk: &[u8], msg: &[u8], sig: &[u8]) -> CryptoResult<()> {
        check_len("ml-dsa-65 pk", SigId::MlDsa65.pk_size(), pk.len())?;
        check_len("ml-dsa-65 sig", SigId::MlDsa65.sig_size(), sig.len())?;
        let pk_arr: EncodedVerifyingKey<MlDsa65> = pk
            .try_into()
            .map_err(|_| CryptoError::Sign("ml-dsa-65 pk array".into()))?;
        let vk = VerifyingKey::<MlDsa65>::decode(&pk_arr);
        let sig_arr: EncodedSignature<MlDsa65> = sig
            .try_into()
            .map_err(|_| CryptoError::Sign("ml-dsa-65 sig array".into()))?;
        let sig_obj = Signature::<MlDsa65>::decode(&sig_arr).ok_or(CryptoError::Verify)?;
        if vk.verify_with_context(msg, b"", &sig_obj) {
            Ok(())
        } else {
            Err(CryptoError::Verify)
        }
    }
}

fn check_len(name: &str, expected: usize, actual: usize) -> CryptoResult<()> {
    if actual != expected {
        Err(CryptoError::InvalidLength { expected, actual })
    } else {
        let _ = name;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml_dsa65_sizes_match_constants() {
        assert_eq!(SigId::MlDsa65.pk_size(), 1952);
        assert_eq!(SigId::MlDsa65.sk_size(), 32);
        assert_eq!(SigId::MlDsa65.sig_size(), 3309);
    }

    #[test]
    fn ml_dsa65_id_matches() {
        let s = MlDsa65Signer;
        assert_eq!(s.id(), SigId::MlDsa65);
    }

    #[test]
    fn ml_dsa65_generate_lengths() {
        let (pk, sk) = MlDsa65Signer.generate().expect("generate");
        assert_eq!(pk.len(), 1952);
        assert_eq!(sk.len(), 32);
    }

    #[test]
    fn ml_dsa65_sign_verify_roundtrip() {
        let (pk, sk) = MlDsa65Signer.generate().expect("generate");
        let msg = b"tos v0.2 d2d post-quantum";
        let sig = MlDsa65Signer.sign(&sk, msg).expect("sign");
        assert_eq!(sig.len(), 3309);
        MlDsa65Signer.verify(&pk, msg, &sig).expect("verify");
    }

    #[test]
    fn ml_dsa65_verify_rejects_tampered_message() {
        let (pk, sk) = MlDsa65Signer.generate().expect("generate");
        let sig = MlDsa65Signer.sign(&sk, b"original").expect("sign");
        let result = MlDsa65Signer.verify(&pk, b"tampered", &sig);
        assert!(result.is_err());
    }

    #[test]
    fn ml_dsa65_verify_rejects_bad_signature() {
        let (pk, _sk) = MlDsa65Signer.generate().expect("generate");
        let mut bad_sig = vec![0u8; 3309];
        bad_sig[0] = 1;
        let result = MlDsa65Signer.verify(&pk, b"msg", &bad_sig);
        assert!(result.is_err());
    }

    #[test]
    fn ml_dsa65_rejects_short_sk() {
        let result = MlDsa65Signer.sign(&[0u8; 16], b"msg");
        assert!(matches!(
            result,
            Err(CryptoError::InvalidLength { .. })
        ));
    }

    #[test]
    fn ml_dsa65_rejects_short_pk() {
        let result = MlDsa65Signer.verify(&[0u8; 100], b"msg", &[0u8; 3309]);
        assert!(matches!(
            result,
            Err(CryptoError::InvalidLength { .. })
        ));
    }
}
