use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use tos_core::adapter::TosAdapter;
use tos_core::sdl::{infer_schema_csv, infer_schema_json, parse_sdl, to_sdl, validate, write_diff_table, TosSchema};
use tos_core::MockAdapter;

use crate::uri::{parse, Scheme};

pub async fn pull(uri_str: &str) -> Result<()> {
    let uri = parse(uri_str).context("invalid URI")?;
    let adapter: Arc<dyn TosAdapter> = build_for_pull(&uri).await?;
    let schema = adapter
        .read_schema()
        .await
        .map_err(|e| anyhow!("read_schema failed: {e}"))?;
    let sdl = to_sdl(&schema);
    print!("{sdl}");
    Ok(())
}

pub async fn push(file: &Path, uri_str: &str) -> Result<()> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("reading SDL file {}", file.display()))?;
    let schema = parse_sdl(&text).map_err(|e| anyhow!("SDL parse error: {e}"))?;
    validate(&schema).map_err(|e| anyhow!("SDL validation error: {e}"))?;
    let uri = parse(uri_str).context("invalid URI")?;

    match uri.scheme {
        Scheme::Json | Scheme::Jsonl | Scheme::Yaml | Scheme::Txt => {
            let sidecar = sidecar_for(&uri.dataset, file);
            std::fs::write(&sidecar, to_sdl(&schema))
                .with_context(|| format!("writing sidecar {}", sidecar.display()))?;
            println!(
                "schema written to {} (sidecar of {})",
                sidecar.display(),
                uri.dataset
            );
            Ok(())
        }
        Scheme::Postgres | Scheme::Mysql | Scheme::Sqlite => {
            let requested = uri
                .params
                .get("table")
                .cloned()
                .or_else(|| {
                    schema
                        .tables
                        .keys()
                        .next()
                        .map(|s| s.to_string())
                });
            for table in schema.tables.values() {
                let stmt = if let Some(ref want) = requested {
                    let mut t = table.clone();
                    if want != &t.name {
                        t.name = want.clone();
                    }
                    generate_create_table(&uri.scheme, &t)
                } else {
                    generate_create_table(&uri.scheme, table)
                };
                println!("{stmt};");
            }
            println!("# dry-run: not executed against the database");
            Ok(())
        }
        _ => Err(anyhow!(
            "schema push not supported for scheme `{}` in v1.0",
            uri.scheme.as_str()
        )),
    }
}

