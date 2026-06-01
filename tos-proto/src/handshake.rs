use crate::error::ProtoResult;
use crate::messages::{Hello, HelloAck};
use crate::transport::{exchange_hello, write_frame};

pub struct Handshake {
    pub hello: Hello,
}

impl Handshake {
    pub fn new(hello: Hello) -> Self {
        Self { hello }
    }

    pub async fn perform<S>(self, stream: &mut S) -> ProtoResult<HelloAck>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        exchange_hello(stream, &self.hello, true).await
    }

    pub async fn accept<S>(hello: &Hello, stream: &mut S) -> ProtoResult<(Hello, HelloAck)>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let (received, ack) = exchange_hello_full(stream, hello).await?;
        Ok((received, ack))
    }

    pub async fn send<S>(&self, stream: &mut S) -> ProtoResult<()>
    where
        S: tokio::io::AsyncWrite + Unpin + Send,
    {
        write_frame(stream, &self.hello).await
    }
}

async fn exchange_hello_full<S>(
    stream: &mut S,
    server_hello: &Hello,
) -> ProtoResult<(Hello, HelloAck)>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    use crate::transport::read_frame;
    let received: Hello = read_frame(stream).await?;
    let ack = HelloAck {
        version: server_hello.version,
        node_id: server_hello.node_id,
        public_key: server_hello.public_key,
        x25519_pub: None,
        caps: server_hello.caps.clone(),
    };
    write_frame(stream, &ack).await?;
    Ok((received, ack))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{Hello, PROTOCOL_VERSION};
    use crate::transport::read_frame;
    use tokio::io::duplex;

    #[tokio::test]
    async fn handshake_duplex() {
        let (mut a, mut b) = duplex(64 * 1024);
        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [3u8; 32],
            public_key: [4u8; 32],
            encrypt: false,
            caps: vec![],
        };
        let hello_for_server = hello.clone();

        let server = tokio::spawn(async move {
            let (received, _ack) = Handshake::accept(&hello_for_server, &mut b).await.unwrap();
            assert_eq!(received.public_key, [4u8; 32]);
        });

        let hs = Handshake::new(hello);
        let ack = hs.perform(&mut a).await.unwrap();
        assert_eq!(ack.public_key, [4u8; 32]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_send_only() {
        let (mut a, mut b) = duplex(64 * 1024);
        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: [7u8; 32],
            public_key: [8u8; 32],
            encrypt: false,
            caps: vec![],
        };
        let server = tokio::spawn(async move {
            let got: Hello = read_frame(&mut b).await.unwrap();
            assert_eq!(got.public_key, [8u8; 32]);
        });
        Handshake::new(hello).send(&mut a).await.unwrap();
        server.await.unwrap();
    }

    #[test]
    fn handshake_new_stores_hello() {
        let hello = Hello {
            version: 1,
            node_id: [0u8; 32],
            public_key: [0u8; 32],
            encrypt: false,
            caps: vec!["json".into()],
        };
        let hs = Handshake::new(hello.clone());
        assert_eq!(hs.hello, hello);
    }
}
