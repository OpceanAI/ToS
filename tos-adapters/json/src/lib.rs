use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use async_trait::async_trait;
use futures::TryStreamExt;
use serde_json::{Map, Value};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeEvent, ChangeOp, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum JsonAdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("adapter: {0}")]
    Adapter(String),
    #[error("parse: {0}")]
    Parse(String),
}

pub struct JsonAdapter {
    name: String,
    path: PathBuf,
    records: RwLock<Vec<TosValue>>,
    schema: RwLock<Option<TosSchema>>,
}

impl JsonAdapter {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            records: RwLock::new(Vec::new()),
            schema: RwLock::new(None),
        }
    }

    pub fn open(name: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, JsonAdapterError> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            if raw.trim().is_empty() {
                Vec::new()
            } else {
                let is_jsonl = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("jsonl") || e.eq_ignore_ascii_case("ndjson"))
                    .unwrap_or(false);
                if is_jsonl {
                    raw.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| {
                            let v: Value = serde_json::from_str(l)?;
                            match v {
                                Value::Object(_) => Ok(TosValue(v)),
                                other => Err(JsonAdapterError::Parse(format!(
                                    "jsonl line is not an object: {other}"
                                ))),
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    let parsed: Value = serde_json::from_str(&raw)?;
                    json_array_to_records(parsed)?
                }
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

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn records(&self) -> Vec<TosValue> {
        self.records.read().expect("json lock poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.records.read().expect("json lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save(&self) -> Result<(), JsonAdapterError> {
        let records = self.records.read().expect("json lock poisoned");
        let is_jsonl = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("jsonl") || e.eq_ignore_ascii_case("ndjson"))
            .unwrap_or(false);
        let serialized = if is_jsonl {
            let mut s = String::new();
            for r in records.iter() {
                s.push_str(&serde_json::to_string(&r.0)?);
                s.push('\n');
            }
            s
        } else {
            let arr: Vec<&Value> = records.iter().map(|t| &t.0).collect();
            serde_json::to_string_pretty(&arr)?
        };
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, serialized)?;
        Ok(())
    }

    fn derive_schema(&self) -> TosSchema {
        let records = self.records.read().expect("json lock poisoned");
        let mut tables: std::collections::BTreeMap<String, TosTable> =
            std::collections::BTreeMap::new();
        let table = records.first().and_then(|v| {
            if let Value::Object(obj) = &v.0 {
                Some(infer_table("rows", obj))
            } else {
                None
            }
        });
        if let Some(t) = table {
            tables.insert(t.name.clone(), t);
        }
        TosSchema {
            name: "json".to_string(),
            version: "0.1.0".to_string(),
            tables,
        }
    }
}

fn json_array_to_records(v: Value) -> Result<Vec<TosValue>, JsonAdapterError> {
    match v {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(TosValue(normalize(item)));
            }
            Ok(out)
        }
        Value::Object(_) => Ok(vec![TosValue(v)]),
        Value::Null => Ok(Vec::new()),
        _ => Err(JsonAdapterError::Adapter(
            "json root must be an array or object".into(),
        )),
    }
}

fn normalize(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut new_map = Map::new();
            for (k, v) in map {
                new_map.insert(k, normalize(v));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(normalize).collect()),
        other => other,
    }
}

fn infer_table(name: &str, obj: &Map<String, Value>) -> TosTable {
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
                Some(tos_core::sdl::FieldIndex { order: 0 })
            } else {
                None
            },
            comment: None,
        });
    }
    TosTable {
        name: name.to_string(),
        key: vec![],
        fields,
        indexes: std::collections::BTreeMap::new(),
        relations: std::collections::BTreeMap::new(),
    }
}

fn infer_primitive(v: &Value) -> PrimitiveType {
    match v {
        Value::Null => PrimitiveType::Any,
        Value::Bool(_) => PrimitiveType::Bool,
        Value::Number(n) => {
            if n.is_i64() {
                PrimitiveType::Int64
            } else if n.is_f64() {
                PrimitiveType::Float64
            } else {
                PrimitiveType::Any
            }
        }
        Value::String(_) => PrimitiveType::Text { max: None },
        Value::Array(_) => PrimitiveType::Any,
        Value::Object(_) => PrimitiveType::Any,
    }
}

