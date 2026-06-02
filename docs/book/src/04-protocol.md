# Wire protocol

ToS speaks a length-prefixed, bincode-serialized, optional-encrypted
P2P protocol over TCP or QUIC. Every frame is a single `Message` enum
variant. Every batch is BLAKE3-hashed and Ed25519-signed.

## Framing

```
┌──────────────────┬─────────────────────────────────────┐
│  4 bytes (BE)    │  N bytes (bincode-serialized)       │
│  payload length  │  (variant tag + struct payload)     │
└──────────────────┴─────────────────────────────────────┘
```

- The length prefix is a `u32` in **big-endian** so a half-read
  buffer is unambiguously recoverable.
- The payload is `bincode::serialize(&Message)` using the default
  config: little-endian, fixed-width, no varints.
- The variant tag is `u32` (the index of the variant in the
  `Message` enum, 0-based).

## The `Message` enum

```rust
pub enum Message {
    Hello(Hello),                       // 0
    HelloAck(HelloAck),                 // 1
    SchemaOffer(SchemaOffer),           // 2
    SchemaDiff(SchemaDiff),             // 3
    SchemaConfirm(SchemaConfirm),       // 4
    StreamStart(StreamStart),           // 5
    Batch(Batch),                       // 6
    Ack(Ack),                           // 7
    StreamEnd(StreamEnd),               // 8
    Done(Done),                         // 9
}
```

There are nine messages, in the order of the protocol dance below.

## Handshake

1. **Hello** (C→S): the client announces its `node_id` (32 B),
   `public_key` (32 B Ed25519), `version: u8 = 1`, `encrypt: bool`,
   and `caps: Vec<String>` (e.g. `["postgres", "redis", "json"]`).
2. **HelloAck** (S→C): the server replies with its own identity and
   the intersection of capabilities.

If `encrypt = true` on both sides, an `x25519_pub` ephemeral key
exchanges and the rest of the session uses ChaCha20-Poly1305.

```text
C → S : Hello       { v=1, node_id=…, public_key=…, encrypt=false,
                      caps=["postgres","redis","json"] }
S → C : HelloAck    { v=1, node_id=…, public_key=…,
                      caps=["json","sqlite"] }
```

The `Handshake` struct in `tos-proto/src/handshake.rs` performs both
halves of the exchange with version-match enforcement.

## Schema negotiation

3. **SchemaOffer** (C→S): the client sends the SDL it intends to
   push, as raw bytes (`Vec<u8>`), plus a BLAKE3 hash of those bytes
   and an Ed25519 signature.
4. **SchemaDiff** (S→C): the server replies with `accepted: bool`
   and an optional human-readable `reason`.
5. **SchemaConfirm** (S→C): a zero-byte confirm so the client knows
   the server has finished validating.

## Streams and batches

6. **StreamStart** (C→S): a fresh `session_id` (32 B), the
   `table: String`, `mode: u8` (0 = full, 1 = incremental), and
   `batch_size: u32`.
7. **Batch** (S→C, repeated): each batch carries a `batch_id: u32`,
   `records: Vec<u8>` (the NDJSON of all rows in the batch),
   `batch_hash: [u8; 32]` (BLAKE3 of `records`), `signature: Vec<u8>`
   (Ed25519 over `batch_hash`), and `count: u32` (number of records).
8. **Ack** (C→S): the client acknowledges `batch_id`. If the
   receiver fails to verify the hash or signature, it closes the
   socket and the runner logs a `tracing::error!`.
9. **StreamEnd** (S→C): the server reports the final `total_records`
   and `duration_ms`.
10. **Done** (C→S): the client confirms graceful shutdown. The
    server then closes the socket.

The receiver never has to load the whole batch: the hash is verified
on the wire and records are streamed out as they are decoded.

## Watch & topology

- The `Watch` trait in `tos-proto/src/watch.rs` lets an adapter
  emit `ChangeStream` events. v1.0 ships mtime polling for the
  JSON adapter.
- A `Topology` is a TOML file describing the node identity and a
  list of pipelines (each with a `from`, one or more `to`, a
  `batch_size`, and a watch interval). The daemon spawns one task
  per pipeline and runs them concurrently.

## Live capture

The `wire_capture` example in `tos-proto/examples/wire_capture.rs`
runs the entire dance on `127.0.0.1:38767` and hexdumps every
frame. Run it with:

```bash
cargo run --example wire_capture -p tos-proto
```

The output is **14 frames · 1675 bytes · ~100 ms**:

| # | Dir | Bytes | Message |
|---|-----|-------|---------|
|  1 | C→S | 123  | `Hello` |
|  2 | S→C | 108  | `HelloAck` |
|  3 | C→S | 278  | `SchemaOffer` (SDL inline) |
|  4 | S→C |  10  | `SchemaDiff` (accepted) |
|  5 | S→C |   8  | `SchemaConfirm` |
|  6 | C→S |  58  | `StreamStart` |
|  7 | S→C | 314  | `Batch #0` (3 records, hash, sig) |
|  8 | C→S |  12  | `Ack` (batch 0) |
|  9 | S→C | 314  | `Batch #1` |
| 10 | C→S |  12  | `Ack` (batch 1) |
| 11 | S→C | 314  | `Batch #2` |
| 12 | C→S |  12  | `Ack` (batch 2) |
| 13 | S→C |  56  | `StreamEnd` (9 records, 12 ms) |
| 14 | C→S |  56  | `Done` |

The first byte of every payload is the variant tag. Frame 1 (Hello)
is:

```
00 00 00 77   00 00 00 00   01   <32B node_id> <32B pk> 00 03…<caps>
└─ len = 119  └─ Hello       └─ v=1
```

Each `Batch` ends with the Ed25519 signature of the BLAKE3 hash,
making the protocol safe to use over an untrusted network: an
attacker that modifies a byte in flight cannot produce a valid
signature.

## Where to go next

- [Adapters](./05-adapters.md) — what each backend does with the
  protocol.
- [Security](./07-security.md) — what is and is not protected.
