# ToS — Translation of Service

A peer-to-peer protocol for moving and synchronizing structured data between
any source and any destination, in real time, without a central broker.

> See [PROJECT.md](./PROJECT.md) for the full technical specification and
> development roadmap.

## Quickstart (v0.1.0)

Once v0.1.0 is released:

```bash
# Install from source
cargo install --path tos-cli

# Migrate a PostgreSQL table to a JSON file in one command
tos push --from postgres://user:pass@localhost/db \
         --to   json:///tmp/backup.json \
         --table users
```

## Status

This project is in active development. See the
[Roadmap section](./PROJECT.md#roadmap) in PROJECT.md for the detailed plan.

Current session (S1 of 6): workspace scaffolding + `tos-core` + `tos-crypto`.

## Workspace layout

```
tos-core/           Layer 1: SDL + universal type system
tos-wire/           Layer 2: binary wire format
tos-crypto/         Cryptography primitives (Ed25519, BLAKE3, ChaCha20)
tos-proto/          Layer 3: P2P protocol (handshake, streams, watch)
tos-adapters/       Layer 4: per-database adapters
  postgres/ mysql/ sqlite/ mongodb/ redis/
  json/ yaml/ txt/
tos-cli/            Layer 5: command-line interface
tests/              cross-crate tests + fixtures
docs/book/          mdBook (S6)
```

## Building & testing

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

> **Termux note:** `cargo clippy` (the rustup wrapper) panics in current
> Termux builds because of an upstream `rustls-platform-verifier` issue.
> Use the direct `cargo-clippy` binary instead, which is shipped with the
> Termux `rust` package and works without the rustup sync step.

## License

Apache-2.0. See [LICENSE](./LICENSE).
