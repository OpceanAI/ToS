use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::TryStreamExt;
use rusqlite::{params_from_iter, Connection, Row};
use serde_json::{Map, Value};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{FieldIndex, TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum SqliteAdapterError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct SqliteAdapter {
    name: String,
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl SqliteAdapter {
    pub fn open(name: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self, SqliteAdapterError> {
        let path: PathBuf = path.into();
        let conn = Connection::open(&path)?;
        Ok(Self {
            name: name.into(),
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    pub fn open_in_memory(name: impl Into<String>) -> Result<Self, SqliteAdapterError> {
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            name: name.into(),
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn list_tables(&self) -> Result<Vec<String>, SqliteAdapterError> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        Ok(names)
    }

    pub fn introspect_table(&self, table: &str) -> Result<TosTable, SqliteAdapterError> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\"")))?;
        let rows: Vec<TableInfo> = stmt
            .query_map([], |r| {
                Ok(TableInfo {
                    name: r.get::<_, String>(1)?,
                    ty: r.get::<_, String>(2)?,
                    notnull: r.get::<_, i64>(3)? != 0,
                    pk: r.get::<_, i64>(5)? as i32,
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut fields = Vec::new();
        for (i, info) in rows.iter().enumerate() {
            fields.push(TosField {
                name: info.name.clone(),
                ty: TosType::Primitive(sqlite_type_to_tos(&info.ty)),
                nullable: !info.notnull,
                primary: info.pk > 0,
                unique: false,
                default: None,
                index: if info.pk > 0 { Some(FieldIndex { order: info.pk }) } else { None },
                comment: None,
            });
            let _ = i;
        }
        Ok(TosTable {
            name: table.to_string(),
            fields,
            indexes: std::collections::BTreeMap::new(),
            relations: std::collections::BTreeMap::new(),
        })
    }

    pub fn execute(&self, sql: &str) -> Result<(), SqliteAdapterError> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        conn.execute(sql, [])?;
        Ok(())
    }
}

struct TableInfo {
    name: String,
    ty: String,
    notnull: bool,
    pk: i32,
}

pub fn sqlite_type_to_tos(sqlite_type: &str) -> PrimitiveType {
    let t = sqlite_type.to_uppercase();
    match t.as_str() {
        "INTEGER" | "INT" | "BIGINT" => PrimitiveType::Int64,
        "REAL" | "DOUBLE" | "FLOAT" | "NUMERIC" => PrimitiveType::Float64,
        "BOOLEAN" | "BOOL" => PrimitiveType::Bool,
        "TEXT" | "VARCHAR" | "CHAR" => PrimitiveType::Text { max: None },
        "BLOB" | "BYTEA" => PrimitiveType::Bytes { max: None },
        "TIMESTAMP" | "DATETIME" => PrimitiveType::Timestamp { with_tz: false },
        "DATE" => PrimitiveType::Date,
        _ => PrimitiveType::Text { max: None },
    }
}

fn row_to_value(row: &Row, cols: &[String]) -> Result<TosValue, rusqlite::Error> {
    let mut obj = Map::new();
    for (i, c) in cols.iter().enumerate() {
        let v = decode_column(row, i);
        obj.insert(c.clone(), v);
    }
    Ok(TosValue(Value::Object(obj)))
}

fn decode_column(row: &Row, i: usize) -> Value {
    if let Ok(Some(s)) = row.get::<_, Option<String>>(i) {
        return Value::String(s);
    }
    if let Ok(Some(b)) = row.get::<_, Option<bool>>(i) {
        return Value::Bool(b);
    }
    if let Ok(Some(n)) = row.get::<_, Option<i64>>(i) {
        return Value::Number(n.into());
    }
    if let Ok(Some(f)) = row.get::<_, Option<f64>>(i) {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
    }
    if let Ok(Some(n)) = row.get::<_, Option<i32>>(i) {
        return Value::Number(n.into());
    }
    if let Ok(Some(n)) = row.get::<_, Option<i16>>(i) {
        return Value::Number(n.into());
    }
    Value::Null
}

#[async_trait]
impl TosAdapter for SqliteAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let conn = self.conn.clone();
        let tables = tokio::task::spawn_blocking(move || {
            let c = conn.lock().expect("sqlite lock poisoned");
            let names: Vec<String> = {
                let mut stmt = c.prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )?;
                let v: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()?;
                v
            };
            let mut schema = TosSchema::new("sqlite");
            for name in &names {
                let mut stmt = c.prepare(&format!(
                    "PRAGMA table_info(\"{}\")",
                    name.replace('"', "\"\"")
                ))?;
                let rows: Vec<TableInfo> = stmt
                    .query_map([], |r| {
                        Ok(TableInfo {
                            name: r.get::<_, String>(1)?,
                            ty: r.get::<_, String>(2)?,
                            notnull: r.get::<_, i64>(3)? != 0,
                            pk: r.get::<_, i64>(5)? as i32,
                        })
                    })?
                    .collect::<Result<_, _>>()?;
                let mut fields = Vec::new();
                for info in &rows {
                    fields.push(TosField {
                        name: info.name.clone(),
                        ty: TosType::Primitive(sqlite_type_to_tos(&info.ty)),
                        nullable: !info.notnull,
                        primary: info.pk > 0,
                        unique: false,
                        default: None,
                        index: if info.pk > 0 {
                            Some(FieldIndex { order: info.pk })
                        } else {
                            None
                        },
                        comment: None,
                    });
                }
                schema.add_table(TosTable {
                    name: name.clone(),
                    fields,
                    indexes: std::collections::BTreeMap::new(),
                    relations: std::collections::BTreeMap::new(),
                });
            }
            Ok::<_, SqliteAdapterError>(schema)
        })
        .await
        .map_err(|e| -> BoxedError { Box::new(std::io::Error::other(format!("join: {e}"))) })??;
        Ok(tables)
    }

    async fn read_records(&self, table: &str) -> Result<RecordStream, BoxedError> {
        let conn = self.conn.clone();
        let table = table.to_string();
        let stream = async_stream::try_stream! {
            let (records, cols) = tokio::task::spawn_blocking(move || {
                let c = conn.lock().expect("sqlite lock poisoned");
                let mut stmt = c.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
                let cols: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
                let rows: Vec<TosValue> = stmt
                    .query_map([], |r| row_to_value(r, &cols))?
                    .collect::<Result<_, _>>()?;
                Ok::<_, SqliteAdapterError>((rows, cols))
            })
            .await
            .map_err(|e| SqliteAdapterError::Adapter(format!("join: {e}")))??;
            for v in records {
                yield v;
            }
            let _ = cols;
        };
        Ok(Box::pin(stream))
    }

    async fn write_records(
        &self,
        table: &str,
        mut records: RecordStream,
    ) -> Result<u64, BoxedError> {
        let mut collected: Vec<TosValue> = Vec::new();
        while let Some(item) = records.try_next().await? {
            collected.push(item);
        }
        let conn = self.conn.clone();
        let table = table.to_string();
        let n = tokio::task::spawn_blocking(move || -> Result<u64, SqliteAdapterError> {
            let c = conn.lock().expect("sqlite lock poisoned");
            let tx = c.unchecked_transaction()?;
            let mut count = 0u64;
            for v in collected {
                let obj = match &v.0 {
                    Value::Object(m) => m,
                    _ => {
                        return Err(SqliteAdapterError::Adapter(
                            "expected JSON object".into(),
                        ));
                    }
                };
                if obj.is_empty() {
                    continue;
                }
                let cols: Vec<String> = obj.keys().cloned().collect();
                let placeholders = (1..=cols.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let col_list = cols
                    .iter()
                    .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "INSERT INTO \"{}\" ({}) VALUES ({})",
                    table.replace('"', "\"\""),
                    col_list,
                    placeholders
                );
                let values: Vec<Box<dyn rusqlite::ToSql>> = cols
                    .iter()
                    .map(|c| match obj.get(c) {
                        Some(Value::String(s)) => Box::new(s.clone()) as Box<dyn rusqlite::ToSql>,
                        Some(Value::Number(n)) => {
                            if let Some(i) = n.as_i64() {
                                Box::new(i) as Box<dyn rusqlite::ToSql>
                            } else if let Some(f) = n.as_f64() {
                                Box::new(f) as Box<dyn rusqlite::ToSql>
                            } else {
                                Box::new(n.to_string()) as Box<dyn rusqlite::ToSql>
                            }
                        }
                        Some(Value::Bool(b)) => Box::new(*b) as Box<dyn rusqlite::ToSql>,
                        Some(Value::Null) => Box::new(Option::<String>::None) as Box<dyn rusqlite::ToSql>,
                        Some(other) => Box::new(other.to_string()) as Box<dyn rusqlite::ToSql>,
                        None => Box::new(Option::<String>::None) as Box<dyn rusqlite::ToSql>,
                    })
                    .collect();
                tx.execute(&sql, params_from_iter(values.iter()))?;
                count += 1;
            }
            tx.commit()?;
            Ok(count)
        })
        .await
        .map_err(|e| -> BoxedError { Box::new(std::io::Error::other(format!("join: {e}"))) })??;
        Ok(n)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(SqliteAdapterError::Adapter(
            "sqlite watch is available in S6 with sqlite update hooks".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_type_to_tos_basic() {
        assert_eq!(sqlite_type_to_tos("INTEGER"), PrimitiveType::Int64);
        assert_eq!(sqlite_type_to_tos("TEXT"), PrimitiveType::Text { max: None });
        assert_eq!(sqlite_type_to_tos("REAL"), PrimitiveType::Float64);
        assert_eq!(sqlite_type_to_tos("BLOB"), PrimitiveType::Bytes { max: None });
        assert_eq!(sqlite_type_to_tos("BOOLEAN"), PrimitiveType::Bool);
        assert_eq!(sqlite_type_to_tos("UNKNOWN"), PrimitiveType::Text { max: None });
    }

    #[test]
    fn introspect_in_memory_db() {
        let a = SqliteAdapter::open_in_memory("test").unwrap();
        a.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
            .unwrap();
        a.execute("INSERT INTO users VALUES (1, 'alice', 1.5)").unwrap();
        a.execute("INSERT INTO users VALUES (2, 'bob', 2.5)").unwrap();
        let tables = a.list_tables().unwrap();
        assert_eq!(tables, vec!["users"]);
        let t = a.introspect_table("users").unwrap();
        assert_eq!(t.name, "users");
        let id_f = t.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_f.ty, TosType::Primitive(PrimitiveType::Int64));
        assert!(id_f.primary);
        let name_f = t.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(
            name_f.ty,
            TosType::Primitive(PrimitiveType::Text { max: None })
        );
        let score_f = t.fields.iter().find(|f| f.name == "score").unwrap();
        assert_eq!(score_f.ty, TosType::Primitive(PrimitiveType::Float64));
    }

    #[tokio::test]
    async fn read_schema_lists_tables() {
        let a = SqliteAdapter::open_in_memory("test").unwrap();
        a.execute("CREATE TABLE t1 (id INTEGER, x TEXT)").unwrap();
        a.execute("CREATE TABLE t2 (id INTEGER, y INTEGER)").unwrap();
        let s = a.read_schema().await.unwrap();
        assert!(s.get_table("t1").is_some());
        assert!(s.get_table("t2").is_some());
    }

    #[tokio::test]
    async fn read_records_returns_all_rows() {
        let a = SqliteAdapter::open_in_memory("test").unwrap();
        a.execute("CREATE TABLE users (id INTEGER, name TEXT)").unwrap();
        a.execute("INSERT INTO users VALUES (1, 'alice')").unwrap();
        a.execute("INSERT INTO users VALUES (2, 'bob')").unwrap();
        a.execute("INSERT INTO users VALUES (3, 'carol')").unwrap();
        let mut s = a.read_records("users").await.unwrap();
        let mut count = 0;
        while let Some(v) = s.try_next().await.unwrap() {
            assert!(v.0.get("id").is_some());
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn write_records_inserts_rows() {
        let a = SqliteAdapter::open_in_memory("test").unwrap();
        a.execute("CREATE TABLE users (id INTEGER, name TEXT)").unwrap();
        let records: Vec<TosValue> = (0..3)
            .map(|i| TosValue(serde_json::json!({"id": i, "name": format!("u-{i}")})))
            .collect();
        let stream = async_stream::try_stream! {
            for v in records {
                yield v;
            }
        };
        let n = a
            .write_records("users", Box::pin(stream))
            .await
            .unwrap();
        assert_eq!(n, 3);
        let tables = a.list_tables().unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn execute_idempotent_create() {
        let a = SqliteAdapter::open_in_memory("test").unwrap();
        a.execute("CREATE TABLE t (id INTEGER)").unwrap();
        let res = a.execute("CREATE TABLE t (id INTEGER)");
        assert!(res.is_err());
    }
}
