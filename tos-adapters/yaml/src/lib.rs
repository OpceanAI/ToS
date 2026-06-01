use std::path::{Path, PathBuf};
use std::sync::RwLock;

use async_trait::async_trait;
use futures::TryStreamExt;
use serde_json::Value;
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{FieldIndex, TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum YamlAdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct YamlAdapter {
    name: String,
    path: PathBuf,
    records: RwLock<Vec<TosValue>>,
    schema: RwLock<Option<TosSchema>>,
}

impl YamlAdapter {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            records: RwLock::new(Vec::new()),
            schema: RwLock::new(None),
        }
    }

    pub fn open(name: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, YamlAdapterError> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            if raw.trim().is_empty() {
                Vec::new()
            } else {
                let parsed: serde_yaml::Value = serde_yaml::from_str(&raw)?;
                yaml_array_to_records(parsed)?
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            name: name.into(),
            path,
            records: RwLock::new(records),
            schema: RwLock::new(None),
        })
    }

    pub fn with_records(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        records: Vec<TosValue>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            records: RwLock::new(records),
            schema: RwLock::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn records(&self) -> Vec<TosValue> {
        self.records.read().expect("yaml lock poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.records.read().expect("yaml lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save(&self) -> Result<(), YamlAdapterError> {
        let records = self.records.read().expect("yaml lock poisoned");
        let yaml_arr: Vec<serde_yaml::Value> = records
            .iter()
            .map(|t| serde_yaml::to_value(&t.0).unwrap_or(serde_yaml::Value::Null))
            .collect();
        let serialized = serde_yaml::to_string(&yaml_arr)?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, serialized)?;
        Ok(())
    }
}

fn yaml_array_to_records(v: serde_yaml::Value) -> Result<Vec<TosValue>, YamlAdapterError> {
    let json: Value = serde_yaml_to_json(v)?;
    match json {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(TosValue(item));
            }
            Ok(out)
        }
        Value::Object(_) => Ok(vec![TosValue(json)]),
        Value::Null => Ok(Vec::new()),
        _ => Err(YamlAdapterError::Adapter(
            "yaml root must be a sequence or mapping".into(),
        )),
    }
}

fn serde_yaml_to_json(v: serde_yaml::Value) -> Result<Value, serde_json::Error> {
    let s = serde_json::to_string(&v)?;
    serde_json::from_str(&s)
}

