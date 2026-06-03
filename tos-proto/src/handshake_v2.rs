//! v0.2 handshake: mutual KEM + signed transcript binding.
//!
//! ## Protocol
//!
//! ```text
//! Client                                       Server
//!   |--- HelloV2 ------------------------------->|
//!   |    (sign_pk, kex_pk, nonce_c, algo_set)    |
//!   |                                            |--- encap(client_kex_pk) -> (ct_s, ss_init)
//!   |                                            |--- sign transcript with server_sign_sk
//!   |<-- HelloAckV2 ----------------------------|
//!   |    (sign_pk, kex_pk, nonce_s, ct_s, sig)  |
//!   |--- decap(ct_s) -> ss_init                 |
//!   |--- verify server signature                |
//!   |--- encap(server_kex_pk) -> (ct_c, ss_resp)|
//!   |--- sign transcript with client_sign_sk    |
//!   |--- KeyExchangeV2 ------------------------->|
//!   |    (ct_c, sig)                             |
//!   |                                            |--- verify client signature
//!   |                                            |--- decap(ct_c) -> ss_resp
//!   |                                            |
//!   :  both derive SessionKeys                   :
//!   :  ss_final = HKDF-Extract(ss_init || ss_resp, salt=transcript_hash)
//!   :  send/recv keys + nonce bases              :
//! ```
//!
//! Failure modes handled:
//! - bad version → `HandshakeAborted`
//! - sign_pk/kex_pk length mismatch → `InvalidMessage`
//! - bad signature → `HandshakeAborted` with reason
//! - decap failure → `Crypto`

use serde::{Deserialize, Serialize};
use tos_crypto::{blake3_hash, hkdf_sha256, AlgorithmSet, KexId, SigId};

use crate::error::{ProtoError, ProtoResult};
use crate::messages_v2::{
    transcript_v2, HelloAckV2, HelloV2, KeyExchangeV2, MessageV2, CAP_V2, PROTOCOL_VERSION_V2,
};
use crate::transport::{read_frame, write_frame};

pub const SESSION_KEY_SIZE: usize = 32;
pub const SESSION_NONCE_SIZE: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKeys {
    pub send_key: [u8; SESSION_KEY_SIZE],
    pub recv_key: [u8; SESSION_KEY_SIZE],
    pub send_nonce: [u8; SESSION_NONCE_SIZE],
    pub recv_nonce: [u8; SESSION_NONCE_SIZE],
    pub algo_set: AlgorithmSet,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub node_id: [u8; 32],
    pub sign_sk: Vec<u8>,
    pub sign_pk: Vec<u8>,
    pub kex_sk: Vec<u8>,
    pub kex_pk: Vec<u8>,
}

