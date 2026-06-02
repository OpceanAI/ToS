# Design goals

The v1.0 design is anchored in five goals. Every implementation choice
is traceable to one of them.

## 1. Typed end-to-end

Bytes are cheap. Types are not.

ToS never passes a record without a schema. The schema is described in
[SDL](./03-sdl.md), serialized alongside the data, and verified on the
receiving end via a BLAKE3 hash. If the destination cannot represent a
field, the schema negotiation rejects the batch before any rows are
written.

Why: without types, a `bool` becomes `0` becomes `0` becomes `'f'` after
two hops, and the receiver cannot tell whether the original was a flag
or a number.

## 2. Heterogeneity without translation layers

ETL tools assume a canonical store. ToS does not. The source and the
destination are peers; either side may be a SQL database, a document
store, a key-value cache, or a flat file. The SDL type system is the
shared lingua franca; the adapter handles the local dialect.

Why: the real world is a federation. Adding "one more destination" must
not require adding "one more translator".

## 3. Streaming and bounded memory

Records are not held in memory. Adapters return a `RecordStream` (a
`futures::Stream`) and the writer pulls from it. Batches have a
configurable size (`batch_size`) and the wire format carries a count
plus a BLAKE3 hash, so a slow consumer can be back-pressured without
ever loading the whole table.

Why: a 100-million-row table should not OOM the daemon.

## 4. P2P, broker-free, single-binary

ToS is one Rust workspace, one CLI, one daemon. The P2P protocol speaks
TCP or QUIC, signs every batch with Ed25519, and negotiates optional
ChaCha20-Poly1305 payload encryption. There is no coordinator service.

Why: brokers are a single point of failure and a privileged attack
target. If the only node you trust is yourself, the protocol works.

## 5. Small surface, big ecosystem

Ten URI schemes today. Each adapter is a single crate with a single
trait implementation. Adding a new backend means writing a new crate
and registering it; nothing else changes.

Why: the cost of a new integration should be linear in the integration,
not exponential in the rest of the system.

## Non-goals

- **CDC / log-tailing.** The `watch` mode polls; it does not tail the
  WAL. A future version will integrate PostgreSQL logical decoding and
  MySQL binlog.
- **Schema evolution on the wire.** v1.0 supports `schema diff`, but
  renames and type widening require a manual migration.
- **Consensus / quorum.** P2P sync is last-writer-wins. Multi-master
  consensus is a research project, not a v1.0 feature.
- **A web UI.** The CLI and the daemon are the entire surface area.
