# The `TosAdapter` contract

Every backend in ToS is a single Rust trait implementation. The trait
is small, async, and the only thing the protocol layer sees. If you
want to add a new backend, you implement this trait and register the
URI scheme in the CLI.

```rust
// tos-core/src/adapter/mod.rs
#[async_trait]
pub trait TosAdapter: Send + Sync {
    fn name(&self) -> &str;

    async fn read_schema(&self) -> Result<TosSchema, BoxedError>;

    async fn read_records(&self, table: &str) -> Result<RecordStream, BoxedError>;

    async fn write_records(
        &self,
        table: &str,
        records: RecordStream,
    ) -> Result<u64, BoxedError>;

    async fn watch(&self, table: &str) -> Result<ChangeStream, BoxedError>;

    async fn close(&self) -> Result<(), BoxedError>;
}
```

## Method-by-method

### `fn name(&self) -> &str`

A short identifier (`"postgres"`, `"redis"`, …) used in logs and
error messages. Must be stable across versions.

### `async fn read_schema(&self) -> Result<TosSchema, BoxedError>`

Return a `TosSchema` describing every table the adapter can see. The
schema is the entry point for `tos schema pull`.

The returned schema **must**:

- contain one `TosTable` per visible table;
- populate `TosField.ty` with the most specific `TosType` that
  round-trips through the backend;
- set `TosField.nullable` correctly;
- set `TosField.primary` for the primary-key columns;
- set `TosTable.key` to the list of column names forming the natural
  key (used by `tos schema push` to mark `key = [...]`).

### `async fn read_records(&self, table: &str) -> Result<RecordStream, …>`

Return a `Stream<Item = Result<TosValue, _>>` of every row in the
named table. The stream is consumed lazily; do not collect it.

A `TosValue` is a thin newtype around `serde_json::Value` with
helpers. For a row with columns `(id, name, age)` the value is:

```json
{"id": 1, "name": "ada", "age": 36}
```

### `async fn write_records(&self, table, records) -> Result<u64, …>`

Consume the entire stream and write every row. Return the number of
rows written. The adapter is free to:

- batch the inserts (the runner will pass the whole stream; the
  adapter is responsible for transaction boundaries);
- create the table if it does not exist (SQL backends do this;
  MongoDB / Redis do not need to);
- use upsert semantics if `TosTable.key` is populated.

### `async fn watch(&self, table) -> Result<ChangeStream, …>`

Return a `ChangeStream` of `(TosValue, Op)` events where `Op ∈
{Insert, Update, Delete}`. v1.0 ships mtime polling for the JSON
adapter; PostgreSQL, MySQL, etc. return an error and the daemon falls
back to re-polling `read_records`.

### `async fn close(&self) -> Result<(), …>`

Release any held resources. The CLI calls this on exit; the daemon
calls it when a pipeline is shut down.

## Cross-cutting concerns

### Error type

Every method returns `Result<_, BoxedError>`. The trait does not
constrain the concrete error; adapters typically define their own
`thiserror`-based enum and `Box::new` it. The CLI prints the
`Display` impl.

### Logging

Adapters should use `tracing` for anything beyond the happy path.
`info!` for a successful batch boundary, `warn!` for retries, and
`error!` for non-recoverable failures.

### Threading

The trait is `Send + Sync`. Adapters with internal mutability must
wrap their state in `Arc<Mutex<_>>` or `Arc<RwLock<_>>`. The SQLite
adapter uses `Arc<Mutex<Connection>>` and dispatches synchronous
calls through `tokio::task::spawn_blocking` to keep the runtime
non-blocking.

### Timeouts

The trait does not impose timeouts, but the daemon does: every
pipeline runs inside `tokio::time::timeout(Duration::from_secs(60), …)`
in the watch loop. If your adapter takes longer, surface a clear
`tracing::error!` so the operator can tune the timeout.

## Worked example: a 30-line in-memory adapter

```rust
use async_trait::async_trait;
use futures::stream;
use tos_core::adapter::*;
use tos_core::sdl::*;
use tos_core::types::PrimitiveType;

pub struct MemAdapter { name: String, rows: Vec<TosValue> }

#[async_trait]
impl TosAdapter for MemAdapter {
    fn name(&self) -> &str { &self.name }
    async fn read_schema(&self) -> Result<TosSchema, _> {
        let mut s = TosSchema::new(&self.name);
        s.add_table(TosTable {
            name: "data".into(),
            key: vec!["id".into()],
            fields: vec![TosField {
                name: "id".into(),
                ty: TosType::Primitive(PrimitiveType::Int64),
                nullable: false, primary: true, unique: true,
                default: None, index: Some(FieldIndex { order: 1 }),
                comment: None,
            }],
            indexes: Default::default(),
            relations: Default::default(),
        });
        Ok(s)
    }
    async fn read_records(&self, _t: &str) -> Result<RecordStream, _> {
        Ok(Box::pin(stream::iter(self.rows.iter().cloned().map(Ok))))
    }
    async fn write_records(&self, _t: &str, mut r: RecordStream)
        -> Result<u64, _>
    {
        let mut n = 0;
        while let Some(_) = r.try_next().await? { n += 1; }
        Ok(n)
    }
    async fn watch(&self, _t: &str) -> Result<ChangeStream, _> { unimplemented!() }
    async fn close(&self) -> Result<(), _> { Ok(()) }
}
```

That's the entire contract. The CLI and the daemon know nothing else
about the backend.

## Where to go next

- [Adapters](./05-adapters.md) — every backend that ships today.
- [Wire protocol](./04-protocol.md) — what travels on the socket once
  the adapter has produced rows.