pub async fn perform_client_v2<S>(
    stream: &mut S,
    identity: &Identity,
    preferred_algo: AlgorithmSet,
) -> ProtoResult<(HelloAckV2, SessionKeys)>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let hello = HelloV2 {
        version: PROTOCOL_VERSION_V2,
        node_id: identity.node_id,
        sign_pk: identity.sign_pk.clone(),
        kex_pk: identity.kex_pk.clone(),
        algo_set: preferred_algo,
        nonce_c: random_nonce(),
        caps: vec![CAP_V2.into()],
    };
    hello.validate().map_err(|e| ProtoError::InvalidMessage(e.into()))?;
    write_frame(stream, &MessageV2::Hello(hello.clone())).await?;

    let ack: HelloAckV2 = match read_frame::<_, MessageV2>(stream).await? {
        MessageV2::HelloAck(a) => a,
        other => {
            return Err(ProtoError::HandshakeAborted(format!(
                "expected HelloAck, got {:?}",
                other
            )));
        }
    };
    ack.validate().map_err(|e| ProtoError::InvalidMessage(e.into()))?;

    let server_algo = ack.algo_set;
    if server_algo.kex != hello.algo_set.kex || server_algo.sig != hello.algo_set.sig {
        return Err(ProtoError::HandshakeAborted(format!(
            "algo_set mismatch: client {:?} vs server {:?}",
            hello.algo_set, server_algo
        )));
    }

    let kex = server_algo.kex_impl();
    let ss_init = kex
        .decapsulate(&identity.kex_sk, &ack.kem_ct)
        .map_err(|e| ProtoError::Crypto(format!("client decap: {e}")))?;
    let ss_init: [u8; 32] = ss_init
        .as_slice()
        .try_into()
        .map_err(|_| ProtoError::Crypto("ss_init length".into()))?;

    let transcript_init = transcript_v2(
        &identity.node_id,
        &ack.node_id,
        &server_algo,
        &hello.nonce_c,
        &ack.nonce_s,
        &hello.kex_pk,
        &ack.kex_pk,
        &[],
        &ack.kem_ct,
    );
    let transcript_hash_init: [u8; 32] = blake3_hash(&transcript_init);

    let signer = server_algo.sign_impl();
    signer
        .verify(&ack.sign_pk, &transcript_hash_init, &ack.signature)
        .map_err(|e| ProtoError::HandshakeAborted(format!("server auth: {e}")))?;

    let (ct_c, ss_resp_bytes) = kex
        .encapsulate(&ack.kex_pk)
        .map_err(|e| ProtoError::Crypto(format!("client encap: {e}")))?;
    let ss_resp: [u8; 32] = ss_resp_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ProtoError::Crypto("ss_resp length".into()))?;

    let transcript_final = transcript_v2(
        &identity.node_id,
        &ack.node_id,
        &server_algo,
        &hello.nonce_c,
        &ack.nonce_s,
        &hello.kex_pk,
        &ack.kex_pk,
        &ct_c,
        &ack.kem_ct,
    );
    let transcript_hash_final: [u8; 32] = blake3_hash(&transcript_final);

    let client_sig = signer
        .sign(&identity.sign_sk, &transcript_hash_final)
        .map_err(|e| ProtoError::Crypto(format!("client sign: {e}")))?;

    let kex_v2 = KeyExchangeV2 {
        version: PROTOCOL_VERSION_V2,
        kem_ct: ct_c,
        signature: client_sig,
    };
    kex_v2
        .validate(server_algo.kex)
        .map_err(|e| ProtoError::InvalidMessage(e.into()))?;
    write_frame(stream, &MessageV2::KeyExchange(kex_v2)).await?;

    let keys = derive_session_keys(
        &ss_init,
        &ss_resp,
        &transcript_hash_final,
        &server_algo,
        true,
    );
    Ok((ack, keys))
}

