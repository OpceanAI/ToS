use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub const NODE_ID_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub version: u8,
    pub node_id: [u8; NODE_ID_SIZE],
    pub public_key: [u8; PUBLIC_KEY_SIZE],
    pub encrypt: bool,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloAck {
    pub version: u8,
    pub node_id: [u8; NODE_ID_SIZE],
    pub public_key: [u8; PUBLIC_KEY_SIZE],
    pub x25519_pub: Option<[u8; 32]>,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaOffer {
    pub sdl: Vec<u8>,
    pub schema_hash: [u8; 32],
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDiff {
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaConfirm;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamStart {
    pub session_id: [u8; 32],
    pub table: String,
    pub mode: u8,
    pub batch_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub batch_id: u32,
    pub records: Vec<u8>,
    pub batch_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack {
    pub batch_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEnd {
    pub session_id: [u8; 32],
    pub total_records: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Done {
    pub session_id: [u8; 32],
    pub total_records: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Hello(Hello),
    HelloAck(HelloAck),
    SchemaOffer(SchemaOffer),
    SchemaDiff(SchemaDiff),
    SchemaConfirm(SchemaConfirm),
    StreamStart(StreamStart),
    Batch(Batch),
    Ack(Ack),
    StreamEnd(StreamEnd),
    Done(Done),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip() {
        let h = Hello {
            version: PROTOCOL_VERSION,
            node_id: [1u8; NODE_ID_SIZE],
            public_key: [2u8; PUBLIC_KEY_SIZE],
            encrypt: false,
            caps: vec!["postgres".into(), "redis".into()],
        };
        let bytes = bincode::serialize(&h).unwrap();
        let h2: Hello = bincode::deserialize(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn batch_roundtrip() {
        let b = Batch {
            batch_id: 7,
            records: vec![0xab; 100],
            batch_hash: [0u8; 32],
            signature: vec![0u8; 64],
            count: 10,
        };
        let bytes = bincode::serialize(&b).unwrap();
        let b2: Batch = bincode::deserialize(&bytes).unwrap();
        assert_eq!(b, b2);
    }

    #[test]
    fn message_enum_roundtrip() {
        let m = Message::Ack(Ack { batch_id: 42 });
        let bytes = bincode::serialize(&m).unwrap();
        let m2: Message = bincode::deserialize(&bytes).unwrap();
        assert_eq!(m, m2);
    }
}