#[async_trait]
impl TosAdapter for JsonAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let guard = self.schema.read().expect("json lock poisoned");
        if let Some(s) = guard.as_ref() {
            return Ok(s.clone());
        }
        drop(guard);
        let s = self.derive_schema();
        *self.schema.write().expect("json lock poisoned") = Some(s.clone());
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
            let mut guard = self.records.write().expect("json lock poisoned");
            guard.extend(new_records);
        }
        self.save()?;
        Ok(count)
    }

    async fn watch(&self, table: &str) -> Result<ChangeStream, BoxedError> {
        let path = self.path.clone();
        let initial_snapshot = self.records();
        let table_name = table.to_string();
        let stream = async_stream::try_stream! {
            let mut seen: std::collections::HashMap<String, TosValue> = std::collections::HashMap::new();
            for (i, v) in initial_snapshot.into_iter().enumerate() {
                seen.insert(format!("row-{i}"), v);
            }
            let mut last_mtime = mtime_of(&path);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let current_mtime = mtime_of(&path);
                if current_mtime == last_mtime {
                    continue;
                }
                last_mtime = current_mtime;
                let raw = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let parsed: Value = match serde_json::from_str(&raw) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let arr = match parsed {
                    Value::Array(a) => a,
                    _ => continue,
                };
                let mut current: std::collections::HashMap<String, TosValue> =
                    std::collections::HashMap::new();
                for (i, v) in arr.into_iter().enumerate() {
                    let key = format!("row-{i}");
                    let tv = TosValue(v);
                    if !seen.contains_key(&key) {
                        yield ChangeEvent {
                            op: ChangeOp::Insert,
                            table: table_name.clone(),
                            before: None,
                            after: Some(tv.clone()),
                        };
                    } else if seen.get(&key) != Some(&tv) {
                        let before = seen.get(&key).cloned();
                        yield ChangeEvent {
                            op: ChangeOp::Update,
                            table: table_name.clone(),
                            before,
                            after: Some(tv.clone()),
                        };
                    }
                    current.insert(key, tv);
                }
                for key in seen.keys() {
                    if !current.contains_key(key) {
                        let before = seen.get(key).cloned();
                        yield ChangeEvent {
                            op: ChangeOp::Delete,
                            table: table_name.clone(),
                            before,
                            after: None,
                        };
                    }
                }
                seen = current;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn close(&self) -> Result<(), BoxedError> {
        self.save()?;
        Ok(())
    }
}

impl JsonAdapter {
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

    pub fn push(&self, value: TosValue) {
        self.records
            .write()
            .expect("json lock poisoned")
            .push(value);
    }
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tos-json-test-{}-{}.json", std::process::id(), name));
        p
    }

    fn cleanup(p: &Path) {
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn infer_primitive_basic() {
        assert_eq!(infer_primitive(&json!(1)), PrimitiveType::Int64);
        assert_eq!(infer_primitive(&json!(true)), PrimitiveType::Bool);
        assert_eq!(infer_primitive(&json!("hi")), PrimitiveType::Text { max: None });
        assert_eq!(infer_primitive(&json!(1.5)), PrimitiveType::Float64);
        assert_eq!(infer_primitive(&json!(null)), PrimitiveType::Any);
        assert_eq!(infer_primitive(&json!([1, 2])), PrimitiveType::Any);
        assert_eq!(infer_primitive(&json!({"a": 1})), PrimitiveType::Any);
    }

    #[test]
    fn open_missing_file_returns_empty() {
        let p = temp_path("missing");
        cleanup(&p);
        let a = JsonAdapter::open("a", &p).unwrap();
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
        cleanup(&p);
    }

    #[test]
    fn open_existing_file_loads_records() {
        let p = temp_path("existing");
        std::fs::write(&p, r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#).unwrap();
        let a = JsonAdapter::open("a", &p).unwrap();
        assert_eq!(a.len(), 2);
        cleanup(&p);
    }

    #[test]
    fn open_empty_file_yields_zero() {
        let p = temp_path("empty");
        std::fs::write(&p, "").unwrap();
        let a = JsonAdapter::open("a", &p).unwrap();
        assert!(a.is_empty());
        cleanup(&p);
    }

    #[test]
    fn save_writes_valid_json() {
        let p = temp_path("save");
        cleanup(&p);
        let a = JsonAdapter::new("a", &p);
        a.push(TosValue(json!({"id": 1, "name": "first"})));
        a.push(TosValue(json!({"id": 2, "name": "second"})));
        a.save().unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "first");
        cleanup(&p);
    }

    #[tokio::test]
    async fn round_trip_read_save_load() {
        let p = temp_path("roundtrip");
        cleanup(&p);
        let initial: Vec<TosValue> = (0..5)
            .map(|i| TosValue(json!({"id": i, "v": format!("row-{i}")})))
            .collect();
        let a = JsonAdapter::with_records("a", &p, initial.clone());
        a.save().unwrap();
        let b = JsonAdapter::open("b", &p).unwrap();
        let loaded = b.records();
        assert_eq!(loaded, initial);
        cleanup(&p);
    }

    #[tokio::test]
    async fn read_records_stream() {
        let a = JsonAdapter::with_records(
            "a",
            temp_path("stream"),
            (0..3)
                .map(|i| TosValue(json!({"id": i})))
                .collect(),
        );
        let mut s = a.read_records("any").await.unwrap();
        let mut count = 0;
        while let Some(v) = s.try_next().await.unwrap() {
            assert_eq!(v.0["id"], json!(count));
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn write_records_appends_and_persists() {
        let p = temp_path("write");
        cleanup(&p);
        let a = JsonAdapter::new("a", &p);
        let to_write: Vec<TosValue> = (0..4)
            .map(|i| TosValue(json!({"i": i})))
            .collect();
        let stream = async_stream::try_stream! {
            for v in to_write {
                yield v;
            }
        };
        let n = a
            .write_records("any", Box::pin(stream))
            .await
            .unwrap();
        assert_eq!(n, 4);
        assert_eq!(a.len(), 4);
        let raw = std::fs::read_to_string(&p).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 4);
        cleanup(&p);
    }

    #[tokio::test]
    async fn read_schema_infers_from_records() {
        let records = vec![TosValue(json!({"id": 1, "name": "x", "score": 0.5}))];
        let a = JsonAdapter::with_records("a", temp_path("schema"), records);
        let s = a.read_schema().await.unwrap();
        let t = s.get_table("rows").expect("rows table");
        let id_field = t.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_field.ty, TosType::Primitive(PrimitiveType::Int64));
        let name_field = t.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(
            name_field.ty,
            TosType::Primitive(PrimitiveType::Text { max: None })
        );
        let score_field = t.fields.iter().find(|f| f.name == "score").unwrap();
        assert_eq!(score_field.ty, TosType::Primitive(PrimitiveType::Float64));
    }

    #[tokio::test]
    async fn read_schema_caches_result() {
        let a = JsonAdapter::with_records(
            "a",
            temp_path("cache"),
            vec![TosValue(json!({"id": 1}))],
        );
        let s1 = a.read_schema().await.unwrap();
        let s2 = a.read_schema().await.unwrap();
        assert_eq!(s1, s2);
    }

    #[tokio::test]
    async fn close_persists_records() {
        let p = temp_path("close");
        cleanup(&p);
        let a = JsonAdapter::new("a", &p);
        a.push(TosValue(json!({"id": 1})));
        a.close().await.unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        cleanup(&p);
    }

    #[test]
    fn open_invalid_json_returns_error() {
        let p = temp_path("invalid");
        std::fs::write(&p, "{not valid json").unwrap();
        let res = JsonAdapter::open("a", &p);
        assert!(res.is_err());
        cleanup(&p);
    }

    #[tokio::test]
    async fn watch_emits_insert_on_new_row() {
        use futures::StreamExt;
        let p = temp_path("watch-insert");
        cleanup(&p);
        std::fs::write(&p, "[]").unwrap();
        let a = JsonAdapter::new("a", &p);
        let mut watch = a.watch("users").await.unwrap();
        let write_task = tokio::spawn({
            let p = p.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                std::fs::write(&p, r#"[{"id":1,"name":"x"}]"#).unwrap();
            }
        });
        let evt = tokio::time::timeout(std::time::Duration::from_secs(5), watch.next())
            .await
            .expect("watch should emit within 5s")
            .expect("watch stream yielded an item")
            .expect("watch item is Ok");
        write_task.await.unwrap();
        assert_eq!(evt.op, ChangeOp::Insert);
        assert_eq!(evt.table, "users");
        assert!(evt.after.is_some());
        assert_eq!(evt.after.unwrap().0["id"], 1);
        cleanup(&p);
    }

    #[tokio::test]
    async fn watch_emits_update_on_modified_row() {
        use futures::StreamExt;
        let p = temp_path("watch-update");
        cleanup(&p);
        std::fs::write(&p, r#"[{"id":1,"v":"old"}]"#).unwrap();
        let a = JsonAdapter::open("a", &p).unwrap();
        let mut watch = a.watch("users").await.unwrap();
        let write_task = tokio::spawn({
            let p = p.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                std::fs::write(&p, r#"[{"id":1,"v":"new"}]"#).unwrap();
            }
        });
        let evt = tokio::time::timeout(std::time::Duration::from_secs(5), watch.next())
            .await
            .expect("watch should emit within 5s")
            .expect("watch stream yielded an item")
            .expect("watch item is Ok");
        write_task.await.unwrap();
        assert_eq!(evt.op, ChangeOp::Update);
        assert!(evt.before.is_some());
        assert!(evt.after.is_some());
        assert_eq!(evt.before.unwrap().0["v"], "old");
        assert_eq!(evt.after.unwrap().0["v"], "new");
        cleanup(&p);
    }

    #[tokio::test]
    async fn watch_emits_delete_on_removed_row() {
        use futures::StreamExt;
        let p = temp_path("watch-delete");
        cleanup(&p);
        std::fs::write(&p, r#"[{"id":1,"v":"a"}]"#).unwrap();
        let a = JsonAdapter::open("a", &p).unwrap();
        let mut watch = a.watch("users").await.unwrap();
        let write_task = tokio::spawn({
            let p = p.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                std::fs::write(&p, "[]").unwrap();
            }
        });
        let evt = tokio::time::timeout(std::time::Duration::from_secs(5), watch.next())
            .await
            .expect("watch should emit within 5s")
            .expect("watch stream yielded an item")
            .expect("watch item is Ok");
        write_task.await.unwrap();
        assert_eq!(evt.op, ChangeOp::Delete);
        assert!(evt.before.is_some());
        assert!(evt.after.is_none());
        cleanup(&p);
    }
}