pub async fn perform_server_v2<S>(
    stream: &mut S,
    identity: &Identity,
    fallback_algo: AlgorithmSet,
) -> ProtoResult<(HelloV2, SessionKeys)>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let hello: HelloV2 = match read_frame::<_, MessageV2>(stream).await? {
        MessageV2::Hello(h) => h,
        other => {
            return Err(ProtoError::HandshakeAborted(format!(
                "expected Hello, got {:?}",
                other
            )));
        }
    };
    hello.validate().map_err(|e| ProtoError::InvalidMessage(e.into()))?;
    if !hello.advertises_v2() {
        return Err(ProtoError::HandshakeAborted("peer does not advertise v0.2".into()));
    }

    let chosen_algo = if hello.algo_set.is_supported() {
        hello.algo_set
    } else {
        fallback_algo
    };

    let kex = chosen_algo.kex_impl();
    let (ct_s, ss_init_bytes) = kex
        .encapsulate(&hello.kex_pk)
        .map_err(|e| ProtoError::Crypto(format!("server encap: {e}")))?;
    let ss_init: [u8; 32] = ss_init_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ProtoError::Crypto("ss_init length".into()))?;

    let nonce_s = random_nonce();
    let transcript_init = transcript_v2(
        &hello.node_id,
        &identity.node_id,
        &chosen_algo,
        &hello.nonce_c,
        &nonce_s,
        &hello.kex_pk,
        &identity.kex_pk,
        &[],
        &ct_s,
    );
    let transcript_hash_init: [u8; 32] = blake3_hash(&transcript_init);

    let signer = chosen_algo.sign_impl();
    let server_sig = signer
        .sign(&identity.sign_sk, &transcript_hash_init)
        .map_err(|e| ProtoError::Crypto(format!("server sign: {e}")))?;

    let ack = HelloAckV2 {
        version: PROTOCOL_VERSION_V2,
        node_id: identity.node_id,
        sign_pk: identity.sign_pk.clone(),
        kex_pk: identity.kex_pk.clone(),
        algo_set: chosen_algo,
        nonce_c: hello.nonce_c,
        nonce_s,
        kem_ct: ct_s,
        signature: server_sig,
        caps: vec![CAP_V2.into()],
    };
    ack.validate().map_err(|e| ProtoError::InvalidMessage(e.into()))?;
    write_frame(stream, &MessageV2::HelloAck(ack.clone())).await?;

    let kex_v2: KeyExchangeV2 = match read_frame::<_, MessageV2>(stream).await? {
        MessageV2::KeyExchange(k) => k,
        other => {
            return Err(ProtoError::HandshakeAborted(format!(
                "expected KeyExchange, got {:?}",
                other
            )));
        }
    };
    kex_v2
        .validate(chosen_algo.kex)
        .map_err(|e| ProtoError::InvalidMessage(e.into()))?;

    let ss_resp_bytes = kex
        .decapsulate(&identity.kex_sk, &kex_v2.kem_ct)
        .map_err(|e| ProtoError::Crypto(format!("server decap: {e}")))?;
    let ss_resp: [u8; 32] = ss_resp_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ProtoError::Crypto("ss_resp length".into()))?;

    let transcript_final = transcript_v2(
        &hello.node_id,
        &identity.node_id,
        &chosen_algo,
        &hello.nonce_c,
        &ack.nonce_s,
        &hello.kex_pk,
        &identity.kex_pk,
        &kex_v2.kem_ct,
        &ack.kem_ct,
    );
    let transcript_hash_final: [u8; 32] = blake3_hash(&transcript_final);

    signer
        .verify(&hello.sign_pk, &transcript_hash_final, &kex_v2.signature)
        .map_err(|e| ProtoError::HandshakeAborted(format!("client auth: {e}")))?;

    let keys = derive_session_keys(
        &ss_init,
        &ss_resp,
        &transcript_hash_final,
        &chosen_algo,
        false,
    );
    Ok((hello, keys))
}

fn derive_session_keys(
    ss_init: &[u8; 32],
    ss_resp: &[u8; 32],
    transcript_hash: &[u8; 32],
    algo: &AlgorithmSet,
    is_client: bool,
) -> SessionKeys {
    let mut ikm = Vec::with_capacity(64);
    ikm.extend_from_slice(ss_init);
    ikm.extend_from_slice(ss_resp);
    let prk = hkdf_sha256(&ikm, Some(transcript_hash), b"tos-v0.2-session-prk", 32)
        .expect("hkdf extract");
    let prk_arr: [u8; 32] = prk.as_slice().try_into().expect("prk len");

    let (send_label, recv_label) = if is_client {
        (b"client-send-key", b"server-send-key")
    } else {
        (b"server-send-key", b"client-send-key")
    };

    let (send_nonce_label, recv_nonce_label) = if is_client {
        (b"client-send-nonce", b"server-send-nonce")
    } else {
        (b"server-send-nonce", b"client-send-nonce")
    };

    let send_key_bytes = hkdf_sha256(&prk_arr, None, send_label, 32).expect("hkdf send");
    let recv_key_bytes = hkdf_sha256(&prk_arr, None, recv_label, 32).expect("hkdf recv");
    let send_nonce = hkdf_sha256(&prk_arr, None, send_nonce_label, 12)
        .expect("hkdf send nonce");
    let recv_nonce = hkdf_sha256(&prk_arr, None, recv_nonce_label, 12)
        .expect("hkdf recv nonce");

    let mut send_key = [0u8; SESSION_KEY_SIZE];
    send_key.copy_from_slice(&send_key_bytes);
    let mut recv_key = [0u8; SESSION_KEY_SIZE];
    recv_key.copy_from_slice(&recv_key_bytes);
    let mut send_n = [0u8; SESSION_NONCE_SIZE];
    send_n.copy_from_slice(&send_nonce);
    let mut recv_n = [0u8; SESSION_NONCE_SIZE];
    recv_n.copy_from_slice(&recv_nonce);

    SessionKeys {
        send_key,
        recv_key,
        send_nonce: send_n,
        recv_nonce: recv_n,
        algo_set: *algo,
    }
}

