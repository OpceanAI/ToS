use std::collections::HashSet;

use crate::error::CoreResult;
use crate::sdl::schema::{TosSchema, TosTable};
use crate::types::{CompoundType, TosType};

pub fn validate(schema: &TosSchema) -> CoreResult<()> {
    let mut errors = Vec::new();

    if schema.name.is_empty() {
        errors.push("schema name is empty".into());
    }

    let table_names: Vec<&String> = schema.tables.keys().collect();
    let table_name_set: HashSet<&str> = table_names.iter().map(|s| s.as_str()).collect();

    for (table_name, table) in &schema.tables {
        validate_table(table, &table_name_set, &mut errors);
        if !table.name.is_empty() && table.name != *table_name {
            errors.push(format!(
                "table key '{table_name}' does not match inner name '{}'",
                table.name
            ));
        }
    }

    for rel in schema.tables.values().flat_map(|t| t.relations.values()) {
        if !table_name_set.contains(rel.schema.as_str()) {
            errors.push(format!(
                "relation '{}' references unknown schema '{}'",
                rel.name, rel.schema
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(crate::error::CoreError::Validation(errors))
    }
}

fn validate_table(
    table: &TosTable,
    known_tables: &HashSet<&str>,
    errors: &mut Vec<String>,
) {
    let mut seen_fields = HashSet::new();
    let mut has_primary = false;
    let field_names: HashSet<&str> = table.fields.iter().map(|f| f.name.as_str()).collect();

    for field in &table.fields {
        if !seen_fields.insert(field.name.as_str()) {
            errors.push(format!(
                "table '{}': duplicate field '{}'",
                table.name, field.name
            ));
        }
        if field.name.is_empty() {
            errors.push(format!("table '{}': field with empty name", table.name));
        }
        if field.primary {
            has_primary = true;
            if matches!(
                field.ty,
                TosType::Compound(CompoundType::Optional(_))
            ) {
                errors.push(format!(
                    "table '{}': primary field '{}' cannot be optional",
                    table.name, field.name
                ));
            }
        }
        if let TosType::Compound(CompoundType::Map { key, .. }) = &field.ty {
            if !is_valid_map_key(key) {
                errors.push(format!(
                    "table '{}': field '{}' has invalid map key type {}",
                    table.name,
                    field.name,
                    key
                ));
            }
        }
        if let TosType::Compound(CompoundType::Union(variants)) = &field.ty {
            if variants.is_empty() {
                errors.push(format!(
                    "table '{}': field '{}' has empty union",
                    table.name, field.name
                ));
            }
        }
    }

    if !has_primary {
        errors.push(format!("table '{}': no primary key defined", table.name));
    }

    for idx in table.indexes.values() {
        for f in &idx.fields {
            if !field_names.contains(f.as_str()) {
                errors.push(format!(
                    "table '{}': index '{}' references unknown field '{}'",
                    table.name, idx.name, f
                ));
            }
        }
    }

    for rel in table.relations.values() {
        if !field_names.contains(rel.foreign_key.as_str()) {
            errors.push(format!(
                "table '{}': relation '{}' foreign_key '{}' not found in fields",
                table.name, rel.name, rel.foreign_key
            ));
        }
        if !known_tables.contains(rel.schema.as_str()) {
            errors.push(format!(
                "table '{}': relation '{}' references unknown schema '{}'",
                table.name, rel.name, rel.schema
            ));
        }
    }
}

fn is_valid_map_key(ty: &TosType) -> bool {
    matches!(
        ty,
        TosType::Primitive(
            PrimitiveType::Text { .. }
                | PrimitiveType::Int8
                | PrimitiveType::Int16
                | PrimitiveType::Int32
                | PrimitiveType::Int64
                | PrimitiveType::Uint8
                | PrimitiveType::Uint16
                | PrimitiveType::Uint32
                | PrimitiveType::Uint64
                | PrimitiveType::Uuid
                | PrimitiveType::Bool
        )
    )
}

use crate::types::PrimitiveType;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl::parser::parse_sdl;

    #[test]
    fn valid_schema_passes() {
        let s = parse_sdl(
            r#"
[schema.users]
id = { type = "uuid", primary = true }
name = { type = "text" }
"#,
        )
        .unwrap();
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn missing_primary_key_fails() {
        let s = parse_sdl(
            r#"
[schema.t]
name = { type = "text" }
"#,
        )
        .unwrap();
        let err = validate(&s).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no primary key"));
    }

    #[test]
    fn unknown_relation_target_fails() {
        let s = parse_sdl(
            r#"
[schema.users]
id = { type = "uuid", primary = true }

[schema.users.relations]
posts = { type = "has_many", schema = "posts", foreign_key = "user_id" }
"#,
        )
        .unwrap();
        let err = validate(&s).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown schema 'posts'"));
    }

    #[test]
    fn index_referencing_unknown_field_fails() {
        let s = parse_sdl(
            r#"
[schema.t]
id = { type = "uuid", primary = true }

[schema.t.indexes]
foo = { fields = ["nope"] }
"#,
        )
        .unwrap();
        let err = validate(&s).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown field 'nope'"));
    }

    #[test]
    fn duplicate_field_fails() {
        let toml = r#"
[schema.t]
id = { type = "uuid", primary = true }
name = { type = "text" }
"#;
        let s = parse_sdl(toml).unwrap();
        let mut tables_iter = s.tables.iter();
        let (_, t) = tables_iter.next().unwrap();
        let dup_name = if t.fields[0].name == "id" { "id" } else { "id_dup" };
        let mut field = t.fields[0].clone();
        field.name = dup_name.to_string();
        let mut t2 = t.clone();
        t2.fields.push(field);
        let mut s2 = s.clone();
        s2.tables.insert(t2.name.clone(), t2);
        let err = validate(&s2).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate field") || msg.contains("primary"));
    }
}