pub async fn infer(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let table_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("inferred")
        .to_string();
    let schema = match ext.as_str() {
        "json" | "jsonl" | "ndjson" => {
            let records = parse_json_records(&text);
            if records.is_empty() {
                return Err(anyhow!("no JSON records found in {}", path.display()));
            }
            let tbl = infer_schema_json(table_name, records)
                .map_err(|e| anyhow!("infer_schema_json: {e}"))?;
            TosSchema {
                name: "inferred".into(),
                version: "0.1.0".into(),
                tables: [(tbl.name.clone(), tbl)].into_iter().collect(),
            }
        }
        "csv" | "tsv" | "txt" => {
            let delim = if ext == "tsv" { b'\t' } else { b',' };
            let mut reader = text.as_bytes();
            let tbl = infer_schema_csv(table_name, &mut reader, true, delim)
                .map_err(|e| anyhow!("infer_schema_csv: {e}"))?;
            TosSchema {
                name: "inferred".into(),
                version: "0.1.0".into(),
                tables: [(tbl.name.clone(), tbl)].into_iter().collect(),
            }
        }
        "yaml" | "yml" => {
            use tos_core::sdl::TosField;
            use tos_core::types::TosType;
            use tos_core::types::PrimitiveType;
            let de: serde_json::Value = serde_yaml::from_str(&text)
                .map_err(|e| anyhow!("yaml parse: {e}"))?;
            let arr = match de {
                serde_json::Value::Array(a) => a,
                other => vec![other],
            };
            if arr.is_empty() {
                return Err(anyhow!("no YAML records found in {}", path.display()));
            }
            let mut fields: Vec<TosField> = Vec::new();
            if let Some(serde_json::Value::Object(obj)) = arr.first() {
                for (i, (k, v)) in obj.iter().enumerate() {
                    let prim = match v {
                        serde_json::Value::Bool(_) => PrimitiveType::Bool,
                        serde_json::Value::Number(n) => {
                            if n.as_i64().is_some() {
                                PrimitiveType::Int64
                            } else {
                                PrimitiveType::Float64
                            }
                        }
                        serde_json::Value::String(_) => PrimitiveType::Text { max: None },
                        _ => PrimitiveType::Any,
                    };
                    fields.push(TosField {
                        name: k.clone(),
                        ty: TosType::Primitive(prim),
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
            }
            let tbl = tos_core::sdl::TosTable {
                name: table_name.clone(),
                key: vec![],
                fields,
                indexes: std::collections::BTreeMap::new(),
                relations: std::collections::BTreeMap::new(),
            };
            TosSchema {
                name: "inferred".into(),
                version: "0.1.0".into(),
                tables: [(tbl.name.clone(), tbl)].into_iter().collect(),
            }
        }
        "toml" | "tos" => {
            parse_sdl(&text).map_err(|e| anyhow!("SDL parse error: {e}"))?
        }
        other => {
            return Err(anyhow!(
                "cannot infer from extension `.{other}` (use .json, .jsonl, .csv, .tsv, .yaml, or .toml)"
            ));
        }
    };
    print!("{}", to_sdl(&schema));
    Ok(())
}

pub fn diff(file1: &Path, file2: &Path) -> Result<()> {
    let a = load_sdl(file1)?;
    let b = load_sdl(file2)?;
    let mut out = String::new();
    out.push_str(&format!("left  = {}\n", file1.display()));
    out.push_str(&format!("right = {}\n\n", file2.display()));

    let mut left_tables: Vec<&str> = a.tables.keys().map(|s| s.as_str()).collect();
    let mut right_tables: Vec<&str> = b.tables.keys().map(|s| s.as_str()).collect();
    left_tables.sort();
    right_tables.sort();

    let mut all: Vec<&str> = left_tables.clone();
    for t in &right_tables {
        if !all.contains(t) {
            all.push(t);
        }
    }
    all.sort();

    for tname in all {
        match (a.tables.get(tname), b.tables.get(tname)) {
            (Some(ta), Some(tb)) => {
                write_diff_table(&mut out, &format!("table `{tname}`"), ta, tb);
            }
            (Some(ta), None) => {
                out.push_str(&format!("--- table `{tname}` (only in left)\n"));
                out.push_str(&format!("  fields: {}\n", ta.fields.len()));
            }
            (None, Some(tb)) => {
                out.push_str(&format!("+++ table `{tname}` (only in right)\n"));
                out.push_str(&format!("  fields: {}\n", tb.fields.len()));
            }
            (None, None) => {}
        }
        out.push('\n');
    }
    print!("{out}");
    Ok(())
}

pub fn validate_file(file: &Path) -> Result<()> {
    let schema = load_sdl(file)?;
    validate(&schema).map_err(|e| anyhow!("SDL validation error: {e}"))?;
    println!("OK: {} ({} tables)", file.display(), schema.tables.len());
    for (name, table) in &schema.tables {
        println!("  - {name}: {} fields", table.fields.len());
    }
    Ok(())
}

fn load_sdl(file: &Path) -> Result<TosSchema> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("reading {}", file.display()))?;
    parse_sdl(&text).map_err(|e| anyhow!("SDL parse error in {}: {e}", file.display()))
}

fn sidecar_for(dataset: &str, sdl_path: &Path) -> PathBuf {
    let candidate = PathBuf::from(dataset);
    if candidate.exists() {
        let stem = candidate
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("schema");
        let parent = candidate.parent().unwrap_or_else(|| Path::new("."));
        return parent.join(format!("{stem}.tos"));
    }
    if let Some(parent) = candidate.parent() {
        if !parent.as_os_str().is_empty() && parent.exists() {
            let stem = candidate
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("schema");
            return parent.join(format!("{stem}.tos"));
        }
    }
    let mut p = sdl_path.to_path_buf();
    p.set_extension("tos");
    p
}

fn parse_json_records(text: &str) -> Vec<serde_json::Value> {
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<serde_json::Value>>(trimmed).unwrap_or_default()
    } else {
        let mut out = Vec::new();
        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.is_object() {
                    out.push(v);
                }
            }
        }
        out
    }
}

