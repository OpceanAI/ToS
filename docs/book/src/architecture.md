# Architecture

ToS is built as a stack of small, replaceable layers. Each layer has one
responsibility, exposes a trait, and depends only on the layers below it.

```
┌──────────────────────────────────────────────────────────────────┐
│  L5  CLI / Daemon       tos-cli                                  │
│      push · sync · schema · topology · node · status · log       │
├──────────────────────────────────────────────────────────────────┤
│  L4  Adapters           tos-adapters/*                           │
│      postgres · mysql · sqlite · mongodb · redis                 │
│      json · jsonl · yaml · txt                                   │
├──────────────────────────────────────────────────────────────────┤
│  L3  P2P protocol       tos-proto                                │
│      handshake · streams · watch · topology · QUIC · TCP         │
├──────────────────────────────────────────────────────────────────┤
│  L2  Wire format        tos-wire                                 │
│      bincode helpers · RecordStream · TosValue                   │
├──────────────────────────────────────────────────────────────────┤
│  L1  Core types & SDL   tos-core                                 │
│      TosSchema · TosTable · TosField · TosType · parser/ser      │
├──────────────────────────────────────────────────────────────────┤
│  L0  Crypto             tos-crypto                               │
│      Ed25519 · BLAKE3 · ChaCha20-Poly1305 · X25519               │
└──────────────────────────────────────────────────────────────────┘
```

## Layer responsibilities

### L0 — Crypto (`tos-crypto`)

- `Identity` wrapper around `ed25519_dalek::SigningKey`
- BLAKE3 hashing for schema and batch integrity
- ChaCha20-Poly1305 for payload encryption (when `encrypt=true` is
  negotiated)
- X25519 for ephemeral session keys

No dependency on any other ToS layer.

### L1 — Core types & SDL (`tos-core`)

- The `TosValue` enum (a thin newtype over `serde_json::Value` with a
  few extension methods)
- The `TosType` enum — the universal type system
- The SDL parser, serializer, validator, inferer, and differ
- The `MockAdapter` for tests and the `TosAdapter` trait

### L2 — Wire format (`tos-wire`)

- `RecordStream` — a `futures::Stream<Item = Result<TosValue, …>>`
- Helpers for batch framing on top of bincode
- The `BATCH_HEADER_SIZE = 44` constant

### L3 — P2P protocol (`tos-proto`)

- The `Transport` trait (sync, async) — `TcpTransport`, `QuicTransport`
- The `Handshake` struct (Hello / HelloAck exchange, version match)
- `Session` (server-side state machine) and `SessionRunner` (drives a
  full sync over the wire)
- The `Message` enum with the nine protocol messages
- The `Watch` trait (mtime poll in v1.0)
- The `Topology` config struct

### L4 — Adapters (`tos-adapters/*`)

Each adapter implements `TosAdapter` for a single backend:

```rust
#[async_trait]
pub trait TosAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn read_schema(&self) -> Result<TosSchema, BoxedError>;
    async fn read_records(&self, table: &str) -> Result<RecordStream, BoxedError>;
    async fn write_records(&self, table: &str, records: RecordStream) -> Result<u64, BoxedError>;
    async fn watch(&self, table: &str) -> Result<ChangeStream, BoxedError>;
    async fn close(&self) -> Result<(), BoxedError>;
}
```

The trait is small, async, and backend-agnostic. Adapters are free to
add their own constructors, options, and helper methods.

### L5 — CLI / Daemon (`tos-cli`)

- Argument parsing via `clap` 4.x
- URI dispatch (10 schemes) to the right adapter
- The 7 subcommands: `push`, `sync`, `schema`, `topology`, `node`,
  `status`, `log`
- The TOML-driven daemon loop with one task per pipeline

## Dependency rule

A layer may depend on any lower-numbered layer. It must not depend on a
higher-numbered one. The compiler enforces this through the `Cargo.toml`
sectioning: each crate's `[dependencies]` lists only its own sub-set of
the workspace.

The five layers compile to 13 crates:

```
tos-core        tos-wire        tos-crypto
tos-proto
tos-adapters/{postgres,mysql,sqlite,mongodb,redis,json,yaml,txt}
tos-cli (= tos_cli lib + tos binary)
```

## Where to go next

- [Workspace layout](./workspace.md) — concrete file tree.
- [TosAdapter contract](./adapter-contract.md) — the trait that every
  adapter implements.
- [Wire protocol](./04-protocol.md) — what travels on the socket.
