use serde::{Deserialize, Serialize};

use crate::error::{WireError, WireResult};
use crate::{BATCH_HEADER_SIZE, FORMAT_ARROW, FORMAT_MSGPACK};

pub const MAX_RECORDS_PER_BATCH: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchHeader {
    pub schema_hash: [u8; 32],
    pub batch_id: u32,
    pub record_count: u32,
    pub format: u8,
    pub flags: [u8; 3],
}

impl Default for BatchHeader {
    fn default() -> Self {
        Self {
            schema_hash: [0u8; 32],
            batch_id: 0,
            record_count: 0,
            format: FORMAT_MSGPACK,
            flags: [0u8; 3],
        }
    }
}

impl BatchHeader {
    pub fn with(schema_hash: [u8; 32], batch_id: u32, count: u32, format: u8) -> Self {
        Self {
            schema_hash,
            batch_id,
            record_count: count,
            format,
            flags: [0u8; 3],
        }
    }

    pub fn to_bytes(&self) -> WireResult<[u8; BATCH_HEADER_SIZE]> {
        let bytes = bincode::serialize(self)?;
        if bytes.len() != BATCH_HEADER_SIZE {
            return Err(WireError::TooShort {
                min: BATCH_HEADER_SIZE,
                actual: bytes.len(),
            });
        }
        let mut out = [0u8; BATCH_HEADER_SIZE];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> WireResult<Self> {
        if bytes.len() < BATCH_HEADER_SIZE {
            return Err(WireError::TooShort {
                min: BATCH_HEADER_SIZE,
                actual: bytes.len(),
            });
        }
        let header: Self = bincode::deserialize(&bytes[..BATCH_HEADER_SIZE])?;
        Ok(header)
    }

    pub fn validate_format(&self) -> WireResult<()> {
        match self.format {
            FORMAT_MSGPACK | FORMAT_ARROW => Ok(()),
            other => Err(WireError::InvalidFormat(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_44() {
        assert_eq!(BATCH_HEADER_SIZE, 44);
    }

    #[test]
    fn header_default_roundtrip() {
        let h = BatchHeader::default();
        let bytes = h.to_bytes().unwrap();
        assert_eq!(bytes.len(), BATCH_HEADER_SIZE);
        let h2 = BatchHeader::from_bytes(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn header_custom_values() {
        let h = BatchHeader {
            schema_hash: [0xab; 32],
            batch_id: 42,
            record_count: 1000,
            format: FORMAT_MSGPACK,
            flags: [0, 0, 0],
        };
        let bytes = h.to_bytes().unwrap();
        let h2 = BatchHeader::from_bytes(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn header_with_constructor() {
        let h = BatchHeader::with([0xab; 32], 7, 50, FORMAT_MSGPACK);
        assert_eq!(h.schema_hash, [0xab; 32]);
        assert_eq!(h.batch_id, 7);
        assert_eq!(h.record_count, 50);
        assert_eq!(h.format, FORMAT_MSGPACK);
        assert_eq!(h.flags, [0, 0, 0]);
    }

    #[test]
    fn short_buffer_errors() {
        let result = BatchHeader::from_bytes(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_format_rejected() {
        let h = BatchHeader {
            format: 0x99,
            ..Default::default()
        };
        assert!(h.validate_format().is_err());
    }
}
