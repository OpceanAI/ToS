use std::collections::BTreeMap;

use async_trait::async_trait;
use bson::{doc, Document};
use futures::TryStreamExt;
use mongodb::options::ClientOptions;
use mongodb::{Client, Collection, Database};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{FieldIndex, TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum MongoAdapterError {
    #[error("mongodb error: {0}")]
    Mongo(#[from] mongodb::error::Error),
    #[error("bson error: {0}")]
    Bson(String),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct MongodbAdapter {
    name: String,
    client: Client,
    db_name: String,
}

impl MongodbAdapter {
    pub async fn connect(url: &str) -> Result<Self, MongoAdapterError> {
        let opts = ClientOptions::parse(url).await?;
        let client = Client::with_options(opts)?;
        let db_name = url
            .split_once("://")
            .and_then(|(_, rest)| {
                let path_part = rest.split('?').next().unwrap_or(rest);
                let slash = path_part.find('/')?;
                let db = &path_part[slash + 1..];
                if db.is_empty() {
                    None
                } else {
                    Some(db.to_string())
                }
            })
            .unwrap_or_else(|| "tos".to_string());
        Ok(Self {
            name: format!("mongodb://{url}"),
            client,
            db_name,
        })
    }

    pub fn database(&self) -> Database {
        self.client.database(&self.db_name)
    }

    pub fn collection(&self, name: &str) -> Collection<Document> {
        self.database().collection::<Document>(name)
    }

    pub async fn list_collections(&self) -> Result<Vec<String>, MongoAdapterError> {
        let names = self
            .database()
            .list_collection_names()
            .await?;
        Ok(names)
    }

    pub async fn introspect(&self, name: &str) -> Result<TosTable, MongoAdapterError> {
        let coll = self.collection(name);
        let mut sample: Option<Document> = None;
        let mut cursor = coll.find(doc! {}).await?;
        while let Some(d) = cursor.try_next().await? {
            if sample.is_none() {
                sample = Some(d);
                break;
            }
        }
        let mut fields = Vec::new();
        if let Some(s) = sample {
            for (i, (k, v)) in s.iter().enumerate() {
                fields.push(TosField {
                    name: k.clone(),
                    ty: TosType::Primitive(bson_type_to_tos(v)),
                    nullable: true,
                    primary: i == 0 && k == "_id",
                    unique: k == "_id",
                    default: None,
                    index: if k == "_id" {
                        Some(FieldIndex { order: 0 })
                    } else {
                        None
                    },
                    comment: None,
                });
            }
        }
        Ok(TosTable {
            name: name.to_string(),
            key: vec![],
            fields,
            indexes: BTreeMap::new(),
            relations: BTreeMap::new(),
        })
    }

    pub async fn insert(
        &self,
        collection: &str,
        docs: Vec<Document>,
    ) -> Result<u64, MongoAdapterError> {
        if docs.is_empty() {
            return Ok(0);
        }
        let coll = self.collection(collection);
        let res = coll.insert_many(docs).await?;
        Ok(res.inserted_ids.len() as u64)
    }
}

pub fn bson_type_to_tos(v: &bson::Bson) -> PrimitiveType {
    match v {
        bson::Bson::Double(_) => PrimitiveType::Float64,
        bson::Bson::String(_) => PrimitiveType::Text { max: None },
        bson::Bson::Boolean(_) => PrimitiveType::Bool,
        bson::Bson::Null => PrimitiveType::Any,
        bson::Bson::Int32(_) => PrimitiveType::Int32,
        bson::Bson::Int64(_) => PrimitiveType::Int64,
        bson::Bson::ObjectId(_) => PrimitiveType::Uuid,
        bson::Bson::DateTime(_) => PrimitiveType::Timestamp { with_tz: true },
        bson::Bson::Binary(_) => PrimitiveType::Bytes { max: None },
        bson::Bson::Array(_) => PrimitiveType::Any,
        bson::Bson::Document(_) => PrimitiveType::Any,
        _ => PrimitiveType::Text { max: None },
    }
}

fn json_to_bson(v: serde_json::Value) -> Result<bson::Bson, MongoAdapterError> {
    bson::to_bson(&v).map_err(|e| MongoAdapterError::Bson(e.to_string()))
}

fn bson_to_json(b: bson::Bson) -> Result<serde_json::Value, MongoAdapterError> {
    let mut out = serde_json::Map::new();
    match b {
        bson::Bson::Document(doc) => {
            for (k, v) in doc {
                let json = bson_value_to_json(&v);
                out.insert(k, json);
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(bson_value_to_json(&other)),
    }
}

fn bson_value_to_json(b: &bson::Bson) -> serde_json::Value {
    match b {
        bson::Bson::Double(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        bson::Bson::String(s) => serde_json::Value::String(s.clone()),
        bson::Bson::Boolean(b) => serde_json::Value::Bool(*b),
        bson::Bson::Null => serde_json::Value::Null,
        bson::Bson::Int32(n) => serde_json::Value::Number((*n).into()),
        bson::Bson::Int64(n) => serde_json::Value::Number((*n).into()),
        bson::Bson::ObjectId(oid) => serde_json::Value::String(oid.to_hex()),
        bson::Bson::DateTime(dt) => serde_json::Value::String(dt.try_to_rfc3339_string().unwrap_or_default()),
        bson::Bson::Binary(b) => serde_json::Value::String(b.bytes.iter().map(|x| format!("{x:02x}")).collect()),
        bson::Bson::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(bson_value_to_json).collect())
        }
        bson::Bson::Document(doc) => {
            let mut m = serde_json::Map::new();
            for (k, v) in doc {
                m.insert(k.to_string(), bson_value_to_json(v));
            }
            serde_json::Value::Object(m)
        }
        _ => serde_json::Value::String(format!("{b:?}")),
    }
}

#[async_trait]
impl TosAdapter for MongodbAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let mut s = TosSchema::new(&self.db_name);
        let colls = self.list_collections().await?;
        for c in colls {
            let table = self.introspect(&c).await?;
            s.add_table(table);
        }
        Ok(s)
    }

    async fn read_records(&self, table: &str) -> Result<RecordStream, BoxedError> {
        let coll = self.collection(table);
        let stream = async_stream::try_stream! {
            let mut cursor = coll.find(doc! {}).await?;
            while let Some(d) = cursor.try_next().await? {
                yield TosValue(bson_to_json(bson::Bson::Document(d))?);
            }
        };
        Ok(Box::pin(stream))
    }

    async fn write_records(
        &self,
        table: &str,
        mut records: RecordStream,
    ) -> Result<u64, BoxedError> {
        let mut docs: Vec<Document> = Vec::new();
        while let Some(item) = records.try_next().await? {
            let bson = json_to_bson(item.0)?;
            let doc = match bson {
                bson::Bson::Document(d) => d,
                _ => {
                    return Err(MongoAdapterError::Adapter(
                        "mongodb write_records: expected JSON object".into(),
                    )
                    .into());
                }
            };
            docs.push(doc);
        }
        self.insert(table, docs).await.map_err(Into::into)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(MongoAdapterError::Adapter(
            "mongodb watch uses change streams (S6)".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        self.client.clone().shutdown().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::oid::ObjectId;

    #[test]
    fn bson_type_basic() {
        assert_eq!(bson_type_to_tos(&bson::Bson::Double(1.5)), PrimitiveType::Float64);
        assert_eq!(bson_type_to_tos(&bson::Bson::String("x".into())), PrimitiveType::Text { max: None });
        assert_eq!(bson_type_to_tos(&bson::Bson::Boolean(true)), PrimitiveType::Bool);
        assert_eq!(bson_type_to_tos(&bson::Bson::Int32(42)), PrimitiveType::Int32);
        assert_eq!(bson_type_to_tos(&bson::Bson::Int64(42)), PrimitiveType::Int64);
        assert_eq!(bson_type_to_tos(&bson::Bson::Null), PrimitiveType::Any);
        assert_eq!(bson_type_to_tos(&bson::Bson::ObjectId(ObjectId::new())), PrimitiveType::Uuid);
    }

    #[test]
    fn bson_value_to_json_string() {
        let j = bson_value_to_json(&bson::Bson::String("hi".into()));
        assert_eq!(j, serde_json::json!("hi"));
    }

    #[test]
    fn bson_value_to_json_int() {
        let j = bson_value_to_json(&bson::Bson::Int32(42));
        assert_eq!(j, serde_json::json!(42));
    }

    #[test]
    fn bson_value_to_json_double() {
        let j = bson_value_to_json(&bson::Bson::Double(1.5));
        assert_eq!(j, serde_json::json!(1.5));
    }

    #[test]
    fn bson_value_to_json_array() {
        let j = bson_value_to_json(&bson::Bson::Array(vec![
            bson::Bson::Int32(1),
            bson::Bson::Int32(2),
        ]));
        assert_eq!(j, serde_json::json!([1, 2]));
    }

    #[test]
    fn bson_value_to_json_objectid() {
        let oid = ObjectId::new();
        let j = bson_value_to_json(&bson::Bson::ObjectId(oid));
        assert_eq!(j, serde_json::Value::String(oid.to_hex()));
    }

    #[test]
    fn json_to_bson_roundtrip_object() {
        let v = serde_json::json!({"a": 1, "b": "x", "c": true});
        let b = json_to_bson(v.clone()).unwrap();
        let d = match b {
            bson::Bson::Document(d) => d,
            _ => panic!("expected document"),
        };
        match d.get("a") {
            Some(bson::Bson::Int64(n)) => assert_eq!(*n, 1),
            other => panic!("expected Int64(1) for 'a', got {other:?}"),
        }
        assert_eq!(d.get_str("b").unwrap(), "x");
        assert!(d.get_bool("c").unwrap());
    }

    #[test]
    fn bson_to_json_object() {
        let mut d = Document::new();
        d.insert("id", 1);
        d.insert("name", "alice");
        let j = bson_to_json(bson::Bson::Document(d)).unwrap();
        assert_eq!(j["id"], 1);
        assert_eq!(j["name"], "alice");
    }
}
