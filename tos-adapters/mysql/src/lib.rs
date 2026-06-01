use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::TryStreamExt;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::{Column, Row};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{FieldIndex, TosField, TosIndex, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum MyAdapterError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uri: {0}")]
    InvalidUri(String),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub struct MysqlAdapter {
    name: String,
    pool: MySqlPool,
    db_name: String,
}

impl MysqlAdapter {
    pub async fn connect(url: &str) -> Result<Self, MyAdapterError> {
        let pool = MySqlPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await?;
        let db_name = schema_from_url(url).unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            name: format!("mysql://{url}"),
            pool,
            db_name,
        })
    }

    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    pub async fn list_tables(&self) -> Result<Vec<String>, MyAdapterError> {
        let rows = sqlx::query("SHOW TABLES")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>(0).ok())
            .collect())
    }

    pub async fn introspect_table(&self, table: &str) -> Result<TosTable, MyAdapterError> {
        let rows = sqlx::query(&format!("SHOW COLUMNS FROM `{}`", table.replace('`', "``")))
            .fetch_all(&self.pool)
            .await?;
        let mut primary_keys: Vec<String> = Vec::new();
        if let Ok(idx_rows) = sqlx::query(&format!("SHOW INDEX FROM `{}`", table.replace('`', "``")))
            .fetch_all(&self.pool)
            .await
        {
            for r in idx_rows {
                let key_name: String = r.try_get("Key_name").unwrap_or_default();
                if key_name == "PRIMARY" {
                    let col: String = r.try_get("Column_name").unwrap_or_default();
                    if !col.is_empty() && !primary_keys.contains(&col) {
                        primary_keys.push(col);
                    }
                }
            }
        }
        let mut fields = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let name: String = row.try_get("Field").unwrap_or_default();
            let ty: String = row.try_get("Type").unwrap_or_default();
            let null: String = row.try_get("Null").unwrap_or_default();
            let key: String = row.try_get("Key").unwrap_or_default();
            let is_pk = primary_keys.contains(&name) || key == "PRI";
            fields.push(TosField {
                name: name.clone(),
                ty: TosType::Primitive(mysql_type_to_tos(&ty)),
                nullable: null.eq_ignore_ascii_case("YES"),
                primary: is_pk,
                unique: key == "UNI",
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
                    fields: primary_keys,
                    unique: true,
                },
            );
        }
        Ok(TosTable {
            name: table.to_string(),
            fields,
            indexes,
            relations: BTreeMap::new(),
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

pub fn mysql_type_to_tos(mysql_type: &str) -> PrimitiveType {
    let base = mysql_type.split('(').next().unwrap_or(mysql_type).to_lowercase();
    match base.as_str() {
        "tinyint" | "smallint" => PrimitiveType::Int16,
        "mediumint" | "int" | "integer" => PrimitiveType::Int32,
        "bigint" => PrimitiveType::Int64,
        "float" => PrimitiveType::Float32,
        "double" | "real" => PrimitiveType::Float64,
        "decimal" | "numeric" => PrimitiveType::Decimal { precision: 38, scale: 0 },
        "bit" | "boolean" | "bool" => PrimitiveType::Bool,
        "char" | "varchar" | "tinytext" | "text" | "mediumtext" | "longtext" | "enum" | "set" => {
            PrimitiveType::Text { max: None }
        }
        "binary" | "varbinary" | "tinyblob" | "blob" | "mediumblob" | "longblob" => {
            PrimitiveType::Bytes { max: None }
        }
        "date" => PrimitiveType::Date,
        "time" => PrimitiveType::Time,
        "datetime" | "timestamp" => PrimitiveType::Timestamp { with_tz: false },
        "year" => PrimitiveType::Int16,
        "json" => PrimitiveType::Any,
        _ => PrimitiveType::Text { max: None },
    }
}

pub fn row_to_value(row: &MySqlRow) -> Result<TosValue, sqlx::Error> {
    use serde_json::{Map, Value};
    let mut obj = Map::new();
    for col in row.columns() {
        let name = col.name();
        let v: Option<String> = row.try_get(name).ok();
        obj.insert(
            name.to_string(),
            v.map(Value::String).unwrap_or(Value::Null),
        );
    }
    Ok(TosValue(Value::Object(obj)))
}

#[async_trait]
impl TosAdapter for MysqlAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let mut s = TosSchema::new(&self.db_name);
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
        let query = format!("SELECT * FROM `{}`", table_owned.replace('`', "``"));
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
                    return Err(MyAdapterError::Adapter(format!(
                        "write_records: expected JSON object, got {other}"
                    ))
                    .into());
                }
            };
            let sql = build_insert_sql(table, &cols);
            let mut query = sqlx::query(&sql);
            for c in &cols {
                query = bind_mysql_value(query, obj.get(c).cloned());
            }
            query.execute(&mut *tx).await?;
            count += 1;
        }
        tx.commit().await?;
        Ok(count)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(MyAdapterError::Adapter(
            "mysql watch is available in S6 (binlog)".into(),
        )
        .into())
    }

    async fn close(&self) -> Result<(), BoxedError> {
        self.pool.close().await;
        Ok(())
    }
}

pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

pub fn build_insert_sql(table: &str, cols: &[String]) -> String {
    let col_list = cols
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = vec!["?"; cols.len()].join(", ");
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident(table),
        col_list,
        placeholders
    )
}

pub fn bind_mysql_value<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    value: Option<serde_json::Value>,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
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
        Some(other) => query.bind(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_type_basic_ints() {
        assert_eq!(mysql_type_to_tos("INT"), PrimitiveType::Int32);
        assert_eq!(mysql_type_to_tos("BIGINT"), PrimitiveType::Int64);
        assert_eq!(mysql_type_to_tos("TINYINT"), PrimitiveType::Int16);
        assert_eq!(mysql_type_to_tos("SMALLINT"), PrimitiveType::Int16);
    }

    #[test]
    fn mysql_type_floats() {
        assert_eq!(mysql_type_to_tos("FLOAT"), PrimitiveType::Float32);
        assert_eq!(mysql_type_to_tos("DOUBLE"), PrimitiveType::Float64);
        assert_eq!(mysql_type_to_tos("REAL"), PrimitiveType::Float64);
    }

    #[test]
    fn mysql_type_text() {
        assert_eq!(mysql_type_to_tos("VARCHAR(255)"), PrimitiveType::Text { max: None });
        assert_eq!(mysql_type_to_tos("TEXT"), PrimitiveType::Text { max: None });
        assert_eq!(mysql_type_to_tos("CHAR(10)"), PrimitiveType::Text { max: None });
        assert_eq!(mysql_type_to_tos("ENUM('a','b')"), PrimitiveType::Text { max: None });
    }

    #[test]
    fn mysql_type_bool() {
        assert_eq!(mysql_type_to_tos("BOOLEAN"), PrimitiveType::Bool);
        assert_eq!(mysql_type_to_tos("BOOL"), PrimitiveType::Bool);
        assert_eq!(mysql_type_to_tos("BIT"), PrimitiveType::Bool);
    }

    #[test]
    fn mysql_type_dates() {
        assert_eq!(mysql_type_to_tos("DATE"), PrimitiveType::Date);
        assert_eq!(mysql_type_to_tos("TIME"), PrimitiveType::Time);
        assert_eq!(mysql_type_to_tos("DATETIME"), PrimitiveType::Timestamp { with_tz: false });
        assert_eq!(mysql_type_to_tos("TIMESTAMP"), PrimitiveType::Timestamp { with_tz: false });
        assert_eq!(mysql_type_to_tos("YEAR"), PrimitiveType::Int16);
    }

    #[test]
    fn mysql_type_special() {
        assert_eq!(mysql_type_to_tos("JSON"), PrimitiveType::Any);
        assert_eq!(mysql_type_to_tos("BLOB"), PrimitiveType::Bytes { max: None });
        assert_eq!(mysql_type_to_tos("DECIMAL(10,2)"), PrimitiveType::Decimal { precision: 38, scale: 0 });
        assert_eq!(mysql_type_to_tos("UNKNOWN_TYPE"), PrimitiveType::Text { max: None });
    }

    #[test]
    fn schema_from_url_basic() {
        assert_eq!(
            schema_from_url("mysql://user:pass@host:3306/mydb"),
            Some("mydb".to_string())
        );
    }

    #[test]
    fn schema_from_url_no_path() {
        assert_eq!(schema_from_url("mysql://host"), None);
    }

    #[test]
    fn schema_from_url_with_query() {
        assert_eq!(
            schema_from_url("mysql://host:3306/mydb?charset=utf8"),
            Some("mydb".to_string())
        );
    }
}
