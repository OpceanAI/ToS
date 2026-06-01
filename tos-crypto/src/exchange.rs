use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{CryptoError, CryptoResult};

pub const SHARED_SECRET_SIZE: usize = 32;

pub struct EphemeralKeyPair {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

impl EphemeralKeyPair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }
}

pub fn derive_session_key(
    my_secret: &StaticSecret,
    their_public_bytes: &[u8],
) -> CryptoResult<[u8; SHARED_SECRET_SIZE]> {
    if their_public_bytes.len() != 32 {
        return Err(CryptoError::InvalidLength {
            expected: 32,
            actual: their_public_bytes.len(),
        });
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(their_public_bytes);
    let their_public = PublicKey::from(arr);
    let shared = my_secret.diffie_hellman(&their_public);
    Ok(shared.to_bytes())
}

pub fn session_key_from_ephemeral(
    my_pair: &EphemeralKeyPair,
    their_public_bytes: &[u8],
) -> CryptoResult<[u8; SHARED_SECRET_SIZE]> {
    derive_session_key(&my_pair.secret, their_public_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecdh_symmetric() {
        let alice = EphemeralKeyPair::generate();
        let bob = EphemeralKeyPair::generate();

        let alice_view = derive_session_key(&alice.secret, &bob.public_bytes()).unwrap();
        let bob_view = derive_session_key(&bob.secret, &alice.public_bytes()).unwrap();

        assert_eq!(alice_view, bob_view);
    }

    #[test]
    fn different_pairs_yield_different_keys() {
        let alice = EphemeralKeyPair::generate();
        let bob1 = EphemeralKeyPair::generate();
        let bob2 = EphemeralKeyPair::generate();

        let k1 = derive_session_key(&alice.secret, &bob1.public_bytes()).unwrap();
        let k2 = derive_session_key(&alice.secret, &bob2.public_bytes()).unwrap();

        assert_ne!(k1, k2);
    }

    #[test]
    fn invalid_public_key_length() {
        let alice = EphemeralKeyPair::generate();
        let result = derive_session_key(&alice.secret, &[0u8; 10]);
        assert!(matches!(result, Err(CryptoError::InvalidLength { .. })));
    }
}
