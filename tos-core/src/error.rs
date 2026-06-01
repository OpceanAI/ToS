use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("parse error at line {line}, column {col}: {msg}")]
    Parse { line: usize, col: usize, msg: String },

    #[error("validation failed: {0:?}")]
    Validation(Vec<String>),

    #[error("inference error: {0}")]
    Inference(String),

    #[error("resolve error: cannot map {from} to {to}: {reason}")]
    Resolve { from: String, to: String, reason: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("toml deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
