use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use tos_core::adapter::TosValue;
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
            "scheme `{}` not supported in v0.2 ({} side)",
            uri.scheme.as_str(),
            role
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

pub async fn push_mock(from_uri: &str, to_uri: &str, table: Option<&str>) -> Result<RunStats> {
    let from = parse(from_uri).context("parsing --from")?;
    let to = parse(to_uri).context("parsing --to")?;
    if from.scheme != Scheme::Mock || to.scheme != Scheme::Mock {
        return Err(anyhow!(
            "tos push v0.2 only supports mock:// URIs (got {} -> {})",
            from.scheme.as_str(),
            to.scheme.as_str()
        ));
    }
    let src = build_mock_adapter(&from, "source")?;
    let dst = build_mock_adapter(&to, "dest")?;
    let table_name = table.unwrap_or("rows").to_string();

    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .context("binding local listener")?;
    let addr: SocketAddr = listener
        .local_addr()
        .ok_or_else(|| anyhow!("listener has no local addr"))?;

    let id_server = Arc::new(Identity::generate());
    let id_client = Arc::new(Identity::generate());
    let batch_size = param_u64(&from, "batch", 100) as u32;

    let server_runner = SessionRunner::new(id_server.clone(), batch_size);
    let dst_for_server = dst.clone();
    let server = tokio::spawn(async move {
        server_runner.run_server(listener, dst_for_server).await
    });

    let client_runner = SessionRunner::new(id_client.clone(), batch_size);
    let src_for_client = src.clone();
    let dst_for_client = dst.clone();
    let table_name_for_client = table_name.clone();
    let client = tokio::spawn(async move {
        client_runner
            .run_client(addr, src_for_client, dst_for_client, &table_name_for_client)
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
