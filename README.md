# ToS — Translation of Service

A peer-to-peer protocol for moving and synchronizing structured data between
any source and any destination, in real time, without a central broker.

ToS moves rows, not bytes. Every payload is described by a schema
([ToS SDL](./docs/book/src/03-sdl.md)) and a typed value system, so a
PostgreSQL table can be replicated to Redis, MongoDB to SQLite, or a CSV
file to a JSON Lines stream, and back, with the same one-line command.

> See [PROJECT.md](./PROJECT.md) for the full technical specification and
> development roadmap, and the [book](./docs/book/) for the user guide.

## Quickstart

```bash
# Build & install
cargo install --path tos-cli

# Single push: PostgreSQL → JSON
tos push --from postgres://user:pass@localhost/db \
         --to   json:///tmp/backup.json \
         --table users

# Fan-out: PostgreSQL → JSON, SQLite, Redis (concurrent, continue-on-error)
tos sync --from postgres://user:pass@localhost/db?table=users \
         --to   json:///tmp/u.json \
         --to   sqlite:///tmp/u.db?table=users \
         --to   redis://localhost:6379?prefix=tos:u

# Live daemon with TOML topology
tos node start --config topology.toml
```

## Status: v1.0

| Layer | Crate(s) | Status |
|-------|----------|--------|
| 1. Core types + SDL | `tos-core` | ✓ stable |
| 2. Wire format | `tos-wire` | ✓ stable |
| 3. Crypto | `tos-crypto` | ✓ stable |
| 4. P2P protocol (TCP+QUIC) | `tos-proto` | ✓ stable |
| 5. Adapters | `tos-adapters/{postgres,mysql,sqlite,mongodb,redis,json,yaml,txt}` | ✓ stable |
| 6. CLI (`push`/`sync`/`schema`/`topology`/`node`/`status`/`log`) | `tos-cli` | ✓ stable |
| 7. CI matrix, musl cross, mdBook, packaging, audit | infra | ✓ done |

> **Live demo** of the wire protocol (frame-by-frame hexdump):
> `cargo run --example wire_capture -p tos-proto`

## Workspace layout

```
tos-core/            Layer 1: SDL + universal type system
tos-wire/            Layer 2: binary wire format
tos-crypto/          Ed25519, BLAKE3, ChaCha20-Poly1305, X25519
tos-proto/           Layer 3: P2P handshake, streams, watch, topology (TCP+QUIC)
tos-adapters/        Layer 4: per-backend adapters
  postgres/ mysql/ sqlite/ mongodb/ redis/
  json/ yaml/ txt/ (+jsonl scheme in json/)
tos-cli/             Layer 5: command-line interface
tests/               cross-crate unit/integration + fixtures
docs/book/           mdBook user guide (architecture, protocol, adapters, CLI)
.github/workflows/   ci.yml, release.yml, audit.yml, book.yml
scripts/             cross-compile, deb-packager, sha256
```

## Supported schemes

`mock://` (in-process), `postgres://`, `mysql://`, `sqlite://`,
`mongodb://`, `redis://`, `json://`, `jsonl://`, `yaml://`, `txt://`,
`tcp://` and `quic://` (P2P).

## Building & testing

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt   --all -- --check
```

> **Termux note:** `cargo clippy` (the rustup wrapper) panics in current
> Termux builds because of an upstream `rustls-platform-verifier` issue.
> Use the direct `cargo-clippy` binary instead, which is shipped with the
> Termux `rust` package and works without the rustup sync step.

## Cross-compile (musl static binaries)

```bash
./scripts/cross.sh aarch64-unknown-linux-musl
./scripts/cross.sh x86_64-unknown-linux-musl
./scripts/cross.sh armv7-unknown-linux-musleabihf
# produces target/<triple>/release/tos (statically linked, ~6 MB)
```

## Security

`./scripts/audit.sh` runs `cargo audit` + `cargo deny` and writes
`SECURITY.md`. The audit is wired into CI via `.github/workflows/audit.yml`.

## License

Apache-2.0. See [LICENSE](./LICENSE).
