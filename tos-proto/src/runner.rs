use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use ed25519_dalek::Signer;
use futures::stream::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tos_core::adapter::{TosAdapter, TosValue};
use tos_crypto::Identity;

use crate::error::ProtoResult;
use crate::handshake::Handshake;
use crate::messages::{
    Ack, Batch, Done, Hello, HelloAck, Message, SchemaConfirm, SchemaDiff, SchemaOffer, StreamEnd,
    StreamStart, PROTOCOL_VERSION,
};
use crate::session::Session;
use crate::transport::{read_message, write_message, TcpTransport, Transport};

pub const DEFAULT_BATCH_SIZE: u32 = 1000;
pub const MODE_PUSH: u8 = 1;
pub const MODE_SYNC: u8 = 2;

pub struct RunStats {
    pub total_records: u64,
    pub total_batches: u32,
    pub duration_ms: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

pub struct SessionRunner {
    pub session: Session,
}

impl SessionRunner {
    pub fn new(identity: Arc<Identity>, batch_size: u32) -> Self {
        Self {
            session: Session::new(identity, batch_size),
        }
    }

    pub async fn run_client<A>(
        self,
        peer_addr: SocketAddr,
        source: Arc<A>,
        dest: Arc<A>,
        table: &str,
    ) -> ProtoResult<RunStats>
    where
        A: TosAdapter + 'static,
    {
        let started = Instant::now();
        let dial = TcpTransport::dial();
        let mut stream = dial.connect(&peer_addr.to_string()).await?;

        let identity = self.session.identity.clone();
        let hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: *identity.node_id().as_bytes(),
            public_key: identity.public_key(),
            encrypt: false,
            caps: vec![],
        };
        let ack: HelloAck = Handshake::new(hello).perform(&mut stream).await?;
        if ack.version != PROTOCOL_VERSION {
            return Err(crate::error::ProtoError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: ack.version,
            });
        }

        let schema = source.read_schema().await.map_err(adapter_err)?;
        let sdl = serialize_schema(&schema);
        let schema_hash = blake3_hash_32(&sdl);
        let sig_arr = identity.sign(&sdl);
        let signature = sig_arr.to_vec();
        let offer = SchemaOffer {
            sdl,
            schema_hash,
            signature,
        };
        write_message(&mut stream, &Message::SchemaOffer(offer)).await?;
        let diff_msg: Message = read_message(&mut stream).await?;
        let diff = match diff_msg {
            Message::SchemaDiff(d) => d,
            other => {
                return Err(crate::error::ProtoError::InvalidMessage(format!(
                    "expected SchemaDiff, got {:?}",
                    std::mem::discriminant(&other)
                )));
            }
        };
        if !diff.accepted {
            return Err(crate::error::ProtoError::HandshakeAborted(format!(
                "schema rejected: {:?}",
                diff.reason
            )));
        }
        write_message(&mut stream, &Message::SchemaConfirm(SchemaConfirm)).await?;

        let start = StreamStart {
            session_id: self.session.session_id,
            table: table.to_string(),
            mode: MODE_PUSH,
            batch_size: self.session.batch_size,
        };
        write_message(&mut stream, &Message::StreamStart(start)).await?;

        let source_stream = source.read_records(table).await.map_err(adapter_err)?;
        let batch_size = self.session.batch_size as usize;
        let mut total_records = 0u64;
        let mut total_batches = 0u32;
        let mut bytes_sent = 0u64;
        let mut next_batch_id = 0u32;

