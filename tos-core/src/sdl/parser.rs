use std::collections::BTreeMap;

use toml::Value as TomlValue;

use crate::error::{CoreError, CoreResult};
use crate::sdl::schema::{
    DefaultValue, FieldIndex, RelationKind, TosField, TosIndex, TosRelation, TosSchema, TosTable,
};
use crate::types::{CompoundType, PrimitiveType, TosType};

struct RawField {
    ty: String,
    primary: bool,
    unique: bool,
    nullable: bool,
    max: Option<u32>,
    precision: Option<u8>,
    scale: Option<u8>,
    with_tz: Option<bool>,
    default: Option<RawDefault>,
    index: Option<i32>,
    comment: Option<String>,
}

enum RawDefault {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

pub fn parse_sdl(toml_str: &str) -> CoreResult<TosSchema> {
    let root: TomlValue = toml_str
        .parse::<TomlValue>()
        .map_err(|e: toml::de::Error| CoreError::TomlDe(e))?;

    let root_table = root.as_table().ok_or_else(|| CoreError::Parse {
        line: 0,
        col: 0,
        msg: "root must be a TOML table".into(),
    })?;

    let mut name = "untitled".to_string();
    let mut version = "0.1.0".to_string();
    let mut schema_section: Option<&toml::Table> = None;

    for (k, v) in root_table {
        match k.as_str() {
            "name" => {
                if let Some(s) = v.as_str() {
                    name = s.to_string();
                }
            }
            "version" => {
                if let Some(s) = v.as_str() {
                    version = s.to_string();
                }
            }
            "schema" => {
                schema_section = v.as_table();
            }
            _ => {}
        }
    }

    let schema_section = schema_section.ok_or_else(|| CoreError::Parse {
        line: 0,
        col: 0,
        msg: "missing [schema] section or nested schema tables".into(),
    })?;

    let mut schema = TosSchema { name, version, tables: BTreeMap::new() };

    for (table_name, table_value) in schema_section {
        let table_toml = table_value.as_table().ok_or_else(|| CoreError::Parse {
            line: 0,
            col: 0,
            msg: format!("schema.{table_name} must be a table"),
        })?;
        let (table, _) = parse_table(table_name.clone(), table_toml)?;
        schema.tables.insert(table_name.clone(), table);
    }

    Ok(schema)
}

fn parse_table(table_name: String, table: &toml::Table) -> CoreResult<(TosTable, ())> {
    let mut fields = Vec::new();
    let mut indexes = BTreeMap::new();
    let mut relations = BTreeMap::new();
    let mut key: Vec<String> = Vec::new();

    for (k, value) in table {
        match k.as_str() {
            "key" => {
                let arr = value.as_array().ok_or_else(|| CoreError::Parse {
                    line: 0,
                    col: 0,
                    msg: format!("{table_name}.key must be an array of field names"),
                })?;
                key = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
            "indexes" => {
                let idx_table = value.as_table().ok_or_else(|| CoreError::Parse {
                    line: 0,
                    col: 0,
                    msg: format!("{table_name}.indexes must be a table"),
                })?;
                for (idx_name, idx_value) in idx_table {
                    let idx_t = idx_value.as_table().ok_or_else(|| CoreError::Parse {
                        line: 0,
                        col: 0,
                        msg: format!("index '{idx_name}' must be a table"),
                    })?;
                    let fields_arr = idx_t
                        .get("fields")
                        .and_then(|v| v.as_array())
                        .ok_or_else(|| CoreError::Parse {
                            line: 0,
                            col: 0,
                            msg: format!("index '{idx_name}' missing 'fields' array"),
                        })?;
                    let fields_vec: Vec<String> = fields_arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    let unique = idx_t.get("unique").and_then(|v| v.as_bool()).unwrap_or(false);
                    indexes.insert(
                        idx_name.clone(),
                        TosIndex {
                            name: idx_name.clone(),
                            fields: fields_vec,
                            unique,
                        },
                    );
                }
            }
            "relations" => {
                let rel_table = value.as_table().ok_or_else(|| CoreError::Parse {
                    line: 0,
                    col: 0,
                    msg: format!("{table_name}.relations must be a table"),
                })?;
                for (rel_name, rel_value) in rel_table {
                    let rel_t = rel_value.as_table().ok_or_else(|| CoreError::Parse {
                        line: 0,
                        col: 0,
                        msg: format!("relation '{rel_name}' must be a table"),
                    })?;
                    let kind_str = rel_t.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
                        CoreError::Parse {
                            line: 0,
                            col: 0,
                            msg: format!("relation '{rel_name}' missing 'type'"),
                        }
                    })?;
                    let kind = match kind_str {
                        "has_many" => RelationKind::HasMany,
                        "has_one" => RelationKind::HasOne,
                        "belongs_to" => RelationKind::BelongsTo,
                        other => {
                            return Err(CoreError::Parse {
                                line: 0,
                                col: 0,
                                msg: format!("unknown relation type '{other}' for '{rel_name}'"),
                            });
                        }
                    };
                    let target_schema = rel_t
                        .get("schema")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| CoreError::Parse {
                            line: 0,
                            col: 0,
                            msg: format!("relation '{rel_name}' missing 'schema'"),
                        })?
                        .to_string();
                    let foreign_key = rel_t
                        .get("foreign_key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| CoreError::Parse {
                            line: 0,
                            col: 0,
                            msg: format!("relation '{rel_name}' missing 'foreign_key'"),
                        })?
                        .to_string();
                    relations.insert(
                        rel_name.clone(),
                        TosRelation {
                            name: rel_name.clone(),
                            kind,
                            schema: target_schema,
                            foreign_key,
                        },
                    );
                }
            }
            _ => {
                let field_t = value.as_table().ok_or_else(|| CoreError::Parse {
                    line: 0,
                    col: 0,
                    msg: format!("field '{k}' must be a table"),
                })?;
                let raw = parse_field_raw(field_t)?;
                let field = convert_field(k.clone(), raw)?;
                fields.push(field);
            }
        }
    }

    let keyset: std::collections::HashSet<&String> = key.iter().collect();
    for f in fields.iter_mut() {
        if keyset.contains(&f.name) {
            f.primary = true;
        }
    }

    Ok((
        TosTable {
            name: table_name,
            key,
            fields,
            indexes,
            relations,
        },
        (),
    ))
}

