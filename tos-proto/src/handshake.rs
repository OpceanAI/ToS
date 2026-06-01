use crate::error::ProtoResult;
use crate::messages::Hello;
use crate::transport::exchange_hello;

pub struct Handshake {
    pub hello: Hello,
}

impl Handshake {
    pub fn new(hello: Hello) -> Self {
        Self { hello }
    }

    pub async fn perform<S>(&self, stream: &mut S) -> ProtoResult<crate::messages::HelloAck>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        exchange_hello(stream, &self.hello, true).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::HelloAck;
    use tokio::io::duplex;

    #[tokio::test]
    async fn handshake_duplex() {
        let (mut a, mut b) = duplex(64 * 1024);
        let hello = Hello {
            version: 1,
            node_id: [3u8; 32],
            public_key: [4u8; 32],
            encrypt: false,
            caps: vec![],
        };
        let hello_for_server = hello.clone();

        let server = tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            tokio::io::AsyncReadExt::read_exact(&mut b, &mut len_buf).await.unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut b, &mut payload).await.unwrap();
            let received: Hello = bincode::deserialize(&payload).unwrap();
            assert_eq!(received, hello_for_server);
            let ack = HelloAck {
                version: 1,
                node_id: [5u8; 32],
                public_key: [6u8; 32],
                x25519_pub: None,
                caps: vec![],
            };
            let resp = bincode::serialize(&ack).unwrap();
            let resp_len = (resp.len() as u32).to_be_bytes();
            tokio::io::AsyncWriteExt::write_all(&mut b, &resp_len).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut b, &resp).await.unwrap();
        });

        let hs = Handshake::new(hello);
        let ack = hs.perform(&mut a).await.unwrap();
        assert_eq!(ack.node_id, [5u8; 32]);
        server.await.unwrap();
    }
}