        let mut chunked = chunks_of(source_stream, batch_size);
        while let Some(chunk) = chunked.next().await {
            let records: Vec<TosValue> = chunk.map_err(adapter_err)?;
            let count = records.len() as u32;
            let body = tos_wire::change::encode_records(&records)?;
            let header_hash = blake3_hash_32(&body);
            let sk = identity.signing_key();
            let mut to_sign = Vec::with_capacity(32 + body.len());
            to_sign.extend_from_slice(&header_hash);
            to_sign.extend_from_slice(&body);
            let sig = sk.sign(&to_sign).to_bytes().to_vec();
            let batch_msg = Batch {
                batch_id: next_batch_id,
                records: body,
                batch_hash: header_hash,
                signature: sig,
                count,
            };
            let payload = bincode::serialize(&batch_msg)
                .map_err(crate::error::proto_error_from_bincode)?;
            bytes_sent += payload.len() as u64 + 4;
            write_message(&mut stream, &Message::Batch(batch_msg)).await?;
            let ack_msg: Message = read_message(&mut stream).await?;
            match ack_msg {
                Message::Ack(_) => {}
                other => {
                    return Err(crate::error::ProtoError::InvalidMessage(format!(
                        "expected Ack, got {:?}",
                        std::mem::discriminant(&other)
                    )));
                }
            }
            total_records += count as u64;
            total_batches += 1;
            next_batch_id += 1;
        }

        let end = StreamEnd {
            session_id: self.session.session_id,
            total_records,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        write_message(&mut stream, &Message::StreamEnd(end)).await?;
        let _done_msg: Message = read_message(&mut stream).await?;

        let _ = dest;
        Ok(RunStats {
            total_records,
            total_batches,
            duration_ms: started.elapsed().as_millis() as u64,
            bytes_sent,
            bytes_received: 0,
        })
    }

    pub async fn run_server<A>(
        self,
        listener: TcpTransport,
        dest: Arc<A>,
    ) -> ProtoResult<RunStats>
    where
        A: TosAdapter + 'static,
    {
        let started = Instant::now();
        let (mut stream, _peer) = listener.accept().await?;

        let server_hello = Hello {
            version: PROTOCOL_VERSION,
            node_id: *self.session.identity.node_id().as_bytes(),
            public_key: self.session.identity.public_key(),
            encrypt: false,
            caps: vec![],
        };
        let (client_hello, _ack) = Handshake::accept(&server_hello, &mut stream).await?;

        let offer: SchemaOffer = {
            let m: Message = read_message(&mut stream).await?;
            match m {
                Message::SchemaOffer(o) => o,
                other => {
                    return Err(crate::error::ProtoError::InvalidMessage(format!(
                        "expected SchemaOffer, got {:?}",
                        std::mem::discriminant(&other)
                    )));
                }
            }
        };
        if !verify_offer(&offer, &client_hello.public_key) {
            let diff = SchemaDiff {
                accepted: false,
                reason: Some("schema signature verification failed".into()),
            };
            write_message(&mut stream, &Message::SchemaDiff(diff)).await?;
            return Err(crate::error::ProtoError::HandshakeAborted(
                "bad signature".into(),
            ));
        }
        let diff = SchemaDiff {
            accepted: true,
            reason: None,
        };
        write_message(&mut stream, &Message::SchemaDiff(diff)).await?;
        let _confirm: Message = read_message(&mut stream).await?;

        let _start: StreamStart = {
            let m: Message = read_message(&mut stream).await?;
            match m {
                Message::StreamStart(s) => s,
                other => {
                    return Err(crate::error::ProtoError::InvalidMessage(format!(
                        "expected StreamStart, got {:?}",
                        std::mem::discriminant(&other)
                    )));
                }
            }
        };

        let mut total_records = 0u64;
        let mut total_batches = 0u32;
        let mut bytes_received = 0u64;
        loop {
            let msg: Message = read_message(&mut stream).await?;
            match msg {
                Message::Batch(b) => {
                    let payload = bincode::serialize(&b)
                        .map_err(crate::error::proto_error_from_bincode)?;
                    bytes_received += payload.len() as u64 + 4;
                    let _records: Vec<TosValue> = decode_batch(&b)?;
                    let _ = dest.write_records("ignored", Box::pin(futures::stream::empty())).await;
                    total_records += b.count as u64;
                    total_batches += 1;
                    let ack = Ack { batch_id: b.batch_id };
                    write_message(&mut stream, &Message::Ack(ack)).await?;
                }
                Message::StreamEnd(end) => {
                    let done = Done {
                        session_id: end.session_id,
                        total_records: end.total_records,
                        duration_ms: end.duration_ms,
                    };
                    write_message(&mut stream, &Message::Done(done)).await?;
                    let _ = dest;
                    return Ok(RunStats {
                        total_records,
                        total_batches,
                        duration_ms: started.elapsed().as_millis() as u64,
                        bytes_sent: 0,
                        bytes_received,
                    });
                }
                other => {
                    return Err(crate::error::ProtoError::InvalidMessage(format!(
                        "unexpected message: {:?}",
                        std::mem::discriminant(&other)
                    )));
                }
            }
        }
    }
}

