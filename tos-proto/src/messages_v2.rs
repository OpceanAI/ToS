//! v0.2 wire messages: Hello/HelloAck/KeyExchangeV2/AuthProof with
//! AlgorithmSet negotiation and variable-length PQ material.
//!
//! This module is **additive** with `messages` (v1.0). The framing function
//! `dispatch_v1_or_v2` reads the first byte of a frame to decide which
//! envelope to deserialize. v1.0 nodes speak only v1.0 messages, v0.2
//! nodes negotiate AlgorithmSet via the V2 envelope and may downgrade
//! to v1.0 if the peer does not advertise V2 capability.
//!
//! Encoding: bincode default config + length-prefix `u32` BE (same as v1.0).

use serde::{Deserialize, Serialize};
use tos_crypto::{AlgorithmSet, KexId, SigId};

#[cfg(test)]
use tos_crypto::CipherId;

pub const PROTOCOL_VERSION_V2: u8 = 2;
pub const NONCE_SIZE: usize = 32;

pub const CAP_V2: &str = "v0.2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloV2 {
    pub version: u8,
    pub node_id: [u8; 32],
    pub sign_pk: Vec<u8>,
    pub kex_pk: Vec<u8>,
    pub algo_set: AlgorithmSet,
    pub nonce_c: [u8; NONCE_SIZE],
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloAckV2 {
    pub version: u8,
    pub node_id: [u8; 32],
    pub sign_pk: Vec<u8>,
    pub kex_pk: Vec<u8>,
    pub algo_set: AlgorithmSet,
    pub nonce_c: [u8; NONCE_SIZE],
    pub nonce_s: [u8; NONCE_SIZE],
    pub kem_ct: Vec<u8>,
    pub signature: Vec<u8>,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyExchangeV2 {
    pub version: u8,
    pub kem_ct: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthProof {
    pub sign_algo: SigId,
    pub transcript_hash: [u8; 32],
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageV2 {
    Hello(HelloV2),
    HelloAck(HelloAckV2),
    KeyExchange(KeyExchangeV2),
}

impl HelloV2 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PROTOCOL_VERSION_V2 {
            return Err("hello_v2: bad version");
        }
        let sig = self.algo_set.sig;
        let kex = self.algo_set.kex;
        if self.sign_pk.len() != sig.pk_size() {
            return Err("hello_v2: sign_pk length");
        }
        if self.kex_pk.len() != kex.pk_size() {
            return Err("hello_v2: kex_pk length");
        }
        Ok(())
    }

    pub fn advertises_v2(&self) -> bool {
        self.caps.iter().any(|c| c == CAP_V2) || self.version == PROTOCOL_VERSION_V2
    }
}

impl HelloAckV2 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PROTOCOL_VERSION_V2 {
            return Err("hello_ack_v2: bad version");
        }
        let sig = self.algo_set.sig;
        let kex = self.algo_set.kex;
        if self.sign_pk.len() != sig.pk_size() {
            return Err("hello_ack_v2: sign_pk length");
        }
        if self.kex_pk.len() != kex.pk_size() {
            return Err("hello_ack_v2: kex_pk length");
        }
        if self.kem_ct.len() != kex.ct_size() {
            return Err("hello_ack_v2: kem_ct length");
        }
        if self.nonce_c != [0u8; NONCE_SIZE] && self.nonce_s == [0u8; NONCE_SIZE] {
            return Err("hello_ack_v2: zero server nonce");
        }
        Ok(())
    }
}

impl KeyExchangeV2 {
    pub fn validate(&self, kex_id: KexId) -> Result<(), &'static str> {
        if self.version != PROTOCOL_VERSION_V2 {
            return Err("key_exchange_v2: bad version");
        }
        if self.kem_ct.len() != kex_id.ct_size() {
            return Err("key_exchange_v2: kem_ct length");
        }
        Ok(())
    }
}

