use std::fmt::Write;

use crate::types::{CompoundType, PrimitiveType, TosType};

use super::schema::{DefaultValue, TosField, TosSchema, TosTable};

pub fn to_sdl(schema: &TosSchema) -> String {
    let mut out = String::new();
    out.push_str(&format!("# ToS schema: {}\n", schema.name));
    out.push_str(&format!("# version: {}\n", schema.version));
    for table in schema.tables.values() {
        out.push('\n');
        out.push_str(&format!("[schema.{}]\n", table.name));
        for field in &table.fields {
            out.push_str(&serialize_field(field));
            out.push('\n');
        }
    }
    out
}

fn serialize_field(field: &TosField) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("type = \"{}\"", tos_type_name(&field.ty)));
    if field.nullable {
        parts.push("nullable = true".to_string());
    }
    if field.primary {
        parts.push("primary = true".to_string());
    }
    if field.unique {
        parts.push("unique = true".to_string());
    }
    if let Some(d) = &field.default {
        parts.push(format!("default = \"{}\"", d.display()));
    }
    if let Some(idx) = &field.index {
        parts.push(format!("index = {}", idx.order));
    }
    if let Some(c) = &field.comment {
        parts.push(format!("comment = \"{}\"", c.replace('"', "\\\"")));
    }
    format!("{} = {{ {} }}", field.name, parts.join(", "))
}

pub fn tos_type_name(ty: &TosType) -> String {
    match ty {
        TosType::Primitive(p) => primitive_name(p),
        TosType::Compound(c) => compound_name(c),
    }
}

fn primitive_name(p: &PrimitiveType) -> String {
    match p {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::Int8 => "int8".to_string(),
        PrimitiveType::Int16 => "int16".to_string(),
        PrimitiveType::Int32 => "int32".to_string(),
        PrimitiveType::Int64 => "int64".to_string(),
        PrimitiveType::Uint8 => "uint8".to_string(),
        PrimitiveType::Uint16 => "uint16".to_string(),
        PrimitiveType::Uint32 => "uint32".to_string(),
        PrimitiveType::Uint64 => "uint64".to_string(),
        PrimitiveType::Float32 => "float32".to_string(),
        PrimitiveType::Float64 => "float64".to_string(),
        PrimitiveType::Decimal { precision, scale } => {
            format!("decimal({precision},{scale})")
        }
        PrimitiveType::Text { max: Some(m) } => format!("text({m})"),
        PrimitiveType::Text { max: None } => "text".to_string(),
        PrimitiveType::Bytes { max: Some(m) } => format!("bytes({m})"),
        PrimitiveType::Bytes { max: None } => "bytes".to_string(),
        PrimitiveType::Uuid => "uuid".to_string(),
        PrimitiveType::Date => "date".to_string(),
        PrimitiveType::Time => "time".to_string(),
        PrimitiveType::Timestamp { with_tz: true } => "timestamptz".to_string(),
        PrimitiveType::Timestamp { with_tz: false } => "timestamp".to_string(),
        PrimitiveType::Duration => "duration".to_string(),
        PrimitiveType::Any => "any".to_string(),
    }
}

fn compound_name(c: &CompoundType) -> String {
    match c {
        CompoundType::Optional(inner) => format!("{}?", tos_type_name(inner)),
        CompoundType::Array(inner) => format!("list<{}>", tos_type_name(inner)),
        CompoundType::Map { key, value } => {
            format!("map<{}, {}>", tos_type_name(key), tos_type_name(value))
        }
        CompoundType::Enum(variants) => {
            let joined: Vec<String> = variants.iter().map(|v| format!("\"{v}\"")).collect();
            format!("enum({})", joined.join(" | "))
        }
        CompoundType::Union(items) => {
            let joined: Vec<String> = items.iter().map(tos_type_name).collect();
            format!("union<{}>", joined.join(" | "))
        }
    }
}

pub fn default_to_string(d: &DefaultValue) -> String {
    d.display()
}

pub fn tables_differ(a: &TosTable, b: &TosTable) -> Vec<String> {
    let mut diffs = Vec::new();
    if a.name != b.name {
        diffs.push(format!("name: `{}` vs `{}`", a.name, b.name));
    }
    if a.fields.len() != b.fields.len() {
        diffs.push(format!(
            "field count: {} vs {}",
            a.fields.len(),
            b.fields.len()
        ));
    }
    for (i, (fa, fb)) in a.fields.iter().zip(b.fields.iter()).enumerate() {
        if fa.name != fb.name {
            diffs.push(format!("field[{}] name: `{}` vs `{}`", i, fa.name, fb.name));
        }
        if fa.ty != fb.ty {
            diffs.push(format!(
                "field[{}] `{}` type: `{}` vs `{}`",
                i,
                fa.name,
                tos_type_name(&fa.ty),
                tos_type_name(&fb.ty)
            ));
        }
        if fa.nullable != fb.nullable {
            diffs.push(format!(
                "field[{}] `{}` nullable: {} vs {}",
                i, fa.name, fa.nullable, fb.nullable
            ));
        }
        if fa.primary != fb.primary {
            diffs.push(format!(
                "field[{}] `{}` primary: {} vs {}",
                i, fa.name, fa.primary, fb.primary
            ));
        }
    }
    diffs
}

