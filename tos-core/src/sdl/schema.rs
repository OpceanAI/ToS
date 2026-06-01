use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::TosType;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TosField {
    pub name: String,
    pub ty: TosType,
    pub nullable: bool,
    pub primary: bool,
    pub unique: bool,
    pub default: Option<DefaultValue>,
    pub index: Option<FieldIndex>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Now,
}

impl DefaultValue {
    pub fn display(&self) -> String {
        match self {
            DefaultValue::Bool(b) => b.to_string(),
            DefaultValue::Int(i) => i.to_string(),
            DefaultValue::Float(f) => f.to_string(),
            DefaultValue::String(s) => s.clone(),
            DefaultValue::Now => "now".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldIndex {
    pub order: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TosIndex {
    pub name: String,
    pub fields: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    HasMany,
    HasOne,
    BelongsTo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TosRelation {
    pub name: String,
    pub kind: RelationKind,
    pub schema: String,
    pub foreign_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TosTable {
    pub name: String,
    pub key: Vec<String>,
    pub fields: Vec<TosField>,
    pub indexes: BTreeMap<String, TosIndex>,
    pub relations: BTreeMap<String, TosRelation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TosSchema {
    pub name: String,
    pub version: String,
    pub tables: BTreeMap<String, TosTable>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

impl TosSchema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: default_version(),
            tables: BTreeMap::new(),
        }
    }

    pub fn add_table(&mut self, table: TosTable) {
        self.tables.insert(table.name.clone(), table);
    }

    pub fn get_table(&self, name: &str) -> Option<&TosTable> {
        self.tables.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompoundType, PrimitiveType};

    #[test]
    fn schema_creation() {
        let mut schema = TosSchema::new("test");
        let table = TosTable {
            name: "users".into(),
            key: vec![],
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
        schema.add_table(table);
        assert!(schema.get_table("users").is_some());
        assert!(schema.get_table("posts").is_none());
    }

    #[test]
    fn default_value_display() {
        assert_eq!(DefaultValue::Now.display(), "now");
        assert_eq!(DefaultValue::Bool(true).display(), "true");
        assert_eq!(DefaultValue::Int(42).display(), "42");
    }

    #[test]
    fn field_with_optional_type() {
        let f = TosField {
            name: "deleted_at".into(),
            ty: TosType::Compound(CompoundType::Optional(Box::new(TosType::Primitive(
                PrimitiveType::Timestamp { with_tz: true },
            )))),
            nullable: true,
            primary: false,
            unique: false,
            default: None,
            index: None,
            comment: None,
        };
        assert_eq!(f.name, "deleted_at");
        assert!(f.nullable);
    }
}
