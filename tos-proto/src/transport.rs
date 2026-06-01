use std::net::SocketAddr;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::error::{ProtoError, ProtoResult};
use crate::messages::{Hello, HelloAck, PROTOCOL_VERSION};

#[async_trait]
pub trait Transport: Send + Sync {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send;

    async fn connect(&self, addr: &str) -> ProtoResult<Self::Stream>;
    async fn accept(&self) -> ProtoResult<(Self::Stream, String)>;
    async fn close(&mut self) -> ProtoResult<()>;
}

pub struct TcpTransport {
    listener: Option<TcpListener>,
    listener_addr: Option<SocketAddr>,
    closed: Mutex<bool>,
}

impl TcpTransport {
    pub async fn bind(addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let listener_addr = listener.local_addr()?;
        Ok(Self {
            listener: Some(listener),
            listener_addr: Some(listener_addr),
            closed: Mutex::new(false),
        })
    }

    pub fn dial() -> Self {
        Self {
            listener: None,
            listener_addr: None,
            closed: Mutex::new(false),
        }
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.listener_addr
    }
}

#[async_trait]
impl Transport for TcpTransport {
    type Stream = TcpStream;

    async fn connect(&self, addr: &str) -> ProtoResult<Self::Stream> {
        let stream = TcpStream::connect(addr).await?;
        Ok(stream)
    }

    async fn accept(&self) -> ProtoResult<(Self::Stream, String)> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| ProtoError::HandshakeAborted("no listener bound".into()))?;
        let (stream, peer) = listener.accept().await?;
        Ok((stream, peer.to_string()))
    }

    async fn close(&mut self) -> ProtoResult<()> {
        let mut closed = self.closed.lock().await;
        if !*closed {
            *closed = true;
            self.listener = None;
        }
        Ok(())
    }
}

pub async fn write_frame<W, T>(w: &mut W, msg: &T) -> ProtoResult<()>
where
    W: AsyncWrite + Unpin + Send,
    T: Serialize + ?Sized,
{
    let payload = bincode::serialize(msg).map_err(crate::error::proto_error_from_bincode)?;
    let len = (payload.len() as u32).to_be_bytes();
    w.write_all(&len).await?;
    w.write_all(&payload).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(r: &mut R) -> ProtoResult<T>
where
    R: AsyncRead + Unpin + Send,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await?;
    let value = bincode::deserialize(&payload).map_err(crate::error::proto_error_from_bincode)?;
    Ok(value)
}

pub async fn write_message<W>(w: &mut W, msg: &crate::messages::Message) -> ProtoResult<()>
where
    W: AsyncWrite + Unpin + Send,
{
    write_frame(w, msg).await
}

pub async fn read_message<R>(r: &mut R) -> ProtoResult<crate::messages::Message>
where
    R: AsyncRead + Unpin + Send,
{
    read_frame(r).await
}

pub async fn exchange_hello<S>(stream: &mut S, hello: &Hello, is_client: bool) -> ProtoResult<HelloAck>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    if is_client {
        write_frame(stream, hello).await?;
        let ack: HelloAck = read_frame(stream).await?;
        if ack.version != hello.version && ack.version != PROTOCOL_VERSION {
            return Err(ProtoError::VersionMismatch {
                expected: hello.version,
                got: ack.version,
            });
        }
        Ok(ack)
    } else {
        let received: Hello = read_frame(stream).await?;
        let ack = HelloAck {
            version: PROTOCOL_VERSION,
            node_id: hello.node_id,
            public_key: hello.public_key,
            x25519_pub: None,
            caps: hello.caps.clone(),
        };
        let _ = received;
        write_frame(stream, &ack).await?;
        Ok(ack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{Batch, SchemaOffer};
    use tokio::io::duplex;

    #[tokio::test]
    async fn frame_roundtrip_hello() {
        let (mut a, mut b) = duplex(64 * 1024);
        let h = Hello {
            version: 1,
            node_id: [1u8; 32],
            public_key: [2u8; 32],
            encrypt: false,
            caps: vec!["postgres".into()],
        };
        let h_clone = h.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &h_clone).await.unwrap();
        });
        let got: Hello = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, h);
    }

    #[tokio::test]
    async fn frame_roundtrip_schema_offer() {
        let (mut a, mut b) = duplex(64 * 1024);
        let so = SchemaOffer {
            sdl: vec![1, 2, 3, 4, 5],
            schema_hash: [0xab; 32],
            signature: vec![0u8; 64],
        };
        let so_clone = so.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &so_clone).await.unwrap();
        });
        let got: SchemaOffer = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, so);
    }

    #[tokio::test]
    async fn frame_roundtrip_batch() {
        let (mut a, mut b) = duplex(64 * 1024);
        let b_orig = Batch {
            batch_id: 7,
            records: vec![0xab; 256],
            batch_hash: [0u8; 32],
            signature: vec![0u8; 64],
            count: 4,
        };
        let b_clone = b_orig.clone();
        let writer = tokio::spawn(async move {
            write_frame(&mut a, &b_clone).await.unwrap();
        });
        let got: Batch = read_frame(&mut b).await.unwrap();
        writer.await.unwrap();
        assert_eq!(got, b_orig);
    }

    #[tokio::test]
    async fn frame_too_short_errors() {
        let (mut a, mut b) = duplex(64 * 1024);
        let writer = tokio::spawn(async move {
            a.write_all(&[0u8, 0u8, 0u8, 5u8]).await.unwrap();
            a.write_all(&[1, 2]).await.unwrap();
            a.shutdown().await.ok();
        });
        let res: ProtoResult<u64> = read_frame(&mut b).await;
        assert!(res.is_err());
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_loopback_exchange_hello() {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [1u8; 32],
            public_key: [2u8; 32],
            encrypt: false,
            caps: vec![],
        };

        let server = tokio::spawn(async move {
            let (mut stream, _peer) = listener.accept().await.unwrap();
            let _: Hello = read_frame(&mut stream).await.unwrap();
            let ack = HelloAck {
                version: PROTOCOL_VERSION,
                node_id: [9u8; 32],
                public_key: [8u8; 32],
                x25519_pub: None,
                caps: vec![],
            };
            write_frame(&mut stream, &ack).await.unwrap();
        });

        let dial = TcpTransport::dial();
        let mut stream = dial.connect(&addr.to_string()).await.unwrap();
        let ack = exchange_hello(&mut stream, &hello, true).await.unwrap();
        assert_eq!(ack.node_id, [9u8; 32]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_loopback_handshake() {
        use crate::handshake::Handshake;

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [3u8; 32],
            public_key: [4u8; 32],
            encrypt: false,
            caps: vec![],
        };
        let server_hello = hello.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _peer) = listener.accept().await.unwrap();
            let _ = exchange_hello(&mut stream, &server_hello, false).await.unwrap();
        });

        let dial = TcpTransport::dial();
        let mut stream = dial.connect(&addr.to_string()).await.unwrap();
        let hs = Handshake::new(hello);
        let ack = hs.perform(&mut stream).await.unwrap();
        assert_eq!(ack.public_key, [4u8; 32]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_to_unreachable_errors() {
        let dial = TcpTransport::dial();
        let res = dial.connect("127.0.0.1:1").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn dial_has_no_listener_addr() {
        let dial = TcpTransport::dial();
        assert!(dial.local_addr().is_none());
    }

    #[tokio::test]
    async fn accept_without_listener_errors() {
        let dial = TcpTransport::dial();
        let res = dial.accept().await;
        assert!(res.is_err());
    }
}
