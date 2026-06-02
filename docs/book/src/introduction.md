# Introduction

**ToS — Translation of Service** is a peer-to-peer protocol for moving
and synchronizing structured data between any source and any destination,
in real time, without a central broker.

ToS treats data as **typed rows**, not as bytes. Every payload is described
by a schema written in the [ToS Schema Description Language](./03-sdl.md)
(SDL), so a single command can move a PostgreSQL table to a JSON file, a
MongoDB collection to SQLite, or a CSV file to a JSONL stream — and back
again — without losing type information.

## Why ToS?

Most data tools assume either "all the data lives in one place" (databases,
ETL tools) or "the data is just bytes" (queues, file sync). ToS is built
for the middle ground: a federation of heterogeneous stores that need to
agree on shape and content but do not want a central pipeline.

Concretely, ToS gives you:

- **One command, any source, any destination** — ten URI schemes today
  (PostgreSQL, MySQL, SQLite, MongoDB, Redis, JSON, JSONL, YAML, TXT, mock).
- **Typed end-to-end** — schemas travel alongside the data, so `int64` stays
  `int64` even if the destination is Redis.
- **Streaming, batched, signed** — every batch carries a BLAKE3 hash and an
  Ed25519 signature, and is ack'd by the receiver.
- **P2P and daemon-friendly** — long-running topologies in TOML, no broker
  required, QUIC or TCP transport.
- **Small and auditable** — the workspace compiles in &lt;60 s, has 348 tests
  and zero clippy warnings.

## What you can build with it

- A CDC-style replica of a production database into a developer SQLite
  snapshot, refreshed every five seconds.
- A federated multi-region Redis with PostgreSQL as the source of truth.
- A migration path off MongoDB into PostgreSQL with automatic schema
  inference from the source.
- A nightly JSON dump of every table for backup, signed and hashed.
- A small P2P network of edge devices that sync a config table among
  themselves without a server.

## What ToS is not

- Not a database. Adapters read and write the backends but do not own them.
- Not a general-purpose message bus. Records are unordered within a batch.
- Not a CDC tracker. There is no log-tailing yet; watch is based on
  re-polling in v1.0.

## Where to go next

- [Quickstart](./quickstart.md) — install and run your first push.
- [Architecture](./architecture.md) — how the layers fit together.
- [Schema (SDL)](./03-sdl.md) — the heart of the protocol.
