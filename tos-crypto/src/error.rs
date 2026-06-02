use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("key generation failed: {0}")]
    Keygen(String),

    #[error("signing failed: {0}")]
    Sign(String),

    #[error("signature verification failed")]
    Verify,

    #[error("key exchange failed: {0}")]
    Exchange(String),

    #[error("encryption failed: {0}")]
    Encrypt(String),

    #[error("decryption failed: {0}")]
    Decrypt(String),

    #[error("key derivation failed: {0}")]
    Kdf(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },

    #[error("serialization error: {0}")]
    Serde(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;
