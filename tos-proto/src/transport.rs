use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::ProtoResult;
use crate::messages::HelloAck;

#[async_trait]
pub trait Transport: Send + Sync {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send;

    async fn connect(&self, addr: &str) -> ProtoResult<Self::Stream>;
    async fn accept(&self) -> ProtoResult<(Self::Stream, String)>;
    async fn close(&self) -> ProtoResult<()>;
}

pub async fn exchange_hello<S>(stream: &mut S, hello: &crate::messages::Hello, is_client: bool) -> ProtoResult<HelloAck>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let payload = bincode::serialize(hello).map_err(crate::error::proto_error_from_bincode)?;
    let len = (payload.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut resp_buf = vec![0u8; resp_len];
    stream.read_exact(&mut resp_buf).await?;
    let ack: HelloAck = bincode::deserialize(&resp_buf).map_err(crate::error::proto_error_from_bincode)?;

    if is_client && ack.version != hello.version {
        return Err(crate::error::ProtoError::VersionMismatch {
            expected: hello.version,
            got: ack.version,
        });
    }

    Ok(ack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn duplex_exchange_hello() {
        let (mut a, b) = duplex(64 * 1024);
        let hello = crate::messages::Hello {
            version: 1,
            node_id: [1u8; 32],
            public_key: [2u8; 32],
            encrypt: false,
            caps: vec!["postgres".into()],
        };
        let hello_clone = hello.clone();

        let server = tokio::spawn(async move {
            let mut stream = b;
            let mut len_buf = [0u8; 4];
            tokio::io::AsyncReadExt::read_exact(&mut stream, &mut len_buf).await.unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut stream, &mut payload).await.unwrap();
            let _: crate::messages::Hello = bincode::deserialize(&payload).unwrap();
            let ack = HelloAck {
                version: hello_clone.version,
                node_id: [9u8; 32],
                public_key: [8u8; 32],
                x25519_pub: None,
                caps: hello_clone.caps.clone(),
            };
            let resp = bincode::serialize(&ack).unwrap();
            let resp_len = (resp.len() as u32).to_be_bytes();
            tokio::io::AsyncWriteExt::write_all(&mut stream, &resp_len).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut stream, &resp).await.unwrap();
            tokio::io::AsyncWriteExt::flush(&mut stream).await.unwrap();
        });

        let ack = exchange_hello(&mut a, &hello, true).await.unwrap();
        assert_eq!(ack.node_id, [9u8; 32]);
        server.await.unwrap();
    }
}
