use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bincode error: {0}")]
    Bincode(String),

    #[error("protocol version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("handshake aborted: {0}")]
    HandshakeAborted(String),

    #[error("connection closed")]
    ConnectionClosed,
}

pub type ProtoResult<T> = Result<T, ProtoError>;

pub fn proto_error_from_bincode(e: bincode::Error) -> ProtoError {
    ProtoError::Bincode(e.to_string())
}