fn parse_field_raw(t: &toml::Table) -> CoreResult<RawField> {
    let ty = t
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::Parse {
            line: 0,
            col: 0,
            msg: "field missing 'type'".into(),
        })?
        .to_string();

    let primary = t.get("primary").and_then(|v| v.as_bool()).unwrap_or(false);
    let unique = t.get("unique").and_then(|v| v.as_bool()).unwrap_or(false);
    let nullable = t.get("nullable").and_then(|v| v.as_bool()).unwrap_or(false);
    let max = t.get("max").and_then(|v| v.as_integer()).map(|i| i as u32);
    let precision = t.get("precision").and_then(|v| v.as_integer()).map(|i| i as u8);
    let scale = t.get("scale").and_then(|v| v.as_integer()).map(|i| i as u8);
    let with_tz = t.get("with_tz").and_then(|v| v.as_bool());
    let default = if let Some(v) = t.get("default") {
        Some(parse_default(v)?)
    } else {
        None
    };
    let index = t.get("index").and_then(|v| v.as_integer()).map(|i| i as i32);
    let comment = t.get("comment").and_then(|v| v.as_str()).map(String::from);

    Ok(RawField {
        ty,
        primary,
        unique,
        nullable,
        max,
        precision,
        scale,
        with_tz,
        default,
        index,
        comment,
    })
}

fn parse_default(v: &TomlValue) -> CoreResult<RawDefault> {
    match v {
        TomlValue::Boolean(b) => Ok(RawDefault::Bool(*b)),
        TomlValue::Integer(i) => Ok(RawDefault::Int(*i)),
        TomlValue::Float(f) => Ok(RawDefault::Float(*f)),
        TomlValue::String(s) => Ok(RawDefault::Str(s.clone())),
        _ => Err(CoreError::Parse {
            line: 0,
            col: 0,
            msg: "unsupported default value type".into(),
        }),
    }
}

fn convert_field(name: String, raw: RawField) -> CoreResult<TosField> {
    let mut ty = parse_type_string(&raw.ty)?;
    apply_type_modifiers(&mut ty, &raw);

    let default = match raw.default {
        Some(RawDefault::Bool(b)) => Some(DefaultValue::Bool(b)),
        Some(RawDefault::Int(i)) => Some(DefaultValue::Int(i)),
        Some(RawDefault::Float(f)) => Some(DefaultValue::Float(f)),
        Some(RawDefault::Str(s)) => {
            if s == "now" {
                Some(DefaultValue::Now)
            } else {
                Some(DefaultValue::String(s))
            }
        }
        None => None,
    };

    let index = raw.index.map(|order| FieldIndex { order });

    Ok(TosField {
        name,
        ty,
        nullable: raw.nullable,
        primary: raw.primary,
        unique: raw.unique,
        default,
        index,
        comment: raw.comment,
    })
}

