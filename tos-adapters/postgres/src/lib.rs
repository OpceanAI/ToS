use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::TryStreamExt;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Row};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{
    FieldIndex, TosField, TosIndex, TosRelation, TosSchema, TosTable,
};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum PgAdapterError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uri: {0}")]
    InvalidUri(String),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct PostgresAdapter {
    name: String,
    pool: PgPool,
    schema_name: String,
}

impl PostgresAdapter {
    pub async fn connect(url: &str) -> Result<Self, PgAdapterError> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await?;
        let schema_name = schema_from_url(url).unwrap_or_else(|| "public".to_string());
        Ok(Self {
            name: format!("postgres://{url}"),
            pool,
            schema_name,
        })
    }

    pub fn from_pool(pool: PgPool, name: String, schema_name: String) -> Self {
        Self { name, pool, schema_name }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list_tables(&self) -> Result<Vec<String>, PgAdapterError> {
        let rows = sqlx::query(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| r.get::<String, _>("table_name"))
            .collect())
    }

    pub async fn introspect_table(&self, table: &str) -> Result<TosTable, PgAdapterError> {
        let rows = sqlx::query(
            "SELECT column_name, data_type, is_nullable, column_default, \
                    character_maximum_length, numeric_precision, numeric_scale \
             FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 \
             ORDER BY ordinal_position",
        )
        .bind(&self.schema_name)
        .bind(table)
        .fetch_all(&self.pool)
        .await?;

        let mut primary_keys: Vec<String> = Vec::new();
        if let Ok(pk_rows) = sqlx::query(
            "SELECT a.attname AS col \
             FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid \
                 AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = ($1 || '.' || $2)::regclass AND i.indisprimary",
        )
        .bind(&self.schema_name)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        {
            for r in pk_rows {
                primary_keys.push(r.get::<String, _>("col"));
            }
        }

        let mut fields = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let col_name: String = row.get("column_name");
            let data_type: String = row.get("data_type");
            let is_nullable: String = row.get("is_nullable");
            let char_max_len: Option<i32> = row.get("character_maximum_length");
            let num_precision: Option<i32> = row.get("numeric_precision");
            let num_scale: Option<i32> = row.get("numeric_scale");
            let is_pk = primary_keys.contains(&col_name);
            let ty = pg_type_to_tos(&data_type, char_max_len, num_precision, num_scale);
            let nullable = is_nullable.eq_ignore_ascii_case("YES") || is_pk;
            fields.push(TosField {
                name: col_name,
                ty,
                nullable,
                primary: is_pk,
                unique: false,
                default: None,
                index: if is_pk { Some(FieldIndex { order: i as i32 }) } else { None },
                comment: None,
            });
        }

        let mut indexes = BTreeMap::new();
        if !primary_keys.is_empty() {
            indexes.insert(
                "pk".to_string(),
                TosIndex {
                    name: "pk".to_string(),
                    fields: primary_keys.clone(),
                    unique: true,
                },
            );
        }

        Ok(TosTable {
            name: table.to_string(),
            fields,
            indexes,
            relations: BTreeMap::<String, TosRelation>::new(),
        })
    }
}

pub fn schema_from_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path_part = rest.split('?').next().unwrap_or(rest);
    let slash = path_part.find('/')?;
    let db_part = &path_part[slash + 1..];
    if db_part.is_empty() {
        return None;
    }
    Some(db_part.to_string())
}

pub fn pg_type_to_tos(
    pg_type: &str,
    char_max_len: Option<i32>,
    num_precision: Option<i32>,
    num_scale: Option<i32>,
) -> TosType {
    let base = match pg_type.to_lowercase().as_str() {
        "boolean" => PrimitiveType::Bool,
        "smallint" | "int2" => PrimitiveType::Int16,
        "integer" | "int4" => PrimitiveType::Int32,
        "bigint" | "int8" => PrimitiveType::Int64,
        "real" | "float4" => PrimitiveType::Float32,
        "double precision" | "float8" => PrimitiveType::Float64,
        "numeric" | "decimal" => PrimitiveType::Decimal {
            precision: num_precision.unwrap_or(38) as u8,
            scale: num_scale.unwrap_or(0) as u8,
        },
        "text" | "varchar" | "character varying" | "char" | "character" | "name" => {
            PrimitiveType::Text {
                max: char_max_len.map(|n| n as u32),
            }
        }
        "bytea" => PrimitiveType::Bytes { max: None },
        "uuid" => PrimitiveType::Uuid,
        "timestamp with time zone" | "timestamptz" => {
            PrimitiveType::Timestamp { with_tz: true }
        }
        "timestamp without time zone" | "timestamp" => {
            PrimitiveType::Timestamp { with_tz: false }
        }
        "date" => PrimitiveType::Date,
        "time with time zone" | "timetz" | "time without time zone" | "time" => {
            PrimitiveType::Time
        }
        "interval" => PrimitiveType::Duration,
        "json" | "jsonb" => PrimitiveType::Any,
        _ => PrimitiveType::Text { max: None },
    };
    TosType::Primitive(base)
}