#[async_trait]
impl TosAdapter for YamlAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let guard = self.schema.read().expect("yaml lock poisoned");
        if let Some(s) = guard.as_ref() {
            return Ok(s.clone());
        }
        drop(guard);
        let s = derive_schema(&self.records.read().expect("yaml lock poisoned"));
        *self.schema.write().expect("yaml lock poisoned") = Some(s.clone());
        Ok(s)
    }

    async fn read_records(&self, _table: &str) -> Result<RecordStream, BoxedError> {
        let snapshot = self.records();
        let stream = async_stream::try_stream! {
            for v in snapshot {
                yield v;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn write_records(
        &self,
        _table: &str,
        mut records: RecordStream,
    ) -> Result<u64, BoxedError> {
        let mut count = 0u64;
        let mut new_records: Vec<TosValue> = Vec::new();
        while let Some(item) = records.try_next().await? {
            new_records.push(item);
            count += 1;
        }
        {
            let mut guard = self.records.write().expect("yaml lock poisoned");
            guard.extend(new_records);
        }
        self.save()?;
        Ok(count)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(YamlAdapterError::Adapter(
            "yaml watch is available via inotify in S6".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        self.save()?;
        Ok(())
    }
}

fn derive_schema(records: &[TosValue]) -> TosSchema {
    let mut tables = std::collections::BTreeMap::new();
    if let Some(first) = records.first() {
        if let Value::Object(obj) = &first.0 {
            let mut fields = Vec::new();
            for (i, (k, v)) in obj.iter().enumerate() {
                fields.push(TosField {
                    name: k.clone(),
                    ty: TosType::Primitive(infer_primitive(v)),
                    nullable: true,
                    primary: i == 0,
                    unique: false,
                    default: None,
                    index: if i == 0 {
                        Some(FieldIndex { order: 0 })
                    } else {
                        None
                    },
                    comment: None,
                });
            }
            tables.insert(
                "rows".to_string(),
                TosTable {
                    name: "rows".to_string(),
                    fields,
                    indexes: std::collections::BTreeMap::new(),
                    relations: std::collections::BTreeMap::new(),
                },
            );
        }
    }
    TosSchema {
        name: "yaml".to_string(),
        version: "0.1.0".to_string(),
        tables,
    }
}

fn infer_primitive(v: &Value) -> PrimitiveType {
    match v {
        Value::Null => PrimitiveType::Any,
        Value::Bool(_) => PrimitiveType::Bool,
        Value::Number(n) => {
            if n.is_i64() {
                PrimitiveType::Int64
            } else {
                PrimitiveType::Float64
            }
        }
        Value::String(_) => PrimitiveType::Text { max: None },
        Value::Array(_) | Value::Object(_) => PrimitiveType::Any,
    }
}

impl YamlAdapter {
    pub fn read_schema_sync(&self) -> TosSchema {
        self.schema
            .read()
            .expect("yaml lock poisoned")
            .clone()
            .unwrap_or_else(|| derive_schema(&self.records.read().expect("yaml lock poisoned")))
    }
}

impl Default for YamlAdapter {
    fn default() -> Self {
        Self::new("yaml", PathBuf::from("data.yaml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(s: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tos-yaml-{}-{}.yaml", std::process::id(), s));
        p
    }

    fn cleanup(p: &Path) {
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn open_missing_returns_empty() {
        let p = temp_path("missing");
        cleanup(&p);
        let a = YamlAdapter::open("a", &p).unwrap();
        assert!(a.is_empty());
        cleanup(&p);
    }

    #[test]
    fn open_existing_loads() {
        let p = temp_path("existing");
        std::fs::write(
            &p,
            "- id: 1\n  name: a\n- id: 2\n  name: b\n- id: 3\n  name: c\n",
        )
        .unwrap();
        let a = YamlAdapter::open("a", &p).unwrap();
        assert_eq!(a.len(), 3);
        cleanup(&p);
    }

    #[test]
    fn save_roundtrip() {
        let p = temp_path("roundtrip");
        cleanup(&p);
        let initial = vec![
            TosValue(serde_json::json!({"id": 1, "name": "a"})),
            TosValue(serde_json::json!({"id": 2, "name": "b"})),
            TosValue(serde_json::json!({"id": 3, "name": "c"})),
        ];
        let a = YamlAdapter::with_records("a", &p, initial.clone());
        a.save().unwrap();
        let b = YamlAdapter::open("b", &p).unwrap();
        assert_eq!(b.records(), initial);
        cleanup(&p);
    }

    #[test]
    fn save_writes_valid_yaml() {
        let p = temp_path("valid");
        cleanup(&p);
        let a = YamlAdapter::with_records(
            "a",
            &p,
            vec![TosValue(serde_json::json!({"id": 1, "name": "x"}))],
        );
        a.save().unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&raw).unwrap();
        let seq = parsed.as_sequence().expect("yaml root is a sequence");
        assert_eq!(seq.len(), 1);
        cleanup(&p);
    }

    #[test]
    fn read_schema_inferred() {
        let records = vec![TosValue(serde_json::json!({"id": 1, "name": "x", "score": 1.5}))];
        let a = YamlAdapter::with_records("a", temp_path("schema"), records);
        let s = a.read_schema_sync();
        let t = s.get_table("rows").unwrap();
        let id_f = t.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_f.ty, TosType::Primitive(PrimitiveType::Int64));
        let score_f = t.fields.iter().find(|f| f.name == "score").unwrap();
        assert_eq!(score_f.ty, TosType::Primitive(PrimitiveType::Float64));
    }

    #[test]
    fn empty_file_yields_empty() {
        let p = temp_path("empty");
        std::fs::write(&p, "").unwrap();
        let a = YamlAdapter::open("a", &p).unwrap();
        assert!(a.is_empty());
        cleanup(&p);
    }

    #[test]
    fn open_invalid_yaml_errors() {
        let p = temp_path("invalid");
        std::fs::write(&p, ": : invalid yaml :::\n  bad indent\n").unwrap();
        let res = YamlAdapter::open("a", &p);
        assert!(res.is_err());
        cleanup(&p);
    }

    #[test]
    fn open_root_object_wraps_in_single_record() {
        let p = temp_path("root-obj");
        std::fs::write(&p, "id: 1\nname: a\n").unwrap();
        let a = YamlAdapter::open("a", &p).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a.records()[0].0["name"], "a");
        cleanup(&p);
    }
}
