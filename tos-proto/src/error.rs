use thiserror::Error;

use tos_crypto::CryptoError;
use tos_wire::WireError;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bincode error: {0}")]
    Bincode(String),

    #[error("wire error: {0}")]
    Wire(String),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("protocol version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("handshake aborted: {0}")]
    HandshakeAborted(String),

    #[error("connection closed")]
    ConnectionClosed,
}

impl From<WireError> for ProtoError {
    fn from(e: WireError) -> Self {
        ProtoError::Wire(e.to_string())
    }
}

impl From<CryptoError> for ProtoError {
    fn from(e: CryptoError) -> Self {
        ProtoError::Crypto(e.to_string())
    }
}

pub type ProtoResult<T> = Result<T, ProtoError>;

pub fn proto_error_from_bincode(e: bincode::Error) -> ProtoError {
    ProtoError::Bincode(e.to_string())
}