pub fn row_to_value(row: &PgRow) -> Result<TosValue, sqlx::Error> {
    use serde_json::{Map, Value};
    let mut obj = Map::new();
    for col in row.columns() {
        let name = col.name();
        let v: Option<Value> = row.try_get(name).ok();
        obj.insert(name.to_string(), v.unwrap_or(Value::Null));
    }
    Ok(TosValue(Value::Object(obj)))
}

#[async_trait]
impl TosAdapter for PostgresAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let mut s = TosSchema::new(&self.schema_name);
        let tables = self.list_tables().await?;
        for t in tables {
            let table = self.introspect_table(&t).await?;
            s.add_table(table);
        }
        Ok(s)
    }

    async fn read_records(&self, table: &str) -> Result<RecordStream, BoxedError> {
        let pool = self.pool.clone();
        let table_owned = table.to_string();
        let query = format!("SELECT * FROM {}", quote_ident(&table_owned));
        let stream = async_stream::try_stream! {
            let mut rows = sqlx::query(&query).fetch(&pool);
            while let Some(row) = rows.try_next().await? {
                yield row_to_value(&row)?;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn write_records(
        &self,
        table: &str,
        mut records: RecordStream,
    ) -> Result<u64, BoxedError> {
        let table_obj = self.introspect_table(table).await?;
        let cols: Vec<String> = table_obj.fields.iter().map(|f| f.name.clone()).collect();
        if cols.is_empty() {
            return Ok(0);
        }
        let mut count = 0u64;
        let mut tx = self.pool.begin().await?;
        while let Some(item) = records.try_next().await? {
            let obj = match &item.0 {
                serde_json::Value::Object(map) => map,
                other => {
                    return Err(PgAdapterError::Adapter(format!(
                        "write_records: expected JSON object, got {other}"
                    ))
                    .into());
                }
            };
            let sql = build_insert_sql(table, &cols);
            let mut query = sqlx::query(&sql);
            for c in &cols {
                query = bind_json_value(query, obj.get(c).cloned());
            }
            query.execute(&mut *tx).await?;
            count += 1;
        }
        tx.commit().await?;
        Ok(count)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(PgAdapterError::Adapter(
            "postgres watch is available in S5 with pgoutput / LISTEN-NOTIFY".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        self.pool.close().await;
        Ok(())
    }
}

pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn build_insert_sql(table: &str, cols: &[String]) -> String {
    let col_list = cols
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=cols.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident(table),
        col_list,
        placeholders
    )
}

pub fn bind_json_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: Option<serde_json::Value>,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    use serde_json::Value;
    match value {
        None | Some(Value::Null) => query.bind(Option::<String>::None),
        Some(Value::Bool(b)) => query.bind(b),
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        Some(Value::String(s)) => query.bind(s),
        Some(Value::Array(_) | Value::Object(_)) => query.bind(value.unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_type_basic_ints() {
        assert_eq!(
            pg_type_to_tos("integer", None, None, None),
            TosType::Primitive(PrimitiveType::Int32)
        );
        assert_eq!(
            pg_type_to_tos("bigint", None, None, None),
            TosType::Primitive(PrimitiveType::Int64)
        );
        assert_eq!(
            pg_type_to_tos("smallint", None, None, None),
            TosType::Primitive(PrimitiveType::Int16)
        );
    }

    #[test]
    fn pg_type_text_with_max() {
        let t = pg_type_to_tos("varchar", Some(255), None, None);
        assert_eq!(t, TosType::Primitive(PrimitiveType::Text { max: Some(255) }));
    }

    #[test]
    fn pg_type_text_unbounded() {
        let t = pg_type_to_tos("text", None, None, None);
        assert_eq!(t, TosType::Primitive(PrimitiveType::Text { max: None }));
    }

    #[test]
    fn pg_type_numeric() {
        let t = pg_type_to_tos("numeric", None, Some(10), Some(2));
        assert_eq!(
            t,
            TosType::Primitive(PrimitiveType::Decimal { precision: 10, scale: 2 })
        );
    }

    #[test]
    fn pg_type_bool() {
        assert_eq!(
            pg_type_to_tos("boolean", None, None, None),
            TosType::Primitive(PrimitiveType::Bool)
        );
    }

    #[test]
    fn pg_type_float() {
        assert_eq!(
            pg_type_to_tos("real", None, None, None),
            TosType::Primitive(PrimitiveType::Float32)
        );
        assert_eq!(
            pg_type_to_tos("double precision", None, None, None),
            TosType::Primitive(PrimitiveType::Float64)
        );
    }

    #[test]
    fn pg_type_uuid_timestamp() {
        assert_eq!(
            pg_type_to_tos("uuid", None, None, None),
            TosType::Primitive(PrimitiveType::Uuid)
        );
        assert_eq!(
            pg_type_to_tos("timestamp with time zone", None, None, None),
            TosType::Primitive(PrimitiveType::Timestamp { with_tz: true })
        );
        assert_eq!(
            pg_type_to_tos("timestamptz", None, None, None),
            TosType::Primitive(PrimitiveType::Timestamp { with_tz: true })
        );
    }

    #[test]
    fn pg_type_json_maps_to_any() {
        assert_eq!(
            pg_type_to_tos("jsonb", None, None, None),
            TosType::Primitive(PrimitiveType::Any)
        );
        assert_eq!(
            pg_type_to_tos("json", None, None, None),
            TosType::Primitive(PrimitiveType::Any)
        );
    }

    #[test]
    fn pg_type_unknown_defaults_to_text() {
        assert_eq!(
            pg_type_to_tos("xml", None, None, None),
            TosType::Primitive(PrimitiveType::Text { max: None })
        );
        assert_eq!(
            pg_type_to_tos("inet", None, None, None),
            TosType::Primitive(PrimitiveType::Text { max: None })
        );
    }

    #[test]
    fn pg_type_aliases() {
        assert_eq!(
            pg_type_to_tos("int4", None, None, None),
            TosType::Primitive(PrimitiveType::Int32)
        );
        assert_eq!(
            pg_type_to_tos("int8", None, None, None),
            TosType::Primitive(PrimitiveType::Int64)
        );
        assert_eq!(
            pg_type_to_tos("float4", None, None, None),
            TosType::Primitive(PrimitiveType::Float32)
        );
        assert_eq!(
            pg_type_to_tos("float8", None, None, None),
            TosType::Primitive(PrimitiveType::Float64)
        );
    }

    #[test]
    fn pg_type_bytes() {
        assert_eq!(
            pg_type_to_tos("bytea", None, None, None),
            TosType::Primitive(PrimitiveType::Bytes { max: None })
        );
    }

    #[test]
    fn pg_type_date_time() {
        assert_eq!(
            pg_type_to_tos("date", None, None, None),
            TosType::Primitive(PrimitiveType::Date)
        );
        assert_eq!(
            pg_type_to_tos("time", None, None, None),
            TosType::Primitive(PrimitiveType::Time)
        );
        assert_eq!(
            pg_type_to_tos("interval", None, None, None),
            TosType::Primitive(PrimitiveType::Duration)
        );
    }

    #[test]
    fn schema_from_url_basic() {
        assert_eq!(
            schema_from_url("postgres://user:pass@localhost:5432/mydb"),
            Some("mydb".to_string())
        );
    }

    #[test]
    fn schema_from_url_no_path() {
        assert_eq!(schema_from_url("postgres://localhost"), None);
    }

    #[test]
    fn schema_from_url_default_public() {
        assert_eq!(schema_from_url("postgres://localhost:5432"), None);
    }

    #[test]
    fn quote_ident_basic() {
        assert_eq!(quote_ident("users"), "\"users\"");
        assert_eq!(quote_ident("weird\"name"), "\"weird\"\"name\"");
    }

    #[test]
    fn build_insert_sql_single_col() {
        let s = build_insert_sql("users", &["id".to_string()]);
        assert_eq!(s, "INSERT INTO \"users\" (\"id\") VALUES ($1)");
    }

    #[test]
    fn build_insert_sql_multi_col() {
        let s = build_insert_sql("users", &["id".into(), "name".into(), "score".into()]);
        assert_eq!(
            s,
            "INSERT INTO \"users\" (\"id\", \"name\", \"score\") VALUES ($1, $2, $3)"
        );
    }

    #[test]
    fn build_insert_sql_quotes_special_idents() {
        let s = build_insert_sql("weird name", &["col\"x".into()]);
        assert_eq!(
            s,
            "INSERT INTO \"weird name\" (\"col\"\"x\") VALUES ($1)"
        );
    }

    #[test]
    fn build_insert_sql_empty_cols_returns_invalid() {
        let s = build_insert_sql("x", &[]);
        assert_eq!(s, "INSERT INTO \"x\" () VALUES ()");
    }
}