fn apply_type_modifiers(ty: &mut TosType, raw: &RawField) {
    if let TosType::Primitive(PrimitiveType::Text { ref mut max }) = ty {
        if max.is_none() {
            *max = raw.max;
        }
    } else if let TosType::Primitive(PrimitiveType::Bytes { ref mut max }) = ty {
        if max.is_none() {
            *max = raw.max;
        }
    } else if let TosType::Primitive(PrimitiveType::Decimal {
        ref mut precision,
        ref mut scale,
    }) = ty
    {
        if let Some(p) = raw.precision {
            *precision = p;
        }
        if let Some(s) = raw.scale {
            *scale = s;
        }
    } else if let TosType::Primitive(PrimitiveType::Timestamp { ref mut with_tz }) = ty {
        if let Some(tz) = raw.with_tz {
            *with_tz = tz;
        }
    }

    if raw.nullable && !matches!(ty, TosType::Compound(CompoundType::Optional(_))) {
        let inner = ty.clone();
        *ty = TosType::Compound(CompoundType::Optional(Box::new(inner)));
    }
}

fn parse_type_string(s: &str) -> CoreResult<TosType> {
    let s = s.trim();

    if let Some(prim) = PrimitiveType::parse(s) {
        return Ok(TosType::Primitive(prim));
    }

    if let Some(rest) = s.strip_prefix("optional<").and_then(|r| r.strip_suffix('>')) {
        let inner = parse_type_string(rest)?;
        return Ok(TosType::Compound(CompoundType::Optional(Box::new(inner))));
    }

    if let Some(rest) = s.strip_prefix("array<").and_then(|r| r.strip_suffix('>')) {
        let inner = parse_type_string(rest)?;
        return Ok(TosType::Compound(CompoundType::Array(Box::new(inner))));
    }

    if let Some(rest) = s.strip_prefix("map<").and_then(|r| r.strip_suffix('>')) {
        let (k, v) = split_two(rest, ',')?;
        let key = parse_type_string(k.trim())?;
        let value = parse_type_string(v.trim())?;
        return Ok(TosType::Compound(CompoundType::Map {
            key: Box::new(key),
            value: Box::new(value),
        }));
    }

    if let Some(rest) = s.strip_prefix("enum(").and_then(|r| r.strip_suffix(')')) {
        let values: Vec<String> = rest
            .split(',')
            .map(|v| v.trim().trim_matches('"').to_string())
            .collect();
        return Ok(TosType::Compound(CompoundType::Enum(values)));
    }

    if let Some(rest) = s.strip_prefix("union<").and_then(|r| r.strip_suffix('>')) {
        let parts: Result<Vec<TosType>, _> = rest.split('|').map(parse_type_string).collect();
        return Ok(TosType::Compound(CompoundType::Union(parts?)));
    }

    Err(CoreError::Parse {
        line: 0,
        col: 0,
        msg: format!("unknown type expression: '{s}'"),
    })
}