pub fn write_diff_table(
    out: &mut String,
    label: &str,
    a: &TosTable,
    b: &TosTable,
) {
    let diffs = tables_differ(a, b);
    writeln!(out, "--- {label}").unwrap();
    if diffs.is_empty() {
        writeln!(out, "  (no differences)").unwrap();
    } else {
        for d in diffs {
            writeln!(out, "  {d}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl::TosSchema;
    use std::collections::BTreeMap;

    fn empty_schema() -> TosSchema {
        TosSchema {
            name: "test".into(),
            version: "0.1.0".into(),
            tables: BTreeMap::new(),
        }
    }

    #[test]
    fn tos_type_name_primitives() {
        assert_eq!(tos_type_name(&TosType::Primitive(PrimitiveType::Bool)), "bool");
        assert_eq!(tos_type_name(&TosType::Primitive(PrimitiveType::Int64)), "int64");
        assert_eq!(
            tos_type_name(&TosType::Primitive(PrimitiveType::Text { max: None })),
            "text"
        );
        assert_eq!(
            tos_type_name(&TosType::Primitive(PrimitiveType::Text { max: Some(255) })),
            "text(255)"
        );
        assert_eq!(
            tos_type_name(&TosType::Primitive(PrimitiveType::Decimal {
                precision: 10,
                scale: 2
            })),
            "decimal(10,2)"
        );
        assert_eq!(
            tos_type_name(&TosType::Primitive(PrimitiveType::Timestamp { with_tz: true })),
            "timestamptz"
        );
        assert_eq!(tos_type_name(&TosType::Primitive(PrimitiveType::Any)), "any");
    }

    #[test]
    fn tos_type_name_compound() {
        assert_eq!(
            tos_type_name(&TosType::Compound(CompoundType::Optional(Box::new(
                TosType::Primitive(PrimitiveType::Int64)
            )))),
            "int64?"
        );
        assert_eq!(
            tos_type_name(&TosType::Compound(CompoundType::Array(Box::new(
                TosType::Primitive(PrimitiveType::Text { max: None })
            )))),
            "list<text>"
        );
    }

    #[test]
    fn to_sdl_minimal() {
        let mut s = empty_schema();
        let t = TosTable {
            name: "users".into(),
            fields: vec![],
            indexes: std::collections::BTreeMap::new(),
            relations: std::collections::BTreeMap::new(),
        };
        s.tables.insert("users".into(), t);
        let out = to_sdl(&s);
        assert!(out.contains("[schema.users]"));
    }

    #[test]
    fn serialize_field_simple() {
        let f = TosField {
            name: "id".into(),
            ty: TosType::Primitive(PrimitiveType::Int64),
            nullable: false,
            primary: true,
            unique: false,
            default: None,
            index: None,
            comment: None,
        };
        let s = serialize_field(&f);
        assert!(s.contains("id = {"));
        assert!(s.contains("type = \"int64\""));
        assert!(s.contains("primary = true"));
    }

    #[test]
    fn serialize_field_with_nullable_and_default() {
        let f = TosField {
            name: "score".into(),
            ty: TosType::Primitive(PrimitiveType::Float64),
            nullable: true,
            primary: false,
            unique: false,
            default: Some(DefaultValue::String("0".into())),
            index: None,
            comment: None,
        };
        let s = serialize_field(&f);
        assert!(s.contains("nullable = true"));
        assert!(s.contains("default = \"0\""));
    }

    #[test]
    fn tables_differ_empty() {
        let a = TosTable {
            name: "x".into(),
            fields: vec![],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let b = TosTable {
            name: "x".into(),
            fields: vec![],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        assert!(tables_differ(&a, &b).is_empty());
    }

    #[test]
    fn tables_differ_field_count() {
        let a = TosTable {
            name: "x".into(),
            fields: vec![TosField {
                name: "id".into(),
                ty: TosType::Primitive(PrimitiveType::Int64),
                nullable: false,
                primary: true,
                unique: false,
                default: None,
                index: None,
                comment: None,
            }],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let b = TosTable {
            name: "x".into(),
            fields: vec![],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let diffs = tables_differ(&a, &b);
        assert!(diffs.iter().any(|d| d.contains("field count")));
    }

    #[test]
    fn tables_differ_field_type() {
        let mk = |ty: PrimitiveType| TosField {
            name: "v".into(),
            ty: TosType::Primitive(ty),
            nullable: false,
            primary: false,
            unique: false,
            default: None,
            index: None,
            comment: None,
        };
        let a = TosTable {
            name: "x".into(),
            fields: vec![mk(PrimitiveType::Int64)],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let b = TosTable {
            name: "x".into(),
            fields: vec![mk(PrimitiveType::Float64)],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let diffs = tables_differ(&a, &b);
        assert!(diffs.iter().any(|d| d.contains("type")));
    }

    #[test]
    fn write_diff_table_no_diffs() {
        let t = TosTable {
            name: "x".into(),
            fields: vec![],
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        };
        let mut s = String::new();
        write_diff_table(&mut s, "test", &t, &t);
        assert!(s.contains("no differences"));
    }
}
