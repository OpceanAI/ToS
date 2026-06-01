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
    use std::pin::Pin;

    use crate::sdl::TosSchema;
    use crate::types::TosType;

    pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;
    pub type RecordStream = Pin<Box<dyn Stream<Item = Result<TosValue, BoxedError>> + Send>>;
    pub type ChangeStream = Pin<Box<dyn Stream<Item = Result<ChangeEvent, BoxedError>> + Send>>;

    #[derive(Debug, Clone, PartialEq)]
    pub struct TosValue(pub serde_json::Value);

    #[derive(Debug, Clone, PartialEq)]
    pub struct ChangeEvent {
        pub op: ChangeOp,
        pub table: String,
        pub before: Option<TosValue>,
        pub after: Option<TosValue>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
