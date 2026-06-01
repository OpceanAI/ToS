use std::path::{Path, PathBuf};
use std::sync::RwLock;

use async_trait::async_trait;
use futures::TryStreamExt;
use serde_json::{Map, Value};
use thiserror::Error;
use tos_core::adapter::{BoxedError, ChangeStream, RecordStream, TosAdapter, TosValue};
use tos_core::sdl::{FieldIndex, TosField, TosSchema, TosTable};
use tos_core::types::{PrimitiveType, TosType};

#[derive(Debug, Error)]
pub enum TxtAdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("adapter: {0}")]
    Adapter(String),
}

pub enum Delimiter {
    Tab,
    Comma,
    Pipe,
    Custom(char),
}

impl Delimiter {
    pub fn as_char(&self) -> char {
        match self {
            Delimiter::Tab => '\t',
            Delimiter::Comma => ',',
            Delimiter::Pipe => '|',
            Delimiter::Custom(c) => *c,
        }
    }

    pub fn from_name(s: &str) -> Self {
        match s {
            "tab" | "tsv" => Delimiter::Tab,
            "csv" | "comma" => Delimiter::Comma,
            "pipe" | "psv" => Delimiter::Pipe,
            other if other.chars().count() == 1 => {
                Delimiter::Custom(other.chars().next().unwrap())
            }
            _ => Delimiter::Comma,
        }
    }
}

pub struct TxtAdapter {
    name: String,
    path: PathBuf,
    delim: Delimiter,
    has_header: bool,
    records: RwLock<Vec<TosValue>>,
    schema: RwLock<Option<TosSchema>>,
}

