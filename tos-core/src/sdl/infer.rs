use std::collections::BTreeMap;

use serde_json::Value as Json;

use crate::error::{CoreError, CoreResult};
use crate::sdl::schema::{TosField, TosTable};
use crate::types::{CompoundType, PrimitiveType, TosType};

pub struct JsonSample {
    pub table_name: String,
    pub records: Vec<Json>,
}

pub fn infer_schema_json(table_name: impl Into<String>, records: Vec<Json>) -> CoreResult<TosTable> {
    let table_name = table_name.into();
    if records.is_empty() {
        return Err(CoreError::Inference(
            "cannot infer schema from empty record set".into(),
        ));
    }

    let mut field_types: BTreeMap<String, Vec<TosType>> = BTreeMap::new();
    let mut field_nullable: BTreeMap<String, bool> = BTreeMap::new();

    for record in &records {
        let obj = record.as_object().ok_or_else(|| {
            CoreError::Inference("record is not a JSON object".into())
        })?;
        for (k, v) in obj {
            let entry = field_types.entry(k.clone()).or_default();
            let (ty, is_null) = infer_value_type(v);
            entry.push(ty);
            if is_null {
                *field_nullable.entry(k.clone()).or_insert(false) = true;
            }
        }
    }

    let mut fields = Vec::new();
    for (name, types) in &field_types {
        let merged = merge_types(types);
        let final_ty = if *field_nullable.get(name).unwrap_or(&false) {
            TosType::Compound(CompoundType::Optional(Box::new(merged)))
        } else {
            merged
        };
        fields.push(TosField {
            name: name.clone(),
            ty: final_ty,
            nullable: *field_nullable.get(name).unwrap_or(&false),
            primary: false,
            unique: false,
            default: None,
            index: None,
            comment: None,
        });
    }

    if let Some(first) = fields.iter_mut().find(|f| f.name == "id") {
        first.primary = true;
    }

    Ok(TosTable {
        name: table_name,
        fields,
        indexes: BTreeMap::new(),
        relations: BTreeMap::new(),
    })
}

pub fn infer_schema_csv(
    table_name: impl Into<String>,
    reader: &mut impl std::io::Read,
    has_header: bool,
    delimiter: u8,
) -> CoreResult<TosTable> {
    use std::io::BufRead;

    let table_name = table_name.into();
    let buf_reader = std::io::BufReader::new(reader);

    let mut headers: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();

    for (i, line) in buf_reader.lines().enumerate() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        let fields: Vec<String> = line
            .as_bytes()
            .split(|&b| b == delimiter)
            .map(|s| String::from_utf8_lossy(s).trim_matches('"').to_string())
            .collect();
        if i == 0 && has_header {
            headers = fields;
        } else {
            rows.push(fields);
        }
    }

    if rows.is_empty() {
        return Err(CoreError::Inference("csv has no data rows".into()));
    }
    if headers.is_empty() {
        headers = (0..rows[0].len()).map(|i| format!("col_{i}")).collect();
    }

    let mut fields = Vec::new();
    for (i, name) in headers.iter().enumerate() {
        let col: Vec<&str> = rows.iter().filter_map(|r| r.get(i).map(|s| s.as_str())).collect();
        let (ty, nullable) = infer_column(&col);
        let ty = if nullable {
            TosType::Compound(CompoundType::Optional(Box::new(ty)))
        } else {
            ty
        };
        fields.push(TosField {
            name: name.clone(),
            ty,
            nullable,
            primary: name == "id",
            unique: false,
            default: None,
            index: None,
            comment: None,
        });
    }

    Ok(TosTable {
        name: table_name,
        fields,
        indexes: BTreeMap::new(),
        relations: BTreeMap::new(),
    })
}

fn infer_value_type(v: &Json) -> (TosType, bool) {
    match v {
        Json::Null => (TosType::Primitive(PrimitiveType::Any), true),
        Json::Bool(_) => (TosType::Primitive(PrimitiveType::Bool), false),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                let ty = if i >= 0 && i <= u32::MAX as i64 {
                    PrimitiveType::Uint32
                } else {
                    PrimitiveType::Int64
                };
                (TosType::Primitive(ty), false)
            } else if let Some(_f) = n.as_f64() {
                (TosType::Primitive(PrimitiveType::Float64), false)
            } else {
                (TosType::Primitive(PrimitiveType::Any), false)
            }
        }
        Json::String(s) => (infer_string_type(s), false),
        Json::Array(arr) => {
            if arr.is_empty() {
                (TosType::Compound(CompoundType::Array(Box::new(TosType::Primitive(PrimitiveType::Any)))), false)
            } else {
                let inner_types: Vec<TosType> = arr.iter().map(|x| infer_value_type(x).0).collect();
                let merged = merge_types(&inner_types);
                (TosType::Compound(CompoundType::Array(Box::new(merged))), false)
            }
        }
        Json::Object(obj) => {
            let value_types: Vec<TosType> = obj.values().map(|v| infer_value_type(v).0).collect();
            let merged = merge_types(&value_types);
            (
                TosType::Compound(CompoundType::Map {
                    key: Box::new(TosType::Primitive(PrimitiveType::Text { max: None })),
                    value: Box::new(merged),
                }),
                false,
            )
        }
    }
}