fn split_two(s: &str, sep: char) -> CoreResult<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            c if c == sep && depth == 0 => return Ok((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    Err(CoreError::Parse {
        line: 0,
        col: 0,
        msg: format!("expected '{sep}' separator in '{s}'"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const USERS_SDL: &str = r#"
[schema.users]
id         = { type = "uuid", primary = true }
name       = { type = "text", max = 255, nullable = false }
email      = { type = "text", unique = true }
age        = { type = "uint16" }
metadata   = { type = "map<text, any>" }
active     = { type = "bool", default = true }
created_at = { type = "timestamp(tz)", default = "now" }
deleted_at = { type = "timestamp(tz)", nullable = true }

[schema.users.indexes]
email_idx  = { fields = ["email"], unique = true }
active_idx = { fields = ["active", "created_at"] }

[schema.users.relations]
posts = { type = "has_many", schema = "posts", foreign_key = "user_id" }
"#;

    #[test]
    fn parse_users_example() {
        let schema = parse_sdl(USERS_SDL).expect("parse ok");
        let users = schema.get_table("users").expect("users table");
        assert_eq!(users.fields.len(), 8);

        let by_name: std::collections::HashMap<&str, &TosField> = users
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f))
            .collect();

        let id = by_name["id"];
        assert_eq!(id.ty, TosType::Primitive(PrimitiveType::Uuid));
        assert!(id.primary);

        let metadata = by_name["metadata"];
        assert_eq!(
            metadata.ty,
            TosType::Compound(CompoundType::Map {
                key: Box::new(TosType::Primitive(PrimitiveType::Text { max: None })),
                value: Box::new(TosType::Primitive(PrimitiveType::Any)),
            })
        );

        let active = by_name["active"];
        assert!(matches!(active.default, Some(DefaultValue::Bool(true))));

        let created_at = by_name["created_at"];
        assert!(matches!(created_at.default, Some(DefaultValue::Now)));
        assert!(matches!(
            created_at.ty,
            TosType::Primitive(PrimitiveType::Timestamp { with_tz: true })
        ));

        let deleted_at = by_name["deleted_at"];
        assert!(deleted_at.nullable);
        assert!(matches!(
            deleted_at.ty,
            TosType::Compound(CompoundType::Optional(_))
        ));

        assert_eq!(users.indexes.len(), 2);
        assert!(users.indexes["email_idx"].unique);

        let posts_rel = &users.relations["posts"];
        assert_eq!(posts_rel.kind, RelationKind::HasMany);
        assert_eq!(posts_rel.schema, "posts");
        assert_eq!(posts_rel.foreign_key, "user_id");
    }

    #[test]
    fn parse_array_type() {
        let sdl = r#"
[schema.tags]
id   = { type = "uuid", primary = true }
name = { type = "array<text>", nullable = false }
"#;
        let schema = parse_sdl(sdl).unwrap();
        let tags = schema.get_table("tags").unwrap();
        let name = tags.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(
            name.ty,
            TosType::Compound(CompoundType::Array(Box::new(TosType::Primitive(
                PrimitiveType::Text { max: None }
            ))))
        );
    }

    #[test]
    fn parse_optional_type() {
        let sdl = r#"
[schema.t]
id   = { type = "uuid", primary = true }
note = { type = "optional<text>" }
"#;
        let schema = parse_sdl(sdl).unwrap();
        let note = schema.get_table("t").unwrap().fields.iter().find(|f| f.name == "note").unwrap();
        assert!(matches!(
            note.ty,
            TosType::Compound(CompoundType::Optional(_))
        ));
    }

    #[test]
    fn parse_enum_type() {
        let sdl = r#"
[schema.s]
id     = { type = "uuid", primary = true }
status = { type = "enum(active, inactive, pending)" }
"#;
        let schema = parse_sdl(sdl).unwrap();
        let status = schema.get_table("s").unwrap().fields.iter().find(|f| f.name == "status").unwrap();
        assert_eq!(
            status.ty,
            TosType::Compound(CompoundType::Enum(vec![
                "active".into(),
                "inactive".into(),
                "pending".into()
            ]))
        );
    }

    #[test]
    fn parse_nested_map() {
        let sdl = r#"
[schema.m]
id   = { type = "uuid", primary = true }
data = { type = "map<text, array<int32>>" }
"#;
        let schema = parse_sdl(sdl).unwrap();
        let data = schema.get_table("m").unwrap().fields.iter().find(|f| f.name == "data").unwrap();
        if let TosType::Compound(CompoundType::Map { key, value }) = &data.ty {
            assert_eq!(**key, TosType::Primitive(PrimitiveType::Text { max: None }));
            assert_eq!(
                **value,
                TosType::Compound(CompoundType::Array(Box::new(TosType::Primitive(
                    PrimitiveType::Int32
                ))))
            );
        } else {
            panic!("expected map type, got {:?}", data.ty);
        }
    }

    #[test]
    fn invalid_type_errors() {
        let sdl = r#"
[schema.bad]
id = { type = "nonsense" }
"#;
        let err = parse_sdl(sdl).unwrap_err();
        assert!(matches!(err, CoreError::Parse { .. }));
    }

    #[test]
    fn roundtrip_idempotent() {
        let schema = parse_sdl(USERS_SDL).unwrap();
        let reparsed = parse_sdl(USERS_SDL).unwrap();
        assert_eq!(schema, reparsed);
    }
}