impl TxtAdapter {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>, delim: Delimiter) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            delim,
            has_header: true,
            records: RwLock::new(Vec::new()),
            schema: RwLock::new(None),
        }
    }

    pub fn open(
        name: impl Into<String>,
        path: impl AsRef<Path>,
        delim: Delimiter,
    ) -> Result<Self, TxtAdapterError> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            parse_records(&raw, &delim, true).map_err(TxtAdapterError::Parse)?
        } else {
            Vec::new()
        };
        Ok(Self {
            name: name.into(),
            path,
            delim,
            has_header: true,
            records: RwLock::new(records),
            schema: RwLock::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn records(&self) -> Vec<TosValue> {
        self.records.read().expect("txt lock poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.records.read().expect("txt lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save(&self) -> Result<(), TxtAdapterError> {
        let records = self.records.read().expect("txt lock poisoned");
        let headers: Vec<String> = records
            .first()
            .and_then(|v| if let Value::Object(m) = &v.0 { Some(m.keys().cloned().collect()) } else { None })
            .unwrap_or_default();
        let delim_char = self.delim.as_char();
        let mut out = String::new();
        if self.has_header && !headers.is_empty() {
            out.push_str(&headers.join(&delim_char.to_string()));
            out.push('\n');
        }
        for v in records.iter() {
            if let Value::Object(m) = &v.0 {
                let row: Vec<String> = headers
                    .iter()
                    .map(|h| match m.get(h) {
                        Some(val) => value_to_csv_cell(val, delim_char),
                        None => String::new(),
                    })
                    .collect();
                out.push_str(&row.join(&delim_char.to_string()));
                out.push('\n');
            }
        }
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, out)?;
        Ok(())
    }
}

fn parse_records(
    raw: &str,
    delim: &Delimiter,
    has_header: bool,
) -> Result<Vec<TosValue>, String> {
    let d = delim.as_char().to_string();
    let mut lines = raw.lines();
    let headers: Vec<String> = if has_header {
        match lines.next() {
            Some(line) => line.split(&d).map(|s| s.trim().to_string()).collect(),
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(&d).collect();
        let mut obj = Map::new();
        for (i, p) in parts.iter().enumerate() {
            let key = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("col{i}"));
            obj.insert(key, parse_cell(p));
        }
        out.push(TosValue(Value::Object(obj)));
    }
    Ok(out)
}

fn parse_cell(s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Ok(f) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
    }
    Value::String(s.to_string())
}

fn value_to_csv_cell(v: &Value, delim: char) -> String {
    match v {
        Value::String(s) => {
            if s.contains(delim) || s.contains('"') || s.contains('\n') {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.clone()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
}

#[async_trait]
impl TosAdapter for TxtAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn read_schema(&self) -> Result<TosSchema, BoxedError> {
        let guard = self.schema.read().expect("txt lock poisoned");
        if let Some(s) = guard.as_ref() {
            return Ok(s.clone());
        }
        drop(guard);
        let s = derive_schema(&self.records.read().expect("txt lock poisoned"));
        *self.schema.write().expect("txt lock poisoned") = Some(s.clone());
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
            let mut guard = self.records.write().expect("txt lock poisoned");
            guard.extend(new_records);
        }
        self.save()?;
        Ok(count)
    }

    async fn watch(&self, _table: &str) -> Result<ChangeStream, BoxedError> {
        Err(TxtAdapterError::Adapter(
            "txt watch is available via inotify in S6".into(),
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
        name: "txt".to_string(),
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

impl TxtAdapter {
    pub fn with_records(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        delim: Delimiter,
        records: Vec<TosValue>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            delim,
            has_header: true,
            records: RwLock::new(records),
            schema: RwLock::new(None),
        }
    }

    pub fn read_schema_sync(&self) -> TosSchema {
        self.schema
            .read()
            .expect("txt lock poisoned")
            .clone()
            .unwrap_or_else(|| derive_schema(&self.records.read().expect("txt lock poisoned")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(s: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tos-txt-{}-{}.csv", std::process::id(), s));
        p
    }

    fn cleanup(p: &Path) {
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn delimiter_as_char() {
        assert_eq!(Delimiter::Tab.as_char(), '\t');
        assert_eq!(Delimiter::Comma.as_char(), ',');
        assert_eq!(Delimiter::Pipe.as_char(), '|');
        assert_eq!(Delimiter::Custom(';').as_char(), ';');
    }

    #[test]
    fn delimiter_from_name() {
        assert_eq!(Delimiter::from_name("tsv").as_char(), '\t');
        assert_eq!(Delimiter::from_name("csv").as_char(), ',');
        assert_eq!(Delimiter::from_name("psv").as_char(), '|');
        assert_eq!(Delimiter::from_name(";").as_char(), ';');
        assert_eq!(Delimiter::from_name("unknown").as_char(), ',');
    }

    #[test]
    fn parse_cell_classifies() {
        assert_eq!(parse_cell("42"), Value::Number(42.into()));
        assert_eq!(parse_cell("true"), Value::Bool(true));
        assert_eq!(parse_cell("false"), Value::Bool(false));
        assert_eq!(parse_cell("1.5"), Value::Number(serde_json::Number::from_f64(1.5).unwrap()));
        assert_eq!(parse_cell("hello"), Value::String("hello".into()));
        assert_eq!(parse_cell(""), Value::Null);
    }

    #[test]
    fn parse_records_basic() {
        let raw = "id,name,score\n1,alice,1.5\n2,bob,2.5\n";
        let r = parse_records(raw, &Delimiter::Comma, true).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0["id"], 1);
        assert_eq!(r[1].0["name"], "bob");
    }

    #[test]
    fn parse_records_tsv() {
        let raw = "id\tname\n1\talice\n2\tbob\n";
        let r = parse_records(raw, &Delimiter::Tab, true).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0["id"], 1);
    }

    #[test]
    fn parse_records_no_header() {
        let raw = "1,alice\n2,bob\n";
        let r = parse_records(raw, &Delimiter::Comma, false).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0["col0"], 1);
        assert_eq!(r[0].0["col1"], "alice");
    }

    #[test]
    fn parse_records_skips_empty_lines() {
        let raw = "id,name\n1,a\n\n2,b\n\n";
        let r = parse_records(raw, &Delimiter::Comma, true).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn csv_cell_quoting() {
        assert_eq!(value_to_csv_cell(&Value::String("a,b".into()), ','), "\"a,b\"");
        assert_eq!(value_to_csv_cell(&Value::String("a\"b".into()), ','), "\"a\"\"b\"");
        assert_eq!(value_to_csv_cell(&Value::String("plain".into()), ','), "plain");
        assert_eq!(value_to_csv_cell(&Value::Bool(true), ','), "true");
        assert_eq!(value_to_csv_cell(&Value::Null, ','), "");
    }

    #[test]
    fn open_missing_file() {
        let p = temp_path("missing");
        cleanup(&p);
        let a = TxtAdapter::open("a", &p, Delimiter::Comma).unwrap();
        assert!(a.is_empty());
        cleanup(&p);
    }

    #[test]
    fn open_existing_loads() {
        let p = temp_path("existing");
        std::fs::write(&p, "id,name\n1,a\n2,b\n3,c\n").unwrap();
        let a = TxtAdapter::open("a", &p, Delimiter::Comma).unwrap();
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
        ];
        let a = TxtAdapter::with_records("a", &p, Delimiter::Comma, initial.clone());
        a.save().unwrap();
        let b = TxtAdapter::open("b", &p, Delimiter::Comma).unwrap();
        assert_eq!(b.records(), initial);
        cleanup(&p);
    }

    #[test]
    fn read_schema_inferred() {
        let records = vec![TosValue(serde_json::json!({
            "id": 1, "name": "x", "score": 0.5
        }))];
        let a = TxtAdapter::with_records("a", temp_path("schema"), Delimiter::Comma, records);
        let s = a.read_schema_sync();
        let t = s.get_table("rows").unwrap();
        let id_f = t.fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_f.ty, TosType::Primitive(PrimitiveType::Int64));
    }
}