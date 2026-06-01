use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Mutex;

use crate::error::{ProtoError, ProtoResult};
use crate::transport::Transport;

pub struct QuicTransport {
    endpoint: Option<Endpoint>,
    closed: Arc<Mutex<bool>>,
}

impl QuicTransport {
    pub async fn bind(addr: &str) -> std::io::Result<Self> {
        let server_config = server_config_self_signed()?;
        let endpoint = Endpoint::server(
            server_config,
            addr.parse().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("bad bind addr: {e}"),
                )
            })?,
        )?;
        Ok(Self {
            endpoint: Some(endpoint),
            closed: Arc::new(Mutex::new(false)),
        })
    }

    pub fn dial() -> std::io::Result<Self> {
        let local = std::net::SocketAddr::from(([0, 0, 0, 0], 0));
        let mut endpoint =
            Endpoint::client(local).map_err(|e| std::io::Error::other(format!("{e}")))?;
        let client_config = client_config_skip_verify();
        endpoint.set_default_client_config(client_config);
        Ok(Self {
            endpoint: Some(endpoint),
            closed: Arc::new(Mutex::new(false)),
        })
    }
}

#[async_trait]
impl Transport for QuicTransport {
    type Stream = QuicStream;

    async fn connect(&self, addr: &str) -> ProtoResult<Self::Stream> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| ProtoError::HandshakeAborted("no quic client endpoint".into()))?;

        let server_name = "tos";
        let conn: Connection = endpoint
            .connect(
                addr.parse()
                    .map_err(|e| ProtoError::HandshakeAborted(format!("bad quic addr: {e}")))?,
                server_name,
            )
            .map_err(|e| ProtoError::HandshakeAborted(format!("quic connect: {e}")))?
            .await
            .map_err(|e| ProtoError::HandshakeAborted(format!("quic connect await: {e}")))?;
        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| ProtoError::HandshakeAborted(format!("open_bi: {e}")))?;
        Ok(QuicStream::new(send, recv))
    }

    async fn accept(&self) -> ProtoResult<(Self::Stream, String)> {
        let endpoint = self.endpoint.as_ref().ok_or_else(|| {
            ProtoError::HandshakeAborted("no quic endpoint bound".into())
        })?;
        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| ProtoError::HandshakeAborted("quic accept returned None".into()))?;
        let conn = incoming
            .await
            .map_err(|e| ProtoError::HandshakeAborted(format!("quic incoming: {e}")))?;
        let peer = conn.remote_address().to_string();
        let (send, recv) = conn.accept_bi().await.map_err(|e| {
            ProtoError::HandshakeAborted(format!("accept_bi: {e}"))
        })?;
        Ok((QuicStream::new(send, recv), peer))
    }

    async fn close(&mut self) -> ProtoResult<()> {
        let mut closed = self.closed.lock().await;
        if !*closed {
            *closed = true;
            if let Some(ep) = self.endpoint.take() {
                ep.close(0u32.into(), b"bye");
            }
        }
        Ok(())
    }
}

pub struct QuicStream {
    send: SendStream,
    recv: RecvStream,
}

impl QuicStream {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }
}

impl AsyncRead for QuicStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.send)
            .poll_write(cx, buf)
            .map_err(|e| std::io::Error::other(format!("{e}")))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.send)
            .poll_flush(cx)
            .map_err(|e| std::io::Error::other(format!("{e}")))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.send)
            .poll_shutdown(cx)
            .map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

fn server_config_self_signed() -> std::io::Result<ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["tos".into()])
        .map_err(|e| std::io::Error::other(format!("rcgen: {e}")))?;
    let cert_der = cert.cert.der().to_vec();
    let key_der = cert.signing_key.serialized_der().to_vec();
    let key_der: quinn::rustls::pki_types::PrivateKeyDer<'static> =
        quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(key_der).into();
    let cert_chain: Vec<quinn::rustls::pki_types::CertificateDer<'static>> = vec![
        quinn::rustls::pki_types::CertificateDer::from(cert_der),
    ];
    ServerConfig::with_single_cert(cert_chain, key_der)
        .map_err(|e| std::io::Error::other(format!("server_config: {e}")))
}

fn client_config_skip_verify() -> ClientConfig {
    let rustls_cfg = build_rustls_skip_verify();
    let quic = QuicClientConfig::try_from(rustls_cfg)
        .expect("rustls config must include TLS 1.3 with initial suite");
    ClientConfig::new(Arc::new(quic))
}

fn build_rustls_skip_verify() -> quinn::rustls::ClientConfig {
    use quinn::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use quinn::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use quinn::rustls::{DigitallySignedStruct, Error, SignatureScheme};

    #[derive(Debug)]
    struct SkipVerify;

    impl ServerCertVerifier for SkipVerify {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ED25519,
            ]
        }
    }

    let mut cfg = quinn::rustls::ClientConfig::builder_with_provider(
        quinn::rustls::crypto::ring::default_provider().into(),
    )
    .with_safe_default_protocol_versions()
    .expect("safe protocols")
    .with_root_certificates(quinn::rustls::RootCertStore::empty())
    .with_no_client_auth();
    cfg.dangerous().set_certificate_verifier(Arc::new(SkipVerify));
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{Hello, HelloAck, PROTOCOL_VERSION};
    use crate::transport::{exchange_hello, read_frame, write_frame};
    #[tokio::test]
    async fn quic_loopback_exchange_hello() {
        let server = QuicTransport::bind("127.0.0.1:0").await.unwrap();
        let local = server.endpoint.as_ref().unwrap().local_addr().unwrap();
        let addr_str = local.to_string();

        let server_hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [1u8; 32],
            public_key: [2u8; 32],
            encrypt: false,
            caps: vec![],
        };

        let server_task = tokio::spawn(async move {
            let (mut stream, _peer) = server.accept().await.unwrap();
            let _ = exchange_hello(&mut stream, &server_hello, false).await;
            let mut sink = [0u8; 16];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await;
        });

        let dial = QuicTransport::dial().unwrap();
        let mut stream = dial.connect(&addr_str).await.unwrap();
        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [3u8; 32],
            public_key: [4u8; 32],
            encrypt: false,
            caps: vec![],
        };
        let ack = exchange_hello(&mut stream, &hello, true).await.unwrap();
        assert_eq!(ack.node_id, [1u8; 32]);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn quic_loopback_frame_roundtrip() {
        let server = QuicTransport::bind("127.0.0.1:0").await.unwrap();
        let addr_str = server.endpoint.as_ref().unwrap().local_addr().unwrap().to_string();

        let server_task = tokio::spawn(async move {
            let (mut stream, _peer) = server.accept().await.unwrap();
            let received: Hello = read_frame(&mut stream).await.unwrap();
            assert_eq!(received.node_id, [3u8; 32]);
            let ack = HelloAck {
                version: PROTOCOL_VERSION,
                node_id: [7u8; 32],
                public_key: [8u8; 32],
                x25519_pub: None,
                caps: vec![],
            };
            write_frame(&mut stream, &ack).await.unwrap();
            let mut sink = [0u8; 16];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await;
        });

        let dial = QuicTransport::dial().unwrap();
        let mut stream = dial.connect(&addr_str).await.unwrap();
        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [3u8; 32],
            public_key: [4u8; 32],
            encrypt: false,
            caps: vec![],
        };
        write_frame(&mut stream, &hello).await.unwrap();
        let got: HelloAck = read_frame(&mut stream).await.unwrap();
        assert_eq!(got.node_id, [7u8; 32]);
        server_task.await.unwrap();
    }
}
