# Reference

## Glossary

- **Adapter** — a Rust crate that implements `TosAdapter` for a
  specific backend (PostgreSQL, Redis, …).
- **Batch** — a single `Batch` message on the wire; one or more
  records, plus a BLAKE3 hash and an Ed25519 signature.
- **CLI** — the `tos` binary.
- **Daemon** — `tos node start`; a long-running process driven by
  a TOML topology.
- **Frame** — a single length-prefixed message on the wire.
- **Handshake** — the `Hello` / `HelloAck` exchange that opens
  every P2P session.
- **Pipeline** — one `[[pipeline]]` block in the topology file.
- **Record** — one row, represented as a `TosValue` (a thin
  wrapper over `serde_json::Value`).
- **Schema** — a `TosSchema` object: a map of table name to
  `TosTable` (columns + key + relations).
- **SDL** — the Schema Description Language; the textual form of
  a schema (see [SDL chapter](./03-sdl.md)).
- **Stream** — a `futures::Stream<Item = Result<TosValue, _>>`.
- **Topology** — the TOML file that drives the daemon.
- **TosValue** — the row-level value type, an enum newtype over
  `serde_json::Value`.
- **Watch** — the `Watch` trait, plus the per-pipeline `watch`
  flag in the topology.

## Changelog

### v1.0 (in progress)

- All S1–S5 work: SDL, wire, crypto, P2P (TCP + QUIC), 9
  adapters, 7 CLI subcommands.
- E2E test suite: 113/113 passing + 1 skip (MongoDB not present
  in CI Termux).
- `cargo test --workspace`: 348 passing.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- mdBook (this book).
- `wire_capture` example with live hexdump of every frame.
- CI matrix: fmt, clippy, test (Ubuntu + macOS), musl (3
  targets), coverage.
- Release workflow: 8 targets, `.deb` package, GitHub release.
- Audit workflow: weekly `cargo audit` + `cargo deny`.
- Bug fix: `tos-adapter-sqlite` `row_to_value` no longer
  mis-decodes INTEGER columns as bool.

### v0.1 (initial)

- Workspace scaffolding.
- `tos-core` SDL parser / serializer / validator.
- `tos-crypto` Ed25519 + BLAKE3 + ChaCha20-Poly1305 + X25519.
- `tos-wire` bincode helpers.
- `tos-proto` TCP handshake, message enum.
- Adapter stubs for all 9 backends.
- `tos-cli` skeleton.
