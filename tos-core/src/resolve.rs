use crate::types::{CompoundType, PrimitiveType, TosType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStatus {
    Lossless,
    Lossy,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub status: ResolutionStatus,
    pub warning: Option<String>,
}

impl Resolution {
    pub fn lossless() -> Self {
        Self {
            status: ResolutionStatus::Lossless,
            warning: None,
        }
    }

    pub fn lossy(warning: impl Into<String>) -> Self {
        Self {
            status: ResolutionStatus::Lossy,
            warning: Some(warning.into()),
        }
    }

    pub fn reject(reason: impl Into<String>) -> Self {
        Self {
            status: ResolutionStatus::Reject,
            warning: Some(reason.into()),
        }
    }

    pub fn is_ok(&self) -> bool {
        !matches!(self.status, ResolutionStatus::Reject)
    }

    pub fn reason(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

pub struct TypeResolver;

impl TypeResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, from: &TosType, to: &TosType) -> Resolution {
        if from == to {
            return Resolution::lossless();
        }
        if matches!(from, TosType::Primitive(PrimitiveType::Any))
            || matches!(to, TosType::Primitive(PrimitiveType::Any))
        {
            return Resolution::lossless();
        }
        match (from, to) {
            (TosType::Primitive(a), TosType::Primitive(b)) => resolve_primitive(a, b),
            (TosType::Compound(a), TosType::Compound(b)) => resolve_compound(a, b),
            _ => Resolution::reject(format!("cannot map {} to {}", from, to)),
        }
    }
}

impl Default for TypeResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_primitive(from: &PrimitiveType, to: &PrimitiveType) -> Resolution {
    use PrimitiveType::*;
    match (from, to) {
        (a, b) if a == b => Resolution::lossless(),
        (Int8 | Int16 | Int32, Int32 | Int64) => Resolution::lossless(),
        (Int8 | Int16 | Int32 | Int64, Int8 | Int16) => {
            Resolution::lossy("int64 source may overflow narrower int target")
        }
        (Int64, Int32) => Resolution::lossy("int64 may not fit in int32"),
        (Uint8 | Uint16 | Uint32, Uint32 | Uint64) => Resolution::lossless(),
        (Uint8 | Uint16 | Uint32 | Uint64, Uint8 | Uint16) => {
            Resolution::lossy("uint64 source may overflow narrower uint target")
        }
        (Uint64, Uint32) => Resolution::lossy("uint64 may not fit in uint32"),
        (Int64, Uint64) => Resolution::lossy("int64 to uint64 may reject negative values"),
        (Uint8 | Uint16 | Uint32 | Uint64, Int64) => Resolution::lossless(),
        (Int8 | Int16 | Int32 | Int64, Float64) => Resolution::lossless(),
        (Int8 | Int16 | Int32, Float32) => Resolution::lossless(),
        (Int64, Float32) => Resolution::lossy("int64 may lose precision in float32"),
        (Float32, Float64) => Resolution::lossless(),
        (Float64, Float32) => Resolution::lossy("float64 to float32 loses precision"),
        (Int32, Decimal { .. }) => Resolution::lossy("int32 may lose precision when stored as decimal"),
        (Float64, Decimal { .. }) => Resolution::lossy("float64 may round when stored as decimal"),
        (
            Timestamp { with_tz: true },
            Timestamp { with_tz: false },
        ) => Resolution::lossy("timezone information will be lost"),
        (
            Timestamp { with_tz: false },
            Timestamp { with_tz: true },
        ) => Resolution::lossless(),
        (Text { .. }, Text { .. }) => Resolution::lossless(),
        (Bytes { .. }, Bytes { .. }) => Resolution::lossless(),
        (Text { .. }, Bytes { .. }) | (Bytes { .. }, Text { .. }) => {
            Resolution::lossy("text <-> bytes conversion is lossy on serialization")
        }
        _ => Resolution::reject(format!("{from} -> {to}")),
    }
}

fn resolve_compound(from: &CompoundType, to: &CompoundType) -> Resolution {
    match (from, to) {
        (CompoundType::Optional(a), CompoundType::Optional(b)) => TypeResolver::new().resolve(a, b),
        (CompoundType::Optional(inner), other) => {
            let other_typed = TosType::Compound(other.clone());
            let inner_res = TypeResolver::new().resolve(inner.as_ref(), &other_typed);
            if matches!(inner_res.status, ResolutionStatus::Reject) {
                Resolution::reject(format!("optional<{inner}> -> {other}"))
            } else {
                inner_res
            }
        }
        (other, CompoundType::Optional(inner)) => {
            let from_typed = TosType::Compound(other.clone());
            let inner_res = TypeResolver::new().resolve(&from_typed, inner.as_ref());
            if matches!(inner_res.status, ResolutionStatus::Reject) {
                Resolution::reject(format!("{other} -> optional<{inner}>"))
            } else {
                Resolution::lossless()
            }
        }
        (CompoundType::Array(a), CompoundType::Array(b)) => TypeResolver::new().resolve(a, b),
        (
            CompoundType::Map { key: k1, value: v1 },
            CompoundType::Map { key: k2, value: v2 },
        ) => {
            let rk = TypeResolver::new().resolve(k1, k2);
            let rv = TypeResolver::new().resolve(v1, v2);
            combine(rk, rv, "map")
        }
        (CompoundType::Enum(a), CompoundType::Enum(b)) => {
            if a == b || a.iter().all(|v| b.contains(v)) {
                Resolution::lossless()
            } else {
                Resolution::reject("enum value not in target set")
            }
        }
        _ => Resolution::reject(format!("{from} -> {to}")),
    }
}

fn combine(a: Resolution, b: Resolution, ctx: &str) -> Resolution {
    if matches!(a.status, ResolutionStatus::Reject) {
        return a;
    }
    if matches!(b.status, ResolutionStatus::Reject) {
        return b;
    }
    match (&a.status, &b.status) {
        (ResolutionStatus::Lossy, _) | (_, ResolutionStatus::Lossy) => {
            Resolution::lossy(format!("{ctx} mapping has lossy components"))
        }
        _ => Resolution::lossless(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_lossless() {
        let t = TosType::Primitive(PrimitiveType::Int32);
        assert_eq!(
            TypeResolver::new().resolve(&t, &t),
            Resolution::lossless()
        );
    }

    #[test]
    fn int32_to_int64_lossless() {
        let from = TosType::Primitive(PrimitiveType::Int32);
        let to = TosType::Primitive(PrimitiveType::Int64);
        assert!(TypeResolver::new().resolve(&from, &to).is_ok());
    }

    #[test]
    fn int64_to_int32_lossy() {
        let from = TosType::Primitive(PrimitiveType::Int64);
        let to = TosType::Primitive(PrimitiveType::Int32);
        let r = TypeResolver::new().resolve(&from, &to);
        assert!(matches!(r.status, ResolutionStatus::Lossy));
    }

    #[test]
    fn float64_to_decimal_lossy() {
        let from = TosType::Primitive(PrimitiveType::Float64);
        let to = TosType::Primitive(PrimitiveType::Decimal {
            precision: 10,
            scale: 2,
        });
        let r = TypeResolver::new().resolve(&from, &to);
        assert!(matches!(r.status, ResolutionStatus::Lossy));
    }

    #[test]
    fn timestamp_tz_to_plain_lossy() {
        let from = TosType::Primitive(PrimitiveType::Timestamp { with_tz: true });
        let to = TosType::Primitive(PrimitiveType::Timestamp { with_tz: false });
        let r = TypeResolver::new().resolve(&from, &to);
        assert!(matches!(r.status, ResolutionStatus::Lossy));
    }

    #[test]
    fn array_int_to_array_long_lossless() {
        let from = TosType::Compound(CompoundType::Array(Box::new(TosType::Primitive(
            PrimitiveType::Int32,
        ))));
        let to = TosType::Compound(CompoundType::Array(Box::new(TosType::Primitive(
            PrimitiveType::Int64,
        ))));
        assert!(TypeResolver::new().resolve(&from, &to).is_ok());
    }

    #[test]
    fn any_always_ok() {
        let any = TosType::Primitive(PrimitiveType::Any);
        let text = TosType::Primitive(PrimitiveType::Text { max: None });
        assert!(TypeResolver::new().resolve(&any, &text).is_ok());
        assert!(TypeResolver::new().resolve(&text, &any).is_ok());
    }

    #[test]
    fn reject_incompatible() {
        let from = TosType::Primitive(PrimitiveType::Bool);
        let to = TosType::Primitive(PrimitiveType::Int32);
        let r = TypeResolver::new().resolve(&from, &to);
        assert!(!r.is_ok());
    }
}
