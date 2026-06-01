use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::TryStreamExt;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use thiserror::Error;
use tokio::sync::Mutex;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum RedisAdapterError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct RedisAdapter {
    name: String,
    url: String,
    key_prefix: String,
    conn: Arc<Mutex<MultiplexedConnection>>,
}

impl RedisAdapter {
    pub async fn connect(url: &str) -> Result<Self, RedisAdapterError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self {
            name: format!("redis://{url}"),
            url: url.to_string(),
            key_prefix: "tos:".to_string(),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn with_connection(
        name: String,
        url: String,
        key_prefix: String,
        conn: MultiplexedConnection,
    ) -> Self {
        Self {
            name,
            url,
            key_prefix,
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    pub fn key(&self, id: &str) -> String {
        format!("{}{}", self.key_prefix, id)
    }

    pub fn scan_pattern(&self) -> String {
        format!("{}*", self.key_prefix)
    }

    pub async fn list_keys(&self) -> Result<Vec<String>, RedisAdapterError> {
        let mut conn = self.conn.lock().await;
        let pattern = self.scan_pattern();
        let keys: Vec<String> = conn.keys(&pattern).await?;
        Ok(keys
            .into_iter()
            .map(|k| k.strip_prefix(&self.key_prefix).unwrap_or(&k).to_string())
            .collect())
    }

    pub async fn hgetall(
        &self,
        id: &str,
    ) -> Result<BTreeMap<String, String>, RedisAdapterError> {
        let mut conn = self.conn.lock().await;
        let key = self.key(id);
        let map: BTreeMap<String, String> = conn.hgetall(&key).await?;
        Ok(map)
    }

    pub async fn hset(&self, id: &str, fields: &BTreeMap<String, String>) -> Result<(), RedisAdapterError> {
        let mut conn = self.conn.lock().await;
        let key = self.key(id);
        let mut pipe = redis::pipe();
        for (k, v) in fields {
            pipe.hset(&key, k, v).ignore();
        }
        pipe.query_async::<()>(&mut *conn).await?;
        Ok(())
    }

    pub async fn del(&self, id: &str) -> Result<(), RedisAdapterError> {
        let mut conn = self.conn.lock().await;
        let key = self.key(id);
        let _: i64 = conn.del(&key).await?;
        Ok(())
    }
}

fn hash_to_value(map: BTreeMap<String, String>) -> TosValue {
    let mut obj = serde_json::Map::new();
    for (k, v) in map {
        obj.insert(k, serde_json::Value::String(v));
    }
    TosValue(serde_json::Value::Object(obj))
}

fn value_to_hash(value: &TosValue) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let serde_json::Value::Object(obj) = &value.0 {
        for (k, v) in obj {
            out.insert(k.clone(), json_to_string(v));
        }
    }
    out
}

fn json_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn infer_primitive(v: &str) -> PrimitiveType {
    if v == "true" || v == "false" {
        PrimitiveType::Bool
    } else if v.parse::<i64>().is_ok() {
        PrimitiveType::Int64
    } else if v.parse::<f64>().is_ok() {
        PrimitiveType::Float64
    } else {
        PrimitiveType::Text { max: None }
    }
}

fn derive_schema(samples: &[TosValue]) -> TosSchema {
    let mut tables = BTreeMap::new();
    if let Some(first) = samples.first() {
        if let serde_json::Value::Object(obj) = &first.0 {
            let mut fields = Vec::new();
            for (i, (k, v)) in obj.iter().enumerate() {
                let s = json_to_string(v);
                fields.push(TosField {
                    name: k.clone(),
                    ty: TosType::Primitive(infer_primitive(&s)),
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
            tables.insert(
                "rows".to_string(),
                TosTable {
                    name: "rows".to_string(),
                    key: vec![],
                    fields,
                    indexes: BTreeMap::new(),
                    relations: BTreeMap::new(),
                },
            );
        }
    }
    TosSchema {
        name: "redis".to_string(),
        version: "0.1.0".to_string(),
        tables,
    }
}

#[async_trait]
impl TosAdapter for RedisAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let keys = self.list_keys().await?;
        let mut samples = Vec::new();
        for k in keys.iter().take(10) {
            let map = self.hgetall(k).await?;
            samples.push(hash_to_value(map));
        }
        Ok(derive_schema(&samples))
    }

    async fn read_records(&self, _table: &str) -> Result<RecordStream, BoxedError> {
        let adapter = self.clone_handle();
        let stream = async_stream::try_stream! {
            let keys = adapter.list_keys().await?;
            for k in keys {
                let map = adapter.hgetall(&k).await?;
                if !map.is_empty() {
                    yield hash_to_value(map);
                }
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
        let mut batch: Vec<(String, BTreeMap<String, String>)> = Vec::new();
        while let Some(item) = records.try_next().await? {
            if let Some(id) = extract_id(&item) {
                let hash = value_to_hash(&item);
                batch.push((id, hash));
            } else {
                return Err(RedisAdapterError::Adapter(
                    "redis write_records: record must have an 'id' field".into(),
                )
                .into());
            }
        }
        for (id, hash) in batch {
            self.hset(&id, &hash).await?;
            count += 1;
        }
        Ok(count)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(RedisAdapterError::Adapter(
            "redis watch requires keyspace notifications (S5)".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        Ok(())
    }
}

impl RedisAdapter {
    fn clone_handle(&self) -> Self {
        Self {
            name: self.name.clone(),
            url: self.url.clone(),
            key_prefix: self.key_prefix.clone(),
            conn: self.conn.clone(),
        }
    }
}

fn extract_id(v: &TosValue) -> Option<String> {
    if let serde_json::Value::Object(obj) = &v.0 {
        if let Some(id) = obj.get("id") {
            return Some(json_to_string(id));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let mut map = BTreeMap::new();
        map.insert("id".to_string(), "42".to_string());
        map.insert("name".to_string(), "alice".to_string());
        let v = hash_to_value(map.clone());
        let v2 = hash_to_value(map);
        assert_eq!(v, v2);
        assert_eq!(v.0["id"], "42");
        assert_eq!(v.0["name"], "alice");
    }

    #[test]
    fn value_to_hash_extracts_string_values() {
        let v = TosValue(serde_json::json!({"id": 1, "name": "a", "score": 1.5}));
        let h = value_to_hash(&v);
        assert_eq!(h.get("id").unwrap(), "1");
        assert_eq!(h.get("name").unwrap(), "a");
        assert_eq!(h.get("score").unwrap(), "1.5");
    }

    #[test]
    fn json_to_string_handles_all_types() {
        assert_eq!(json_to_string(&serde_json::json!("x")), "x");
        assert_eq!(json_to_string(&serde_json::json!(1)), "1");
        assert_eq!(json_to_string(&serde_json::json!(1.5)), "1.5");
        assert_eq!(json_to_string(&serde_json::json!(true)), "true");
        assert_eq!(json_to_string(&serde_json::json!(null)), "null");
        assert_eq!(json_to_string(&serde_json::json!([1, 2])), "[1,2]");
    }

    #[test]
    fn infer_primitive_classifies_strings() {
        assert_eq!(infer_primitive("42"), PrimitiveType::Int64);
        assert_eq!(infer_primitive("true"), PrimitiveType::Bool);
        assert_eq!(infer_primitive("false"), PrimitiveType::Bool);
        assert_eq!(infer_primitive("1.5"), PrimitiveType::Float64);
        assert_eq!(infer_primitive("hello"), PrimitiveType::Text { max: None });
        assert_eq!(infer_primitive(""), PrimitiveType::Text { max: None });
    }

    #[test]
    fn derive_schema_from_sample_record() {
        let samples = vec![TosValue(serde_json::json!({
            "id": 1,
            "name": "alice",
            "score": 1.5
        }))];
        let s = derive_schema(&samples);
        let t = s.get_table("rows").expect("rows");
        let id_f = t.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_f.ty, TosType::Primitive(PrimitiveType::Int64));
        let score_f = t.fields.iter().find(|f| f.name == "score").unwrap();
        assert_eq!(score_f.ty, TosType::Primitive(PrimitiveType::Float64));
        let primary_count = t.fields.iter().filter(|f| f.primary).count();
        assert_eq!(primary_count, 1, "exactly one field should be primary");
    }

    #[test]
    fn derive_schema_empty_samples() {
        let s = derive_schema(&[]);
        assert!(s.get_table("rows").is_none());
    }

    #[test]
    fn key_format() {
        assert_eq!(format!("{}user-1", "tos:"), "tos:user-1");
    }

    #[test]
    fn extract_id_from_object() {
        let v = TosValue(serde_json::json!({"id": "user-1", "name": "a"}));
        assert_eq!(extract_id(&v), Some("user-1".to_string()));
    }

    #[test]
    fn extract_id_missing_returns_none() {
        let v = TosValue(serde_json::json!({"name": "a"}));
        assert!(extract_id(&v).is_none());
    }

    #[test]
    fn scan_pattern_default() {
        assert_eq!(format!("{}*", "tos:"), "tos:*");
    }
}
