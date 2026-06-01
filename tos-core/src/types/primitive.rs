use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimitiveType {
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Decimal { precision: u8, scale: u8 },
    Text { max: Option<u32> },
    Bytes { max: Option<u32> },
    Uuid,
    Timestamp { with_tz: bool },
    Date,
    Time,
    Duration,
    Any,
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::Bool => write!(f, "bool"),
            PrimitiveType::Int8 => write!(f, "int8"),
            PrimitiveType::Int16 => write!(f, "int16"),
            PrimitiveType::Int32 => write!(f, "int32"),
            PrimitiveType::Int64 => write!(f, "int64"),
            PrimitiveType::Uint8 => write!(f, "uint8"),
            PrimitiveType::Uint16 => write!(f, "uint16"),
            PrimitiveType::Uint32 => write!(f, "uint32"),
            PrimitiveType::Uint64 => write!(f, "uint64"),
            PrimitiveType::Float32 => write!(f, "float32"),
            PrimitiveType::Float64 => write!(f, "float64"),
            PrimitiveType::Decimal { precision, scale } => {
                write!(f, "decimal({precision},{scale})")
            }
            PrimitiveType::Text { max: Some(m) } => write!(f, "text({m})"),
            PrimitiveType::Text { max: None } => write!(f, "text"),
            PrimitiveType::Bytes { max: Some(m) } => write!(f, "bytes({m})"),
            PrimitiveType::Bytes { max: None } => write!(f, "bytes"),
            PrimitiveType::Uuid => write!(f, "uuid"),
            PrimitiveType::Timestamp { with_tz: true } => write!(f, "timestamp(tz)"),
            PrimitiveType::Timestamp { with_tz: false } => write!(f, "timestamp"),
            PrimitiveType::Date => write!(f, "date"),
            PrimitiveType::Time => write!(f, "time"),
            PrimitiveType::Duration => write!(f, "duration"),
            PrimitiveType::Any => write!(f, "any"),
        }
    }
}

impl PrimitiveType {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        match s {
            "bool" => Some(Self::Bool),
            "int8" | "i8" => Some(Self::Int8),
            "int16" | "i16" => Some(Self::Int16),
            "int32" | "i32" => Some(Self::Int32),
            "int64" | "i64" => Some(Self::Int64),
            "uint8" | "u8" => Some(Self::Uint8),
            "uint16" | "u16" => Some(Self::Uint16),
            "uint32" | "u32" => Some(Self::Uint32),
            "uint64" | "u64" => Some(Self::Uint64),
            "float32" | "f32" => Some(Self::Float32),
            "float64" | "f64" => Some(Self::Float64),
            "uuid" => Some(Self::Uuid),
            "timestamp" => Some(Self::Timestamp { with_tz: false }),
            "timestamp(tz)" | "timestamptz" => Some(Self::Timestamp { with_tz: true }),
            "date" => Some(Self::Date),
            "time" => Some(Self::Time),
            "duration" => Some(Self::Duration),
            "any" => Some(Self::Any),
            "text" => Some(Self::Text { max: None }),
            "bytes" => Some(Self::Bytes { max: None }),
            _ => {
                if let Some(rest) = s.strip_prefix("text(").and_then(|r| r.strip_suffix(')')) {
                    return rest.parse().ok().map(|m| Self::Text { max: Some(m) });
                }
                if let Some(rest) = s.strip_prefix("bytes(").and_then(|r| r.strip_suffix(')')) {
                    return rest.parse().ok().map(|m| Self::Bytes { max: Some(m) });
                }
                if let Some(rest) = s
                    .strip_prefix("decimal(")
                    .and_then(|r| r.strip_suffix(')'))
                {
                    let mut parts = rest.split(',');
                    let p = parts.next()?.trim().parse().ok()?;
                    let sc = parts.next()?.trim().parse().ok()?;
                    return Some(Self::Decimal {
                        precision: p,
                        scale: sc,
                    });
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_roundtrip_simple() {
        let cases = [
            (PrimitiveType::Bool, "bool"),
            (PrimitiveType::Int32, "int32"),
            (PrimitiveType::Uint64, "uint64"),
            (PrimitiveType::Uuid, "uuid"),
            (PrimitiveType::Date, "date"),
        ];
        for (t, s) in cases {
            assert_eq!(t.to_string(), s);
            assert_eq!(PrimitiveType::parse(s), Some(t));
        }
    }

    #[test]
    fn display_roundtrip_parameterized() {
        let t = PrimitiveType::Decimal {
            precision: 10,
            scale: 2,
        };
        assert_eq!(t.to_string(), "decimal(10,2)");
        assert_eq!(
            PrimitiveType::parse("decimal(10,2)"),
            Some(PrimitiveType::Decimal {
                precision: 10,
                scale: 2
            })
        );

        let t = PrimitiveType::Text { max: Some(255) };
        assert_eq!(t.to_string(), "text(255)");
        assert_eq!(PrimitiveType::parse("text(255)"), Some(t));
    }

    #[test]
    fn timestamp_with_tz() {
        let t = PrimitiveType::Timestamp { with_tz: true };
        assert_eq!(t.to_string(), "timestamp(tz)");
        assert_eq!(PrimitiveType::parse("timestamp(tz)"), Some(t));
        assert_eq!(
            PrimitiveType::parse("timestamptz"),
            Some(PrimitiveType::Timestamp { with_tz: true })
        );
    }

    #[test]
    fn from_str_unknown() {
        assert_eq!(PrimitiveType::parse("nonsense"), None);
        assert_eq!(PrimitiveType::parse("decimal(abc,2)"), None);
    }
}