fn generate_create_table(scheme: &Scheme, table: &tos_core::sdl::TosTable) -> String {
    use tos_core::sdl::tos_type_name;
    let mut cols: Vec<String> = Vec::new();
    for f in &table.fields {
        let mut col = format!("  {} {}", quote_ident(scheme, &f.name), tos_type_name(&f.ty));
        if !f.nullable {
            col.push_str(" NOT NULL");
        }
        if f.primary {
            col.push_str(" PRIMARY KEY");
        }
        if f.unique {
            col.push_str(" UNIQUE");
        }
        cols.push(col);
    }
    format!(
        "CREATE TABLE {} (\n{}\n)",
        quote_ident(scheme, &table.name),
        cols.join(",\n")
    )
}

fn quote_ident(scheme: &Scheme, name: &str) -> String {
    match scheme {
        Scheme::Mysql => format!("`{name}`"),
        _ => format!("\"{name}\""),
    }
}

async fn build_for_pull(uri: &crate::uri::Uri) -> Result<Arc<dyn TosAdapter>> {
    use tos_adapter_json::JsonAdapter;
    use tos_adapter_mongodb::MongodbAdapter;
    use tos_adapter_mysql::MysqlAdapter;
    use tos_adapter_postgres::PostgresAdapter;
    use tos_adapter_redis::RedisAdapter;
    use tos_adapter_sqlite::SqliteAdapter;
    use tos_adapter_txt::TxtAdapter;
    use tos_adapter_yaml::YamlAdapter;
    match uri.scheme {
        Scheme::Mock => {
            let schema = crate::cmd::schema_for_dataset(&uri.dataset);
            let records = (0..crate::uri::param_u64(uri, "records", 0))
                .map(|i| crate::cmd::synthetic_record(&uri.dataset, i))
                .collect::<Vec<_>>();
            Ok(Arc::new(MockAdapter::with_records(
                format!("pull:{}", uri.dataset),
                schema,
                records,
            )))
        }
        Scheme::Json => Ok(Arc::new(
            JsonAdapter::open(format!("pull:{}", uri.dataset), std::path::Path::new(&uri.dataset))
                .map_err(|e| anyhow!("json open: {e}"))?,
        )),
        Scheme::Jsonl => Ok(Arc::new(
            JsonAdapter::open(
                format!("pull:{}", uri.dataset),
                std::path::Path::new(&uri.dataset),
            )
            .map_err(|e| anyhow!("jsonl open: {e}"))?,
        )),
        Scheme::Txt => Ok(Arc::new(
            TxtAdapter::open(
                format!("pull:{}", uri.dataset),
                std::path::Path::new(&uri.dataset),
                tos_adapter_txt::Delimiter::Comma,
            )
            .map_err(|e| anyhow!("txt open: {e}"))?,
        )),
        Scheme::Yaml => Ok(Arc::new(
            YamlAdapter::open(format!("pull:{}", uri.dataset), std::path::Path::new(&uri.dataset))
                .map_err(|e| anyhow!("yaml open: {e}"))?,
        )),
        Scheme::Sqlite => {
            let adapter = SqliteAdapter::open(format!("pull:{}", uri.dataset), &uri.dataset)
                .map_err(|e| anyhow!("sqlite open: {e}"))?;
            Ok(Arc::new(adapter))
        }
        Scheme::Postgres => {
            let url = format!("postgres://{}", uri.dataset);
            let adapter = PostgresAdapter::connect(&url)
                .await
                .map_err(|e| anyhow!("pg connect: {e}"))?;
            Ok(Arc::new(adapter))
        }
        Scheme::Mysql => {
            let url = format!("mysql://{}", uri.dataset);
            let adapter = MysqlAdapter::connect(&url)
                .await
                .map_err(|e| anyhow!("mysql connect: {e}"))?;
            Ok(Arc::new(adapter))
        }
        Scheme::Mongodb => {
            let url = format!("mongodb://{}", uri.dataset);
            let adapter = MongodbAdapter::connect(&url)
                .await
                .map_err(|e| anyhow!("mongodb connect: {e}"))?;
            Ok(Arc::new(adapter))
        }
        Scheme::Redis => {
            let url = format!("redis://{}", uri.dataset);
            let adapter = RedisAdapter::connect(&url)
                .await
                .map_err(|e| anyhow!("redis connect: {e}"))?;
            Ok(Arc::new(adapter))
        }
    }
}