fn infer_string_type(s: &str) -> TosType {
    if is_uuid(s) {
        return TosType::Primitive(PrimitiveType::Uuid);
    }
    if is_iso_date(s) {
        return TosType::Primitive(PrimitiveType::Timestamp { with_tz: true });
    }
    if s.parse::<i64>().is_ok() {
        return TosType::Primitive(PrimitiveType::Int64);
    }
    if s.parse::<f64>().is_ok() {
        return TosType::Primitive(PrimitiveType::Float64);
    }
    if s == "true" || s == "false" {
        return TosType::Primitive(PrimitiveType::Bool);
    }
    TosType::Primitive(PrimitiveType::Text { max: None })
}

fn is_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn is_iso_date(s: &str) -> bool {
    if s.len() < 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

fn infer_column(col: &[&str]) -> (TosType, bool) {
    let mut nullable = false;
    let mut types = Vec::new();
    for s in col {
        if s.is_empty() || s.eq_ignore_ascii_case("null") {
            nullable = true;
            continue;
        }
        types.push(infer_string_type(s));
    }
    if types.is_empty() {
        return (TosType::Primitive(PrimitiveType::Any), true);
    }
    (merge_types(&types), nullable)
}

fn merge_types(types: &[TosType]) -> TosType {
    if types.is_empty() {
        return TosType::Primitive(PrimitiveType::Any);
    }
    if types.iter().all(|t| t == &types[0]) {
        return types[0].clone();
    }
    let any = types.iter().any(|t| matches!(t, TosType::Primitive(PrimitiveType::Any)));
    if any {
        return TosType::Primitive(PrimitiveType::Any);
    }
    if types.iter().all(|t| t.is_integer()) {
        return TosType::Primitive(PrimitiveType::Int64);
    }
    if types.iter().all(|t| t.is_numeric()) {
        return TosType::Primitive(PrimitiveType::Float64);
    }
    if types.iter().all(|t| matches!(t, TosType::Primitive(PrimitiveType::Text { .. }))) {
        return TosType::Primitive(PrimitiveType::Text { max: None });
    }
    TosType::Primitive(PrimitiveType::Text { max: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infer_basic_types() {
        let records = vec![
            json!({
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "name": "Alice",
                "age": 30,
                "active": true,
                "score": 3.15,
                "joined": "2026-01-15T00:00:00Z"
            }),
            json!({
                "id": "550e8400-e29b-41d4-a716-446655440001",
                "name": "Bob",
                "age": 25,
                "active": false,
                "score": 2.71,
                "joined": "2026-02-20T00:00:00Z"
            }),
        ];
        let table = infer_schema_json("users", records).unwrap();
        assert_eq!(table.fields.len(), 6);

        let by_name: std::collections::HashMap<_, _> =
            table.fields.iter().map(|f| (f.name.as_str(), f)).collect();
        assert_eq!(by_name["id"].ty, TosType::Primitive(PrimitiveType::Uuid));
        assert!(by_name["id"].primary);
        assert_eq!(by_name["name"].ty, TosType::Primitive(PrimitiveType::Text { max: None }));
        assert_eq!(by_name["age"].ty, TosType::Primitive(PrimitiveType::Uint32));
        assert_eq!(by_name["active"].ty, TosType::Primitive(PrimitiveType::Bool));
        assert_eq!(by_name["score"].ty, TosType::Primitive(PrimitiveType::Float64));
        assert_eq!(
            by_name["joined"].ty,
            TosType::Primitive(PrimitiveType::Timestamp { with_tz: true })
        );
    }

    #[test]
    fn infer_nullable() {
        let records = vec![
            json!({ "id": 1, "note": "a" }),
            json!({ "id": 2, "note": null }),
        ];
        let table = infer_schema_json("t", records).unwrap();
        let note = table.fields.iter().find(|f| f.name == "note").unwrap();
        assert!(matches!(note.ty, TosType::Compound(CompoundType::Optional(_))));
        assert!(note.nullable);
    }

    #[test]
    fn infer_array_and_object() {
        let records = vec![json!({ "id": 1, "tags": ["a", "b"], "meta": { "k": 1 } })];
        let table = infer_schema_json("t", records).unwrap();
        let tags = table.fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(matches!(tags.ty, TosType::Compound(CompoundType::Array(_))));
        let meta = table.fields.iter().find(|f| f.name == "meta").unwrap();
        assert!(matches!(meta.ty, TosType::Compound(CompoundType::Map { .. })));
    }

    #[test]
    fn infer_csv() {
        let csv = b"id,name,age\n1,Alice,30\n2,Bob,25\n";
        let table = infer_schema_csv("users", &mut csv.as_ref(), true, b',').unwrap();
        assert_eq!(table.fields.len(), 3);
        let id = table.fields.iter().find(|f| f.name == "id").unwrap();
        assert!(id.primary);
        let age = table.fields.iter().find(|f| f.name == "age").unwrap();
        assert!(matches!(age.ty, TosType::Primitive(PrimitiveType::Int64)));
    }

    #[test]
    fn infer_empty_errors() {
        let records: Vec<Json> = vec![];
        let err = infer_schema_json("t", records).unwrap_err();
        assert!(matches!(err, CoreError::Inference(_)));
    }

    #[test]
    fn is_uuid_test() {
        assert!(is_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("550e8400-e29b-41d4-a716-44665544000"));
    }

    #[test]
    fn is_iso_date_test() {
        assert!(is_iso_date("2026-01-15T00:00:00Z"));
        assert!(is_iso_date("2026-01-15"));
        assert!(!is_iso_date("15-01-2026"));
        assert!(!is_iso_date("not-a-date"));
    }
}
