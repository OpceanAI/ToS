#![recursion_limit = "512"]

pub mod error;
pub mod resolve;
pub mod sdl;
pub mod types;

pub use error::{CoreError, CoreResult};
pub use resolve::{Resolution, ResolutionStatus, TypeResolver};
pub use sdl::{
    infer_schema_csv, infer_schema_json, parse_sdl, validate, DefaultValue, FieldIndex, JsonSample,
    RelationKind, TosField, TosIndex, TosRelation, TosSchema, TosTable,
};
pub use types::{CompoundType, PrimitiveType, TosType};

pub mod adapter {
    use async_trait::async_trait;
    use futures::Stream;
    use serde::{Deserialize, Serialize};
    use std::pin::Pin;

    use crate::sdl::TosSchema;
    use crate::types::TosType;

    pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;
    pub type RecordStream = Pin<Box<dyn Stream<Item = Result<TosValue, BoxedError>> + Send>>;
    pub type ChangeStream = Pin<Box<dyn Stream<Item = Result<ChangeEvent, BoxedError>> + Send>>;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct TosValue(pub serde_json::Value);

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct ChangeEvent {
        pub op: ChangeOp,
        pub table: String,
        pub before: Option<TosValue>,
        pub after: Option<TosValue>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ChangeOp {
        Insert,
        Update,
        Delete,
    }

    #[async_trait]
    pub trait TosAdapter: Send + Sync {
        fn name(&self) -> &str;

        async fn read_schema(&self) -> Result<TosSchema, BoxedError>;
        async fn write_schema(&self, _schema: &TosSchema) -> Result<(), BoxedError> {
            Ok(())
        }

        async fn read_records(
            &self,
            _table: &str,
        ) -> Result<RecordStream, BoxedError>;

        async fn write_records(
            &self,
            _table: &str,
            _records: RecordStream,
        ) -> Result<u64, BoxedError> {
            Ok(0)
        }

        async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn close(&self) -> Result<(), BoxedError> {
            Ok(())
        }
    }

    pub fn field_type(schema: &TosSchema, table: &str, field: &str) -> Option<TosType> {
        schema
            .get_table(table)
            .and_then(|t| t.fields.iter().find(|f| f.name == field))
            .map(|f| f.ty.clone())
    }
}

pub mod mock {
    use std::sync::RwLock;

    use async_trait::async_trait;
    use futures::stream::{self, StreamExt};

    use crate::sdl::TosSchema;

    use super::adapter::{
        BoxedError, RecordStream, TosAdapter, TosValue,
    };

    pub struct MockAdapter {
        name: String,
        schema: TosSchema,
        records: RwLock<Vec<TosValue>>,
    }

    impl MockAdapter {
        pub fn new(name: impl Into<String>, schema: TosSchema) -> Self {
            Self {
                name: name.into(),
                schema,
                records: RwLock::new(Vec::new()),
            }
        }

        pub fn with_records(
            name: impl Into<String>,
            schema: TosSchema,
            records: Vec<TosValue>,
        ) -> Self {
            Self {
                name: name.into(),
                schema,
                records: RwLock::new(records),
            }
        }

        pub fn records(&self) -> Vec<TosValue> {
            self.records.read().expect("mock lock poisoned").clone()
        }

        pub fn len(&self) -> usize {
            self.records.read().expect("mock lock poisoned").len()
        }

        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        pub fn push(&self, value: TosValue) {
            self.records
                .write()
                .expect("mock lock poisoned")
                .push(value);
        }
    }

    #[async_trait]
    impl TosAdapter for MockAdapter {
        fn name(&self) -> &str {
            &self.name
        }

        async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
            Ok(self.schema.clone())
        }

        async fn read_records(&self, _table: &str) -> Result<RecordStream, BoxedError> {
            let snapshot = self.records();
            Ok(Box::pin(stream::iter(snapshot.into_iter().map(Ok))))
        }

        async fn write_records(
            &self,
            _table: &str,
            mut records: RecordStream,
        ) -> Result<u64, BoxedError> {
            let mut count = 0u64;
            while let Some(item) = records.next().await {
                let v = item?;
                self.records
                    .write()
                    .expect("mock lock poisoned")
                    .push(v);
                count += 1;
            }
            Ok(count)
        }

        async fn close(&self) -> Result<(), BoxedError> {
            Ok(())
        }
    }
}

pub use mock::MockAdapter;

#[cfg(test)]
mod mock_tests {
    use std::collections::BTreeMap;

    use super::mock::MockAdapter;
    use super::adapter::{BoxedError, TosAdapter, TosValue};
    use crate::sdl::TosSchema;
    use futures::StreamExt;
    use serde_json::json;

    fn empty_schema() -> TosSchema {
        TosSchema {
            name: "test".into(),
            version: "1".into(),
            tables: BTreeMap::new(),
        }
    }

    fn sample(i: u64) -> TosValue {
        TosValue(json!({"id": i, "name": format!("r-{i}")}))
    }

    #[tokio::test]
    async fn mock_round_trip_preserves_order() {
        let initial: Vec<TosValue> = (0..5).map(sample).collect();
        let a = MockAdapter::with_records("a", empty_schema(), initial.clone());
        let mut stream = a.read_records("any").await.unwrap();
        let mut got = Vec::new();
        while let Some(v) = stream.next().await {
            got.push(v.unwrap());
        }
        assert_eq!(got, initial);
    }

    #[tokio::test]
    async fn mock_empty_read() {
        let a = MockAdapter::new("a", empty_schema());
        let mut stream = a.read_records("any").await.unwrap();
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn mock_schema_returns_configured() {
        let s = empty_schema();
        let a = MockAdapter::new("a", s.clone());
        let got = a.read_schema().await.unwrap();
        assert_eq!(got, s);
    }

    #[tokio::test]
    async fn mock_write_appends() {
        let a = MockAdapter::new("a", empty_schema());
        let to_write: Vec<TosValue> = (0..3).map(sample).collect();
        let input = {
            use futures::stream;
            Box::pin(stream::iter(to_write.clone().into_iter().map(Ok::<_, BoxedError>)))
        };
        let n = a.write_records("any", input).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(a.len(), 3);
        assert_eq!(a.records(), to_write);
    }

    #[tokio::test]
    async fn mock_concurrent_writes() {
        use std::sync::Arc;
        let a = Arc::new(MockAdapter::new("a", empty_schema()));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let a2 = a.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..100 {
                    a2.push(TosValue(json!({"i": i})));
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(a.len(), 1000);
    }

    #[tokio::test]
    async fn mock_name_accessor() {
        let a = MockAdapter::new("my-mock", empty_schema());
        assert_eq!(a.name(), "my-mock");
    }

    #[tokio::test]
    async fn mock_len_and_is_empty() {
        let a = MockAdapter::with_records("a", empty_schema(), vec![sample(1)]);
        assert_eq!(a.len(), 1);
        assert!(!a.is_empty());
        let b = MockAdapter::new("b", empty_schema());
        assert_eq!(b.len(), 0);
        assert!(b.is_empty());
    }

    #[tokio::test]
    async fn mock_with_real_schema() {
        use crate::sdl::parser::parse_sdl;
        let toml_src = r#"
[schema.users]
id   = { type = "uuid", primary = true }
name = { type = "text" }
"#;
        let schema = parse_sdl(toml_src).unwrap();
        let a = MockAdapter::with_records("a", schema.clone(), vec![]);
        let got = a.read_schema().await.unwrap();
        assert_eq!(got, schema);
    }
}
