use serde::{Deserialize, Serialize};
use std::fmt;

use super::TosType;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompoundType {
    Optional(Box<TosType>),
    Array(Box<TosType>),
    Map {
        key: Box<TosType>,
        value: Box<TosType>,
    },
    Enum(Vec<String>),
    Union(Vec<TosType>),
}

impl fmt::Display for CompoundType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompoundType::Optional(inner) => write!(f, "optional<{inner}>"),
            CompoundType::Array(inner) => write!(f, "array<{inner}>"),
            CompoundType::Map { key, value } => write!(f, "map<{key}, {value}>"),
            CompoundType::Enum(values) => {
                write!(f, "enum(")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            CompoundType::Union(variants) => {
                write!(f, "union<")?;
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ">")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PrimitiveType;

    #[test]
    fn display_optional() {
        let t = CompoundType::Optional(Box::new(TosType::Primitive(PrimitiveType::Int32)));
        assert_eq!(t.to_string(), "optional<int32>");
    }

    #[test]
    fn display_array() {
        let t = CompoundType::Array(Box::new(TosType::Primitive(PrimitiveType::Text { max: None })));
        assert_eq!(t.to_string(), "array<text>");
    }

    #[test]
    fn display_map() {
        let t = CompoundType::Map {
            key: Box::new(TosType::Primitive(PrimitiveType::Text { max: None })),
            value: Box::new(TosType::Primitive(PrimitiveType::Any)),
        };
        assert_eq!(t.to_string(), "map<text, any>");
    }

    #[test]
    fn display_enum() {
        let t = CompoundType::Enum(vec!["active".into(), "inactive".into(), "pending".into()]);
        assert_eq!(t.to_string(), "enum(active, inactive, pending)");
    }

    #[test]
    fn display_union() {
        let t = CompoundType::Union(vec![
            TosType::Primitive(PrimitiveType::Int32),
            TosType::Primitive(PrimitiveType::Text { max: None }),
        ]);
        assert_eq!(t.to_string(), "union<int32 | text>");
    }
}