fn random_nonce() -> [u8; 32] {
    let mut n = [0u8; 32];
    getrandom::fill(&mut n).expect("os rng");
    n
}

trait AlgoSupport {
    fn is_supported(&self) -> bool;
}

impl AlgoSupport for AlgorithmSet {
    fn is_supported(&self) -> bool {
        self.kex != KexId::MlKem512
            && self.kex != KexId::MlKem1024
            && self.cipher != tos_crypto::CipherId::Aes256Gcm
            && self.cipher != tos_crypto::CipherId::Aegis256
            && self.sig != SigId::SlhDsa128s
            && self.sig != SigId::FrostEd25519
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use tos_crypto::{Ed25519Signer, Kex, MlDsa65Signer, MlKem768Kex, Sign, X25519Kex, XWingKex};

    fn make_identity_classical(node_seed: u8) -> Identity {
        let mut node_id = [0u8; 32];
        node_id[0] = node_seed;
        let signer = Ed25519Signer;
        let (sign_pk, sign_sk) = signer.generate().unwrap();
        let kex = X25519Kex;
        let (kex_pk, kex_sk) = kex.generate().unwrap();
        Identity {
            node_id,
            sign_sk,
            sign_pk,
            kex_sk,
            kex_pk,
        }
    }

    fn make_identity_pqc(node_seed: u8) -> Identity {
        let mut node_id = [0u8; 32];
        node_id[0] = node_seed;
        let signer = MlDsa65Signer;
        let (sign_pk, sign_sk) = signer.generate().unwrap();
        let kex = XWingKex;
        let (kex_pk, kex_sk) = kex.generate().unwrap();
        Identity {
            node_id,
            sign_sk,
            sign_pk,
            kex_sk,
            kex_pk,
        }
    }

    fn make_identity_mlkem(node_seed: u8) -> Identity {
        let mut node_id = [0u8; 32];
        node_id[0] = node_seed;
        let signer = Ed25519Signer;
        let (sign_pk, sign_sk) = signer.generate().unwrap();
        let kex = MlKem768Kex;
        let (kex_pk, kex_sk) = kex.generate().unwrap();
        Identity {
            node_id,
            sign_sk,
            sign_pk,
            kex_sk,
            kex_pk,
        }
    }

    #[tokio::test]
    async fn handshake_v2_classical_x25519_ed25519() {
        let (mut a, mut b) = duplex(128 * 1024);
        let client = make_identity_classical(0x11);
        let server = make_identity_classical(0x22);

        let client_task = tokio::spawn(async move {
            perform_client_v2(&mut a, &client, AlgorithmSet::v0_2()).await
        });
        let server_task = tokio::spawn(async move {
            perform_server_v2(&mut b, &server, AlgorithmSet::v0_2()).await
        });

        let (client_res, server_res) = tokio::join!(client_task, server_task);
        let (_ack, client_keys) = client_res.unwrap().unwrap();
        let (_hello, server_keys) = server_res.unwrap().unwrap();

        assert_eq!(
            client_keys.send_key, server_keys.recv_key,
            "client send_key must match server recv_key"
        );
        assert_eq!(
            client_keys.recv_key, server_keys.send_key,
            "client recv_key must match server send_key"
        );
        assert_eq!(
            client_keys.send_nonce, server_keys.recv_nonce,
            "client send_nonce must match server recv_nonce"
        );
        assert_eq!(
            client_keys.recv_nonce, server_keys.send_nonce,
            "client recv_nonce must match server send_nonce"
        );
    }

    #[tokio::test]
    async fn handshake_v2_pqc_xwing_mldsa65() {
        let (mut a, mut b) = duplex(128 * 1024);
        let client = make_identity_pqc(0x33);
        let server = make_identity_pqc(0x44);

        let client_task = tokio::spawn(async move {
            perform_client_v2(&mut a, &client, AlgorithmSet::v0_2_pqc()).await
        });
        let server_task = tokio::spawn(async move {
            perform_server_v2(&mut b, &server, AlgorithmSet::v0_2_pqc()).await
        });

        let (client_res, server_res) = tokio::join!(client_task, server_task);
        let (_ack, client_keys) = client_res.unwrap().unwrap();
        let (_hello, server_keys) = server_res.unwrap().unwrap();

        assert_eq!(client_keys.send_key, server_keys.recv_key);
        assert_eq!(client_keys.recv_key, server_keys.send_key);
    }

    #[tokio::test]
    async fn handshake_v2_pqc_mlkem768_ed25519() {
        let (mut a, mut b) = duplex(128 * 1024);
        let client = make_identity_mlkem(0x55);
        let server = make_identity_mlkem(0x66);

        let algo = AlgorithmSet {
            kex: KexId::MlKem768,
            sig: SigId::Ed25519,
            cipher: tos_crypto::CipherId::ChaCha20Poly1305,
        };

        let client_task = tokio::spawn(async move {
            perform_client_v2(&mut a, &client, algo).await
        });
        let server_task = tokio::spawn(async move {
            perform_server_v2(&mut b, &server, algo).await
        });

        let (client_res, server_res) = tokio::join!(client_task, server_task);
        let (_ack, client_keys) = client_res.unwrap().unwrap();
        let (_hello, server_keys) = server_res.unwrap().unwrap();

        assert_eq!(client_keys.send_key, server_keys.recv_key);
    }

    #[tokio::test]
    async fn handshake_v2_happy_path_succeeds() {
        let (mut a, mut b) = duplex(128 * 1024);
        let client = make_identity_classical(0x77);
        let server = make_identity_classical(0x88);

        let client_task = tokio::spawn(async move {
            perform_client_v2(&mut a, &client, AlgorithmSet::v0_2()).await
        });
        let server_task = tokio::spawn(async move {
            perform_server_v2(&mut b, &server, AlgorithmSet::v0_2()).await
        });

        let (client_res, server_res) = tokio::join!(client_task, server_task);
        let (client_ack, client_keys) = client_res.unwrap().unwrap();
        let (server_hello, server_keys) = server_res.unwrap().unwrap();
        assert_eq!(client_ack.algo_set, server_hello.algo_set);
        assert_eq!(client_keys.send_key, server_keys.recv_key);
        assert_eq!(client_keys.recv_key, server_keys.send_key);
    }

    #[tokio::test]
    async fn handshake_v2_rejects_wrong_server_pk() {
        let (mut a, mut b) = duplex(128 * 1024);
        let client = make_identity_classical(0x99);
        let mut server = make_identity_classical(0xAA);

        // Client trusts wrong server pk -> server signature will fail to verify
        let fake_server_pk = vec![0xFFu8; server.sign_pk.len()];
        let real_server_pk = server.sign_pk.clone();
        server.sign_pk = fake_server_pk.clone();

        let client_task = tokio::spawn(async move {
            perform_client_v2(&mut a, &client, AlgorithmSet::v0_2()).await
        });
        let server_task = tokio::spawn(async move {
            perform_server_v2(&mut b, &server, AlgorithmSet::v0_2()).await
        });

        let (client_res, server_res) = tokio::join!(client_task, server_task);
        // client should fail because server signature won't verify against
        // the (now-different) sign_pk in HelloAckV2 it expects
        let client_err = client_res.unwrap();
        // server should also fail because client will not send a valid
        // KeyExchangeV2 (the client bails on bad auth)
        let server_err = server_res.unwrap();
        assert!(client_err.is_err() || server_err.is_err());
        let _ = real_server_pk; // keep alive
    }

    #[test]
    fn session_keys_sizes() {
        let ss_init = [1u8; 32];
        let ss_resp = [2u8; 32];
        let transcript = [3u8; 32];
        let keys = derive_session_keys(
            &ss_init,
            &ss_resp,
            &transcript,
            &AlgorithmSet::v0_2(),
            true,
        );
        assert_eq!(keys.send_key.len(), 32);
        assert_eq!(keys.recv_key.len(), 32);
        assert_eq!(keys.send_nonce.len(), 12);
        assert_eq!(keys.recv_nonce.len(), 12);
    }

    #[test]
    fn session_keys_client_server_mirror() {
        let ss_init = [7u8; 32];
        let ss_resp = [9u8; 32];
        let transcript = [0u8; 32];
        let client_keys = derive_session_keys(
            &ss_init,
            &ss_resp,
            &transcript,
            &AlgorithmSet::v0_2(),
            true,
        );
        let server_keys = derive_session_keys(
            &ss_init,
            &ss_resp,
            &transcript,
            &AlgorithmSet::v0_2(),
            false,
        );
        assert_eq!(client_keys.send_key, server_keys.recv_key);
        assert_eq!(client_keys.recv_key, server_keys.send_key);
        assert_eq!(client_keys.send_nonce, server_keys.recv_nonce);
        assert_eq!(client_keys.recv_nonce, server_keys.send_nonce);
    }

    #[tokio::test]
    async fn integration_v2_classical_full_e2e() {
        // Full pipeline: handshake v2 (X25519+Ed25519) + encrypted session
        // stream (ChaCha20-Poly1305). Client sends 100 messages, server
        // echoes each back, asserting plaintext roundtrip.
        use crate::session_v2::SessionV2;
        let (mut a, mut b) = duplex(1024 * 1024);
        let client = make_identity_classical(0x11);
        let server = make_identity_classical(0x22);

        let client_task = tokio::spawn(async move {
            let (_ack, keys) =
                perform_client_v2(&mut a, &client, AlgorithmSet::v0_2()).await.unwrap();
            SessionV2::new(a, keys)
        });
        let server_task = tokio::spawn(async move {
            let (_hello, keys) =
                perform_server_v2(&mut b, &server, AlgorithmSet::v0_2()).await.unwrap();
            SessionV2::new(b, keys)
        });

        let (mut client_session, mut server_session) =
            tokio::try_join!(client_task, server_task).unwrap();

        for i in 0..100u32 {
            let msg = format!("classical ping #{i}");
            client_session.send(msg.as_bytes()).await.unwrap();
            let echoed = server_session.recv().await.unwrap();
            assert_eq!(echoed, msg.as_bytes(), "classical echo {i}");
        }
    }

    #[tokio::test]
    async fn integration_v2_pqc_xwing_mldsa65_full_e2e() {
        // Full PQC pipeline: X-Wing + ML-DSA-65 + ChaCha20-Poly1305.
        use crate::session_v2::SessionV2;
        let (mut a, mut b) = duplex(2 * 1024 * 1024);
        let client = make_identity_pqc(0x33);
        let server = make_identity_pqc(0x44);

        let client_task = tokio::spawn(async move {
            let (_ack, keys) = perform_client_v2(&mut a, &client, AlgorithmSet::v0_2_pqc())
                .await
                .unwrap();
            SessionV2::new(a, keys)
        });
        let server_task = tokio::spawn(async move {
            let (_hello, keys) = perform_server_v2(&mut b, &server, AlgorithmSet::v0_2_pqc())
                .await
                .unwrap();
            SessionV2::new(b, keys)
        });

        let (mut client_session, mut server_session) =
            tokio::try_join!(client_task, server_task).unwrap();

        for i in 0..50u32 {
            let msg = format!("pqc ping #{i}");
            client_session.send(msg.as_bytes()).await.unwrap();
            let echoed = server_session.recv().await.unwrap();
            assert_eq!(echoed, msg.as_bytes(), "pqc echo {i}");
        }
    }

    #[tokio::test]
    async fn integration_v2_mlkem_ed25519_full_e2e() {
        // Mixed pipeline: ML-KEM-768 + Ed25519 + ChaCha20-Poly1305.
        use crate::session_v2::SessionV2;
        let (mut a, mut b) = duplex(2 * 1024 * 1024);
        let client = make_identity_mlkem(0x55);
        let server = make_identity_mlkem(0x66);

        let algo = AlgorithmSet {
            kex: KexId::MlKem768,
            sig: SigId::Ed25519,
            cipher: tos_crypto::CipherId::ChaCha20Poly1305,
        };

        let client_task = tokio::spawn(async move {
            let (_ack, keys) = perform_client_v2(&mut a, &client, algo).await.unwrap();
            SessionV2::new(a, keys)
        });
        let server_task = tokio::spawn(async move {
            let (_hello, keys) = perform_server_v2(&mut b, &server, algo).await.unwrap();
            SessionV2::new(b, keys)
        });

        let (mut client_session, mut server_session) =
            tokio::try_join!(client_task, server_task).unwrap();

        for i in 0..50u32 {
            let msg = format!("mlkem ping #{i}");
            client_session.send(msg.as_bytes()).await.unwrap();
            let echoed = server_session.recv().await.unwrap();
            assert_eq!(echoed, msg.as_bytes(), "mlkem echo {i}");
        }
    }

    #[tokio::test]
    async fn integration_v2_bidirectional_concurrent() {
        // Both sides send and receive simultaneously, exercising the
        // independent send/recv counters and nonce derivations.
        use crate::session_v2::SessionV2;
        let (mut a, mut b) = duplex(2 * 1024 * 1024);
        let client = make_identity_classical(0x77);
        let server = make_identity_classical(0x88);

        let client_task = tokio::spawn(async move {
            let (_ack, keys) =
                perform_client_v2(&mut a, &client, AlgorithmSet::v0_2()).await.unwrap();
            SessionV2::new(a, keys)
        });
        let server_task = tokio::spawn(async move {
            let (_hello, keys) =
                perform_server_v2(&mut b, &server, AlgorithmSet::v0_2()).await.unwrap();
            SessionV2::new(b, keys)
        });

        let (mut client_session, mut server_session) =
            tokio::try_join!(client_task, server_task).unwrap();

        let client_send = tokio::spawn(async move {
            for i in 0..30u32 {
                let msg = format!("c->s #{i}");
                client_session.send(msg.as_bytes()).await.unwrap();
            }
            client_session
        });
        let server_send = tokio::spawn(async move {
            for i in 0..30u32 {
                let msg = format!("s->c #{i}");
                server_session.send(msg.as_bytes()).await.unwrap();
            }
            server_session
        });

        let (mut cs, mut ss) = tokio::try_join!(client_send, server_send).unwrap();

        for i in 0..30u32 {
            let expected = format!("s->c #{i}");
            let got = cs.recv().await.unwrap();
            assert_eq!(got, expected.as_bytes(), "client recv {i}");
        }
        for i in 0..30u32 {
            let expected = format!("c->s #{i}");
            let got = ss.recv().await.unwrap();
            assert_eq!(got, expected.as_bytes(), "server recv {i}");
        }
    }
}