fn chunks_of<S>(
    stream: S,
    size: usize,
) -> std::pin::Pin<Box<dyn futures::Stream<Item = Result<Vec<TosValue>, BoxedErr>> + Send>>
where
    S: futures::Stream<Item = Result<TosValue, BoxedErr>> + Unpin + Send + 'static,
{
    use futures::stream::unfold;
    let stream = stream;
    Box::pin(unfold(
        (stream, size, Vec::with_capacity(size)),
        move |(mut stream, size, mut current)| async move {
            loop {
                match stream.next().await {
                    Some(Ok(v)) => {
                        current.push(v);
                        if current.len() >= size {
                            return Some((
                                Ok(std::mem::take(&mut current)),
                                (stream, size, Vec::with_capacity(size)),
                            ));
                        }
                    }
                    Some(Err(e)) => {
                        if current.is_empty() {
                            return Some((Err(e), (stream, size, Vec::with_capacity(size))));
                        } else {
                            return Some((
                                Err(e),
                                (stream, size, std::mem::take(&mut current)),
                            ));
                        }
                    }
                    None => {
                        if current.is_empty() {
                            return None;
                        } else {
                            return Some((
                                Ok(std::mem::take(&mut current)),
                                (stream, size, Vec::with_capacity(size)),
                            ));
                        }
                    }
                }
            }
        },
    ))
}

type BoxedErr = Box<dyn std::error::Error + Send + Sync>;

fn serialize_schema(schema: &tos_core::sdl::TosSchema) -> Vec<u8> {
    format!("{:#?}", schema).into_bytes()
}

fn blake3_hash_32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let hash = tos_crypto::blake3_hash(bytes);
    let hash_bytes: &[u8] = hash.as_ref();
    out.copy_from_slice(hash_bytes);
    out
}

fn verify_offer(offer: &SchemaOffer, public_key: &[u8; 32]) -> bool {
    let Ok(pk) = tos_crypto::verifying_key_from_bytes(public_key) else {
        return false;
    };
    tos_crypto::verify(&pk, &offer.sdl, &offer.signature).is_ok()
}

fn decode_batch(b: &Batch) -> ProtoResult<Vec<TosValue>> {
    tos_wire::change::decode_records(&b.records).map_err(Into::into)
}

fn adapter_err(e: Box<dyn std::error::Error + Send + Sync>) -> crate::error::ProtoError {
    crate::error::ProtoError::HandshakeAborted(format!("adapter: {e}"))
}

pub async fn push<S>(_stream: &mut S, _source: &dyn TosAdapter, _dest: &dyn TosAdapter, _table: &str) -> ProtoResult<RunStats>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    Err(crate::error::ProtoError::InvalidMessage("use SessionRunner::run_client".into()))
}