pub fn transcript_v2(
    client_node: &[u8; 32],
    server_node: &[u8; 32],
    algo_set: &AlgorithmSet,
    nonce_c: &[u8; NONCE_SIZE],
    nonce_s: &[u8; NONCE_SIZE],
    client_kex_pk: &[u8],
    server_kex_pk: &[u8],
    client_kem_ct: &[u8],
    server_kem_ct: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(b"tos-v0.2-transcript-v1");
    out.push(PROTOCOL_VERSION_V2);
    out.extend_from_slice(client_node);
    out.extend_from_slice(server_node);
    out.extend_from_slice(&(algo_set.kex as u16).to_be_bytes());
    out.extend_from_slice(&(algo_set.sig as u16).to_be_bytes());
    out.extend_from_slice(&(algo_set.cipher as u16).to_be_bytes());
    out.extend_from_slice(nonce_c);
    out.extend_from_slice(nonce_s);
    let cli = (client_kex_pk.len() as u32).to_be_bytes();
    out.extend_from_slice(&cli);
    out.extend_from_slice(client_kex_pk);
    let svr = (server_kex_pk.len() as u32).to_be_bytes();
    out.extend_from_slice(&svr);
    out.extend_from_slice(server_kex_pk);
    let c = (client_kem_ct.len() as u32).to_be_bytes();
    out.extend_from_slice(&c);
    out.extend_from_slice(client_kem_ct);
    let s = (server_kem_ct.len() as u32).to_be_bytes();
    out.extend_from_slice(&s);
    out.extend_from_slice(server_kem_ct);
    out
}

