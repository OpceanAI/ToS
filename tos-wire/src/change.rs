use serde::{Deserialize, Serialize};

use crate::error::{WireError, WireResult};
use crate::batch::BatchHeader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeOp {
    Insert = 1,
    Update = 2,
    Delete = 3,
}

impl ChangeOp {
    pub fn from_byte(b: u8) -> WireResult<Self> {
        match b {
            1 => Ok(Self::Insert),
            2 => Ok(Self::Update),
            3 => Ok(Self::Delete),
            _ => Err(WireError::InvalidFormat(b)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub change_id: [u8; 16],
    pub timestamp_ns: u64,
    pub op: ChangeOp,
    pub table: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

impl ChangeRecord {
    pub fn encode(&self) -> WireResult<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn decode(bytes: &[u8]) -> WireResult<Self> {
        Ok(bincode::deserialize(bytes)?)
    }
}

pub fn split_header_body(buf: &[u8]) -> WireResult<(BatchHeader, &[u8])> {
    use crate::BATCH_HEADER_SIZE;
    if buf.len() < BATCH_HEADER_SIZE {
        return Err(WireError::TooShort {
            min: BATCH_HEADER_SIZE,
            actual: buf.len(),
        });
    }
    let header = BatchHeader::from_bytes(&buf[..BATCH_HEADER_SIZE])?;
    Ok((header, &buf[BATCH_HEADER_SIZE..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_op_from_byte() {
        assert_eq!(ChangeOp::from_byte(1).unwrap(), ChangeOp::Insert);
        assert_eq!(ChangeOp::from_byte(2).unwrap(), ChangeOp::Update);
        assert_eq!(ChangeOp::from_byte(3).unwrap(), ChangeOp::Delete);
        assert!(ChangeOp::from_byte(0).is_err());
        assert!(ChangeOp::from_byte(99).is_err());
    }

    #[test]
    fn change_record_roundtrip() {
        let cr = ChangeRecord {
            change_id: [1u8; 16],
            timestamp_ns: 1_700_000_000_000_000_000,
            op: ChangeOp::Insert,
            table: "users".into(),
            before: None,
            after: Some(b"{}".to_vec()),
        };
        let bytes = cr.encode().unwrap();
        let parsed = ChangeRecord::decode(&bytes).unwrap();
        assert_eq!(cr, parsed);
    }

    #[test]
    fn split_header_body_correct() {
        let h = BatchHeader::default();
        let buf = [0u8; 30];
        let mut full = h.to_bytes().unwrap().to_vec();
        full.extend_from_slice(&buf[20..]);
        let (parsed_h, rest) = split_header_body(&full).unwrap();
        assert_eq!(parsed_h, h);
        assert_eq!(rest.len(), 10);
    }
}
