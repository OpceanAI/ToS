use serde::{Deserialize, Serialize};
use std::fmt;

pub mod compound;
pub mod primitive;

pub use compound::CompoundType;
pub use primitive::PrimitiveType;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TosType {
    Primitive(PrimitiveType),
    Compound(CompoundType),
}

impl Serialize for TosType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            TosType::Primitive(p) => p.serialize(serializer),
            TosType::Compound(c) => c.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TosType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Ok(p) = serde_json::from_value::<PrimitiveType>(value.clone()) {
            return Ok(TosType::Primitive(p));
        }
        if let Ok(c) = serde_json::from_value::<CompoundType>(value) {
            return Ok(TosType::Compound(c));
        }
        Err(D::Error::custom("value is neither a valid primitive nor compound TosType"))
    }
}

impl TosType {
    pub fn is_optional(&self) -> bool {
        matches!(
            self,
            TosType::Compound(CompoundType::Optional(inner)) if matches!(inner.as_ref(), TosType::Primitive(PrimitiveType::Any))
        ) || matches!(
            self,
            TosType::Compound(CompoundType::Optional(_))
        )
    }

    pub fn unwrap_optional(&self) -> &TosType {
        match self {
            TosType::Compound(CompoundType::Optional(inner)) => inner.as_ref(),
            other => other,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            TosType::Primitive(
                PrimitiveType::Int8
                    | PrimitiveType::Int16
                    | PrimitiveType::Int32
                    | PrimitiveType::Int64
                    | PrimitiveType::Uint8
                    | PrimitiveType::Uint16
                    | PrimitiveType::Uint32
                    | PrimitiveType::Uint64
                    | PrimitiveType::Float32
                    | PrimitiveType::Float64
                    | PrimitiveType::Decimal { .. }
            )
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            TosType::Primitive(
                PrimitiveType::Int8
                    | PrimitiveType::Int16
                    | PrimitiveType::Int32
                    | PrimitiveType::Int64
                    | PrimitiveType::Uint8
                    | PrimitiveType::Uint16
                    | PrimitiveType::Uint32
                    | PrimitiveType::Uint64
            )
        )
    }
}

impl fmt::Display for TosType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TosType::Primitive(p) => write!(f, "{p}"),
            TosType::Compound(c) => write!(f, "{c}"),
        }
    }
}

impl From<PrimitiveType> for TosType {
    fn from(p: PrimitiveType) -> Self {
        TosType::Primitive(p)
    }
}

impl From<CompoundType> for TosType {
    fn from(c: CompoundType) -> Self {
        TosType::Compound(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_types_detected() {
        assert!(TosType::from(PrimitiveType::Int32).is_integer());
        assert!(TosType::from(PrimitiveType::Uint64).is_integer());
        assert!(!TosType::from(PrimitiveType::Float64).is_integer());
    }

    #[test]
    fn numeric_types_detected() {
        assert!(TosType::from(PrimitiveType::Int32).is_numeric());
        assert!(TosType::from(PrimitiveType::Float32).is_numeric());
        assert!(TosType::from(PrimitiveType::Decimal {
            precision: 10,
            scale: 2
        })
        .is_numeric());
        assert!(!TosType::from(PrimitiveType::Text { max: None }).is_numeric());
    }

    #[test]
    fn optional_unwrap() {
        let inner = TosType::from(PrimitiveType::Int32);
        let opt = TosType::from(CompoundType::Optional(Box::new(inner.clone())));
        assert!(opt.is_optional());
        assert_eq!(opt.unwrap_optional(), &inner);
    }
}