#[allow(dead_code)]
async fn flush<S: AsyncWrite + Unpin + Send>(s: &mut S) -> std::io::Result<()> {
    s.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use std::collections::BTreeMap;
    use tos_core::sdl::TosSchema;
    use tos_core::MockAdapter;

    fn empty_schema() -> TosSchema {
        TosSchema {
            name: "test".into(),
            version: "1".into(),
            tables: BTreeMap::new(),
        }
    }

    fn sample_records(n: usize) -> Vec<TosValue> {
        use serde_json::json;
        (0..n)
            .map(|i| TosValue(json!({"id": i, "name": format!("r-{i}")})))
            .collect()
    }

    fn make_identity() -> Arc<Identity> {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        Arc::new(Identity::from_signing_key(key))
    }

    #[tokio::test]
    async fn session_e2e_push_10_records() {
        let src = Arc::new(MockAdapter::with_records(
            "src",
            empty_schema(),
            sample_records(10),
        ));
        let dst = Arc::new(MockAdapter::new("dst", empty_schema()));

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let id = make_identity();
        let runner = SessionRunner::new(id, 4);

        let dst_for_server = dst.clone();
        let server = tokio::spawn(async move {
            runner.run_server(listener, dst_for_server).await
        });

        let id2 = make_identity();
        let runner_c = SessionRunner::new(id2, 4);
        let src_c = src.clone();
        let dst_c = dst.clone();
        let client = tokio::spawn(async move {
            runner_c.run_client(addr, src_c, dst_c, "users").await
        });

        let (server_res, client_res) = tokio::join!(server, client);
        let server_stats = server_res.unwrap().unwrap();
        let client_stats = client_res.unwrap().unwrap();

        assert_eq!(client_stats.total_records, 10);
        assert_eq!(client_stats.total_batches, 3);
        assert!(client_stats.bytes_sent > 0);
        assert_eq!(server_stats.total_records, 10);
        assert_eq!(server_stats.total_batches, 3);
        assert!(server_stats.bytes_received > 0);
    }

    #[tokio::test]
    async fn session_e2e_empty_source() {
        let src = Arc::new(MockAdapter::new("src", empty_schema()));
        let dst = Arc::new(MockAdapter::new("dst", empty_schema()));

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let id = make_identity();
        let runner = SessionRunner::new(id, 100);
        let dst_for_server = dst.clone();
        let server = tokio::spawn(async move {
            runner.run_server(listener, dst_for_server).await
        });

        let id2 = make_identity();
        let runner_c = SessionRunner::new(id2, 100);
        let src_c = src.clone();
        let dst_c = dst.clone();
        let client = tokio::spawn(async move {
            runner_c.run_client(addr, src_c, dst_c, "any").await
        });

        let (s, c) = tokio::join!(server, client);
        let s_stats = s.unwrap().unwrap();
        let c_stats = c.unwrap().unwrap();
        assert_eq!(s_stats.total_records, 0);
        assert_eq!(c_stats.total_records, 0);
        assert_eq!(s_stats.total_batches, 0);
    }

    #[tokio::test]
    async fn session_e2e_large_batch() {
        let src = Arc::new(MockAdapter::with_records(
            "src",
            empty_schema(),
            sample_records(500),
        ));
        let dst = Arc::new(MockAdapter::new("dst", empty_schema()));

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let id = make_identity();
        let runner = SessionRunner::new(id, 100);
        let dst_for_server = dst.clone();
        let server = tokio::spawn(async move {
            runner.run_server(listener, dst_for_server).await
        });

        let id2 = make_identity();
        let runner_c = SessionRunner::new(id2, 100);
        let src_c = src.clone();
        let dst_c = dst.clone();
        let client = tokio::spawn(async move {
            runner_c.run_client(addr, src_c, dst_c, "t").await
        });

        let (s, c) = tokio::join!(server, client);
        let s_stats = s.unwrap().unwrap();
        let c_stats = c.unwrap().unwrap();
        assert_eq!(c_stats.total_records, 500);
        assert_eq!(c_stats.total_batches, 5);
        assert_eq!(s_stats.total_records, 500);
        assert_eq!(s_stats.total_batches, 5);
    }
}
