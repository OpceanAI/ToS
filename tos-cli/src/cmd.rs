use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use tos_adapter_json::JsonAdapter;
use tos_adapter_mongodb::MongodbAdapter;
use tos_adapter_mysql::MysqlAdapter;
use tos_adapter_postgres::PostgresAdapter;
use tos_adapter_redis::RedisAdapter;
use tos_adapter_sqlite::SqliteAdapter;
use tos_adapter_txt::{Delimiter, TxtAdapter};
use tos_adapter_yaml::YamlAdapter;
use tos_core::adapter::{TosAdapter, TosValue};
use tos_core::sdl::TosSchema;
use tos_core::MockAdapter;
use tos_crypto::Identity;
use tos_proto::runner::{RunStats, SessionRunner};
use tos_proto::transport::TcpTransport;

use crate::uri::{param_u64, parse, Scheme, Uri};

pub fn build_mock_adapter(uri: &Uri, role: &str) -> Result<Arc<MockAdapter>> {
    let schema = schema_for_dataset(&uri.dataset);
    if uri.scheme != Scheme::Mock {
        return Err(anyhow!(
            "internal: build_mock_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let n = param_u64(uri, "records", 0);
    let mut records = Vec::with_capacity(n as usize);
    for i in 0..n {
        records.push(synthetic_record(&uri.dataset, i));
    }
    Ok(Arc::new(MockAdapter::with_records(
        format!("{role}:{}", uri.dataset),
        schema,
        records,
    )))
}

pub fn build_json_adapter(uri: &Uri, role: &str) -> Result<Arc<JsonAdapter>> {
    if uri.scheme != Scheme::Json {
        return Err(anyhow!(
            "internal: build_json_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let path = PathBuf::from(&uri.dataset);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {}", parent.display()))?;
        }
    }
    let adapter = if path.exists() {
        JsonAdapter::open(format!("{role}:{}", uri.dataset), &path)?
    } else {
        JsonAdapter::new(format!("{role}:{}", uri.dataset), &path)
    };
    Ok(Arc::new(adapter))
}

pub async fn build_postgres_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<PostgresAdapter>> {
    if uri.scheme != Scheme::Postgres {
        return Err(anyhow!(
            "internal: build_postgres_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let url = format!("postgres://{}", uri.dataset);
    let adapter = PostgresAdapter::connect(&url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let _ = role;
    Ok(Arc::new(adapter))
}

pub async fn build_redis_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<RedisAdapter>> {
    if uri.scheme != Scheme::Redis {
        return Err(anyhow!(
            "internal: build_redis_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let url = format!("redis://{}", uri.dataset);
    let adapter = RedisAdapter::connect(&url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let _ = role;
    Ok(Arc::new(adapter))
}

pub async fn build_mysql_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<MysqlAdapter>> {
    if uri.scheme != Scheme::Mysql {
        return Err(anyhow!(
            "internal: build_mysql_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let url = format!("mysql://{}", uri.dataset);
    let adapter = MysqlAdapter::connect(&url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let _ = role;
    Ok(Arc::new(adapter))
}

pub async fn build_sqlite_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<SqliteAdapter>> {
    if uri.scheme != Scheme::Sqlite {
        return Err(anyhow!(
            "internal: build_sqlite_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let path = if uri.dataset == ":memory:" || uri.dataset.is_empty() {
        ":memory:".to_string()
    } else {
        uri.dataset.clone()
    };
    let adapter = SqliteAdapter::open(format!("{role}:{path}"), &path)
        .with_context(|| format!("opening sqlite at {path}"))?;
    Ok(Arc::new(adapter))
}

pub async fn build_mongodb_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<MongodbAdapter>> {
    if uri.scheme != Scheme::Mongodb {
        return Err(anyhow!(
            "internal: build_mongodb_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let url = format!("mongodb://{}", uri.dataset);
    let adapter = MongodbAdapter::connect(&url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let _ = role;
    Ok(Arc::new(adapter))
}

pub fn build_yaml_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<YamlAdapter>> {
    if uri.scheme != Scheme::Yaml {
        return Err(anyhow!(
            "internal: build_yaml_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let path = uri.dataset.clone();
    let adapter = if std::path::Path::new(&path).exists() {
        YamlAdapter::open(format!("{role}:{path}"), &path)
            .with_context(|| format!("opening yaml {path}"))?
    } else {
        YamlAdapter::new(format!("{role}:{path}"), &path)
    };
    Ok(Arc::new(adapter))
}

pub fn build_txt_adapter(
    uri: &Uri,
    role: &str,
) -> Result<Arc<TxtAdapter>> {
    if uri.scheme != Scheme::Txt {
        return Err(anyhow!(
            "internal: build_txt_adapter called with scheme `{}`",
            uri.scheme.as_str()
        ));
    }
    let delim = Delimiter::from_name(uri.params.get("delim").map(|s| s.as_str()).unwrap_or("csv"));
    let path = uri.dataset.clone();
    let adapter = if std::path::Path::new(&path).exists() {
        TxtAdapter::open(format!("{role}:{path}"), &path, delim)
            .with_context(|| format!("opening txt {path}"))?
    } else {
        TxtAdapter::new(format!("{role}:{path}"), &path, delim)
    };
    Ok(Arc::new(adapter))
}

pub async fn build_adapter(uri: &Uri, role: &str) -> Result<Arc<dyn TosAdapter>> {
    match &uri.scheme {
        Scheme::Mock => Ok(build_mock_adapter(uri, role)? as Arc<dyn TosAdapter>),
        Scheme::Json => Ok(build_json_adapter(uri, role)? as Arc<dyn TosAdapter>),
        Scheme::Postgres => {
            Ok(build_postgres_adapter(uri, role).await? as Arc<dyn TosAdapter>)
        }
        Scheme::Redis => {
            Ok(build_redis_adapter(uri, role).await? as Arc<dyn TosAdapter>)
        }
        Scheme::Mysql => {
            Ok(build_mysql_adapter(uri, role).await? as Arc<dyn TosAdapter>)
        }
        Scheme::Sqlite => {
            Ok(build_sqlite_adapter(uri, role).await? as Arc<dyn TosAdapter>)
        }
        Scheme::Mongodb => {
            Ok(build_mongodb_adapter(uri, role).await? as Arc<dyn TosAdapter>)
        }
        Scheme::Yaml => Ok(build_yaml_adapter(uri, role)? as Arc<dyn TosAdapter>),
        Scheme::Txt => Ok(build_txt_adapter(uri, role)? as Arc<dyn TosAdapter>),
    }
}

pub fn synthetic_record(dataset: &str, i: u64) -> TosValue {
    let body = json!({
        "id": i,
        "dataset": dataset,
        "payload": format!("{dataset}-row-{i}"),
        "score": (i % 100) as f64 + 0.5,
    });
    TosValue(body)
}

pub fn schema_for_dataset(name: &str) -> TosSchema {
    let mut s = TosSchema::new(name);
    let mut users = tos_core::sdl::TosTable {
        name: "rows".into(),
        fields: vec![],
        indexes: BTreeMap::new(),
        relations: BTreeMap::new(),
    };
    users.fields.push(tos_core::sdl::TosField {
        name: "id".into(),
        ty: tos_core::types::TosType::Primitive(tos_core::types::PrimitiveType::Int64),
        nullable: false,
        primary: true,
        unique: false,
        default: None,
        index: Some(tos_core::sdl::FieldIndex { order: 0 }),
        comment: None,
    });
    users.fields.push(tos_core::sdl::TosField {
        name: "dataset".into(),
        ty: tos_core::types::TosType::Primitive(tos_core::types::PrimitiveType::Text { max: None }),
        nullable: false,
        primary: false,
        unique: false,
        default: None,
        index: None,
        comment: None,
    });
    users.fields.push(tos_core::sdl::TosField {
        name: "payload".into(),
        ty: tos_core::types::TosType::Primitive(tos_core::types::PrimitiveType::Text { max: None }),
        nullable: false,
        primary: false,
        unique: false,
        default: None,
        index: None,
        comment: None,
    });
    users.fields.push(tos_core::sdl::TosField {
        name: "score".into(),
        ty: tos_core::types::TosType::Primitive(tos_core::types::PrimitiveType::Float64),
        nullable: true,
        primary: false,
        unique: false,
        default: None,
        index: None,
        comment: None,
    });
    s.add_table(users);
    let _ = name;
    s
}

pub async fn push_one(
    src: Arc<dyn TosAdapter>,
    dst: Arc<dyn TosAdapter>,
    table: &str,
    batch_size: u32,
) -> Result<RunStats> {
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .context("binding local listener")?;
    let addr: SocketAddr = listener
        .local_addr()
        .ok_or_else(|| anyhow!("listener has no local addr"))?;

    let id_server = Arc::new(Identity::generate());
    let id_client = Arc::new(Identity::generate());

    let server_runner = SessionRunner::new(id_server.clone(), batch_size);
    let dst_for_server = dst.clone();
    let server = tokio::spawn(async move {
        server_runner.run_server(listener, dst_for_server).await
    });

    let client_runner = SessionRunner::new(id_client.clone(), batch_size);
    let src_for_client = src.clone();
    let dst_for_client = dst.clone();
    let table_for_client = table.to_string();
    let client = tokio::spawn(async move {
        client_runner
            .run_client(addr, src_for_client, dst_for_client, &table_for_client)
            .await
    });

    let (server_res, client_res) = tokio::join!(server, client);
    let _server_stats = server_res
        .context("server task panicked")?
        .context("server run failed")?;
    let client_stats = client_res
        .context("client task panicked")?
        .context("client run failed")?;

    Ok(client_stats)
}

pub async fn push(from_uri: &str, to_uri: &str, table: Option<&str>) -> Result<RunStats> {
    let from = parse(from_uri).context("parsing --from")?;
    let to = parse(to_uri).context("parsing --to")?;
    let src = build_adapter(&from, "source").await?;
    let dst = build_adapter(&to, "dest").await?;
    let table_name = table
        .map(|s| s.to_string())
        .or_else(|| from.params.get("table").cloned())
        .unwrap_or_else(|| "rows".to_string());
    let batch_size = param_u64(&from, "batch", 100) as u32;
    push_one(src, dst, &table_name, batch_size).await
}

pub async fn sync(
    from_uri: &str,
    to_uris: &[String],
    table: Option<&str>,
    watch: bool,
    interval_secs: u64,
) -> Result<Vec<RunStats>> {
    if to_uris.is_empty() {
        return Err(anyhow!("sync requires at least one --to destination"));
    }
    let from = parse(from_uri).context("parsing --from")?;
    let src = build_adapter(&from, "source").await?;
    let mut dsts = Vec::new();
    for to_uri in to_uris {
        let to = parse(to_uri).context(format!("parsing --to {to_uri}"))?;
        let dst = build_adapter(&to, "dest").await?;
        dsts.push((to_uri.clone(), dst));
    }
    let table_name = table
        .map(|s| s.to_string())
        .or_else(|| from.params.get("table").cloned())
        .unwrap_or_else(|| "rows".to_string());
    let batch_size = param_u64(&from, "batch", 100) as u32;

    let mut all_stats = Vec::new();
    if watch {
        loop {
            for (uri, dst) in &dsts {
                let s = push_one(src.clone(), dst.clone(), &table_name, batch_size).await
                    .with_context(|| format!("sync iteration to {uri}"))?;
                all_stats.push(s);
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    } else {
        for (uri, dst) in &dsts {
            let s = push_one(src.clone(), dst.clone(), &table_name, batch_size).await
                .with_context(|| format!("sync to {uri}"))?;
            all_stats.push(s);
        }
    }
    Ok(all_stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn uri(scheme: Scheme, dataset: &str, params: Vec<(&str, &str)>) -> Uri {
        Uri {
            scheme,
            dataset: dataset.to_string(),
            params: params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<_, _>>(),
        }
    }

    #[test]
    fn build_mock_returns_correct_count() {
        let u = uri(Scheme::Mock, "demo", vec![("records", "5")]);
        let a = build_mock_adapter(&u, "src").unwrap();
        assert_eq!(a.len(), 5);
    }

    #[test]
    fn build_mock_with_zero_records() {
        let u = uri(Scheme::Mock, "demo", vec![]);
        let a = build_mock_adapter(&u, "src").unwrap();
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn build_mock_wrong_scheme_panics_safe() {
        let u = uri(Scheme::Json, "x", vec![]);
        let res = build_mock_adapter(&u, "src");
        assert!(res.is_err());
    }

    #[test]
    fn build_json_new_file() {
        let p = std::env::temp_dir().join(format!(
            "tos-cli-build-json-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        let u = uri(Scheme::Json, &p.to_string_lossy(), vec![]);
        let a = build_json_adapter(&u, "src").unwrap();
        assert!(a.is_empty());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn build_json_loads_existing() {
        let p = std::env::temp_dir().join(format!(
            "tos-cli-build-json-existing-{}.json",
            std::process::id()
        ));
        std::fs::write(&p, r#"[{"id":1},{"id":2},{"id":3}]"#).unwrap();
        let u = uri(Scheme::Json, &p.to_string_lossy(), vec![]);
        let a = build_json_adapter(&u, "src").unwrap();
        assert_eq!(a.len(), 3);
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn build_adapter_dispatches_mock() {
        let u = uri(Scheme::Mock, "x", vec![("records", "3")]);
        let a = build_adapter(&u, "src").await.unwrap();
        assert_eq!(a.name(), "src:x");
    }

    #[tokio::test]
    async fn build_adapter_dispatches_json() {
        let p = std::env::temp_dir().join(format!(
            "tos-cli-disp-json-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        let u = uri(Scheme::Json, &p.to_string_lossy(), vec![]);
        let a = build_adapter(&u, "src").await.unwrap();
        assert_eq!(a.name(), format!("src:{}", p.to_string_lossy()));
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn build_adapter_dispatches_mysql_uri() {
        let u = uri(Scheme::Mysql, "localhost/db", vec![]);
        let res = build_adapter(&u, "src").await;
        assert!(res.is_err());
        let err = res.err().unwrap();
        let msg = format!("{err:?}");
        assert!(msg.contains("mysql"), "error should mention scheme: {msg}");
    }

    #[tokio::test]
    async fn sync_to_empty_list_errors() {
        let res = sync("mock://a?records=1", &[], None, false, 1).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn sync_to_two_mock_destinations() {
        let res = sync(
            "mock://demo?records=5",
            &["mock://out1".to_string(), "mock://out2".to_string()],
            None,
            false,
            1,
        )
        .await;
        let stats = res.expect("sync should succeed");
        assert_eq!(stats.len(), 2);
        for s in &stats {
            assert_eq!(s.total_records, 5);
        }
    }
}
