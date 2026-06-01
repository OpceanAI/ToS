use thiserror::Error;

#[derive(Debug, Error)]
pub enum WireError {
    #[error("bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("rmp_serde encode error: {0}")]
    MsgpackEncode(String),

    #[error("rmp_serde decode error: {0}")]
    MsgpackDecode(String),

    #[error("batch too short: {actual} bytes, minimum {min}")]
    TooShort { min: usize, actual: usize },

    #[error("invalid format byte: 0x{0:02x}")]
    InvalidFormat(u8),

    #[error("schema hash mismatch: expected {expected}, got {actual}")]
    SchemaHashMismatch { expected: String, actual: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type WireResult<T> = Result<T, WireError>;