pub fn dispatch_v1_or_v2(buf: &[u8]) -> Result<u8, &'static str> {
    if buf.is_empty() {
        return Err("empty frame");
    }
    match buf[0] {
        0u8 => Err("null discriminator"),
        1u8 | 2u8 | 3u8 | 4u8 | 5u8 | 6u8 | 7u8 | 8u8 | 9u8 => {
            if buf[0] == PROTOCOL_VERSION_V2 {
                Ok(2)
            } else if buf[0] == 1 {
                Ok(1)
            } else {
                Err("unsupported protocol version in discriminator")
            }
        }
        _ => {
            if buf[0] == PROTOCOL_VERSION_V2 {
                Ok(2)
            } else if buf[0] == 1 {
                Ok(1)
            } else {
                Err("unsupported protocol version in discriminator")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello_v2_pqc() -> HelloV2 {
        let algo = AlgorithmSet::v0_2_pqc();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        HelloV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x11u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            caps: vec![CAP_V2.into(), "d2d".into()],
        }
    }

    fn hello_v2_classical() -> HelloV2 {
        let algo = AlgorithmSet::v0_2();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        HelloV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x11u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            caps: vec![CAP_V2.into()],
        }
    }

    fn sample_pq_keys(algo: &AlgorithmSet) -> (Vec<u8>, Vec<u8>) {
        let sign_pk = vec![0xABu8; algo.sig.pk_size()];
        let kex_pk = vec![0xCDu8; algo.kex.pk_size()];
        (sign_pk, kex_pk)
    }

    #[test]
    fn hello_v2_pqc_lengths_match_pqc_sizes() {
        let h = hello_v2_pqc();
        assert_eq!(h.sign_pk.len(), SigId::MlDsa65.pk_size());
        assert_eq!(h.kex_pk.len(), KexId::XWing.pk_size());
        assert!(h.validate().is_ok());
    }

    #[test]
    fn hello_v2_classical_lengths_match_classical_sizes() {
        let h = hello_v2_classical();
        assert_eq!(h.sign_pk.len(), SigId::Ed25519.pk_size());
        assert_eq!(h.kex_pk.len(), KexId::X25519.pk_size());
        assert!(h.validate().is_ok());
    }

    #[test]
    fn hello_v2_rejects_mismatched_sign_pk_len() {
        let mut h = hello_v2_pqc();
        h.sign_pk = vec![0u8; 16];
        assert!(h.validate().is_err());
    }

    #[test]
    fn hello_v2_rejects_mismatched_kex_pk_len() {
        let mut h = hello_v2_pqc();
        h.kex_pk = vec![0u8; 16];
        assert!(h.validate().is_err());
    }

    #[test]
    fn hello_v2_rejects_bad_version() {
        let mut h = hello_v2_pqc();
        h.version = 1;
        assert!(h.validate().is_err());
    }

    #[test]
    fn hello_v2_serde_roundtrip_pqc() {
        let h = hello_v2_pqc();
        let bytes = bincode::serialize(&h).unwrap();
        let h2: HelloV2 = bincode::deserialize(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn hello_v2_serde_roundtrip_classical() {
        let h = hello_v2_classical();
        let bytes = bincode::serialize(&h).unwrap();
        let h2: HelloV2 = bincode::deserialize(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn hello_v2_advertises_v2() {
        let h = hello_v2_pqc();
        assert!(h.advertises_v2());
    }

    #[test]
    fn hello_v2_no_v2_cap_still_v2() {
        let mut h = hello_v2_pqc();
        h.caps = vec!["d2d".into()];
        assert!(h.advertises_v2());
    }

    #[test]
    fn hello_ack_v2_pqc_lengths() {
        let algo = AlgorithmSet::v0_2_pqc();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        let kem_ct = vec![0xEFu8; algo.kex.ct_size()];
        let ack = HelloAckV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x22u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            nonce_s: [0x44u8; NONCE_SIZE],
            kem_ct,
            signature: vec![0u8; algo.sig.sig_size()],
            caps: vec![CAP_V2.into()],
        };
        assert!(ack.validate().is_ok());
    }

    #[test]
    fn hello_ack_v2_rejects_bad_kem_ct_len() {
        let algo = AlgorithmSet::v0_2_pqc();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        let ack = HelloAckV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x22u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            nonce_s: [0x44u8; NONCE_SIZE],
            kem_ct: vec![0u8; 16],
            signature: vec![0u8; algo.sig.sig_size()],
            caps: vec![],
        };
        assert!(ack.validate().is_err());
    }

    #[test]
    fn hello_ack_v2_serde_roundtrip_pqc() {
        let algo = AlgorithmSet::v0_2_pqc();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        let kem_ct = vec![0xEFu8; algo.kex.ct_size()];
        let ack = HelloAckV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x22u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            nonce_s: [0x44u8; NONCE_SIZE],
            kem_ct,
            signature: vec![0u8; algo.sig.sig_size()],
            caps: vec![CAP_V2.into()],
        };
        let bytes = bincode::serialize(&ack).unwrap();
        let ack2: HelloAckV2 = bincode::deserialize(&bytes).unwrap();
        assert_eq!(ack, ack2);
    }

    #[test]
    fn key_exchange_v2_pqc_lengths() {
        let algo = AlgorithmSet::v0_2_pqc();
        let kex = KeyExchangeV2 {
            version: PROTOCOL_VERSION_V2,
            kem_ct: vec![0x77u8; algo.kex.ct_size()],
            signature: vec![0u8; algo.sig.sig_size()],
        };
        assert!(kex.validate(algo.kex).is_ok());
    }

    #[test]
    fn key_exchange_v2_rejects_bad_kem_ct() {
        let algo = AlgorithmSet::v0_2_pqc();
        let kex = KeyExchangeV2 {
            version: PROTOCOL_VERSION_V2,
            kem_ct: vec![0u8; 16],
            signature: vec![0u8; algo.sig.sig_size()],
        };
        assert!(kex.validate(algo.kex).is_err());
    }

    #[test]
    fn message_v2_roundtrip() {
        let h = hello_v2_pqc();
        let m = MessageV2::Hello(h);
        let bytes = bincode::serialize(&m).unwrap();
        let m2: MessageV2 = bincode::deserialize(&bytes).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn transcript_v2_is_deterministic() {
        let a = transcript_v2(
            &[0x11u8; 32],
            &[0x22u8; 32],
            &AlgorithmSet::v0_2_pqc(),
            &[0x33u8; NONCE_SIZE],
            &[0x44u8; NONCE_SIZE],
            &[0xABu8; 1216],
            &[0xCDu8; 1216],
            &[0xEFu8; 1120],
            &[0x99u8; 1120],
        );
        let b = transcript_v2(
            &[0x11u8; 32],
            &[0x22u8; 32],
            &AlgorithmSet::v0_2_pqc(),
            &[0x33u8; NONCE_SIZE],
            &[0x44u8; NONCE_SIZE],
            &[0xABu8; 1216],
            &[0xCDu8; 1216],
            &[0xEFu8; 1120],
            &[0x99u8; 1120],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn transcript_v2_changes_with_nonce() {
        let a = transcript_v2(
            &[0x11u8; 32],
            &[0x22u8; 32],
            &AlgorithmSet::v0_2_pqc(),
            &[0x33u8; NONCE_SIZE],
            &[0x44u8; NONCE_SIZE],
            &[0xABu8; 1216],
            &[0xCDu8; 1216],
            &[0xEFu8; 1120],
            &[0x99u8; 1120],
        );
        let b = transcript_v2(
            &[0x11u8; 32],
            &[0x22u8; 32],
            &AlgorithmSet::v0_2_pqc(),
            &[0xAAu8; NONCE_SIZE],
            &[0x44u8; NONCE_SIZE],
            &[0xABu8; 1216],
            &[0xCDu8; 1216],
            &[0xEFu8; 1120],
            &[0x99u8; 1120],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn dispatch_v1_or_v2_recognizes_versions() {
        let v2_bytes = bincode::serialize(&PROTOCOL_VERSION_V2).unwrap();
        assert_eq!(dispatch_v1_or_v2(&v2_bytes), Ok(2));
        let v1_bytes = bincode::serialize(&1u8).unwrap();
        assert_eq!(dispatch_v1_or_v2(&v1_bytes), Ok(1));
        assert!(dispatch_v1_or_v2(&[]).is_err());
    }

    #[test]
    fn cipher_in_set() {
        let algo = AlgorithmSet::v0_2_pqc();
        assert_eq!(algo.cipher, CipherId::ChaCha20Poly1305);
    }

    #[tokio::test]
    async fn hello_v2_wire_roundtrip_pqc() {
        use crate::transport::{read_frame, write_frame};
        use tokio::io::duplex;
        let (mut a, mut b) = duplex(64 * 1024);
        let h = hello_v2_pqc();
        let h_clone = h.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &h_clone).await.unwrap();
        });
        let got: HelloV2 = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, h);
    }

    #[tokio::test]
    async fn hello_v2_wire_roundtrip_classical() {
        use crate::transport::{read_frame, write_frame};
        use tokio::io::duplex;
        let (mut a, mut b) = duplex(64 * 1024);
        let h = hello_v2_classical();
        let h_clone = h.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &h_clone).await.unwrap();
        });
        let got: HelloV2 = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, h);
    }

    #[tokio::test]
    async fn hello_ack_v2_wire_roundtrip_pqc() {
        use crate::transport::{read_frame, write_frame};
        use tokio::io::duplex;
        let (mut a, mut b) = duplex(64 * 1024);
        let algo = AlgorithmSet::v0_2_pqc();
        let (sign_pk, kex_pk) = sample_pq_keys(&algo);
        let kem_ct = vec![0xEFu8; algo.kex.ct_size()];
        let ack = HelloAckV2 {
            version: PROTOCOL_VERSION_V2,
            node_id: [0x22u8; 32],
            sign_pk,
            kex_pk,
            algo_set: algo,
            nonce_c: [0x33u8; NONCE_SIZE],
            nonce_s: [0x44u8; NONCE_SIZE],
            kem_ct,
            signature: vec![0u8; algo.sig.sig_size()],
            caps: vec![CAP_V2.into()],
        };
        let ack_clone = ack.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &ack_clone).await.unwrap();
        });
        let got: HelloAckV2 = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, ack);
    }

    #[tokio::test]
    async fn key_exchange_v2_wire_roundtrip() {
        use crate::transport::{read_frame, write_frame};
        use tokio::io::duplex;
        let (mut a, mut b) = duplex(64 * 1024);
        let algo = AlgorithmSet::v0_2_pqc();
        let kex = KeyExchangeV2 {
            version: PROTOCOL_VERSION_V2,
            kem_ct: vec![0x77u8; algo.kex.ct_size()],
            signature: vec![0u8; algo.sig.sig_size()],
        };
        let k_clone = kex.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &k_clone).await.unwrap();
        });
        let got: KeyExchangeV2 = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, kex);
    }
}
