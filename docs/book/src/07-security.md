# Security

ToS is a data-movement tool, so the threat model is "an attacker that
can read or modify bytes on the wire between two nodes". This chapter
spells out what is and is not protected in v1.0.

## Threat model

In scope:

- A passive observer that can read every byte on the wire.
- An active attacker that can modify, drop, replay, or reorder
  bytes.
- A malicious peer that connects to the daemon pretending to be a
  legitimate node.

Out of scope:

- Compromise of either endpoint (the attacker is assumed not to
  have local code execution on the running process).
- Compromise of the source or destination database (ToS does not
  add to the security model of the underlying backend; it does not
  subtract from it either).
- Denial of service at the network layer.

## Authenticity

Every protocol message carries the `node_id` and `public_key` of the
sender. The `Batch` message additionally carries an Ed25519 signature
of the BLAKE3 hash of the records:

```text
signature = Ed25519.sign(sender_sk, blake3(records))
```

The receiver verifies:

1. `blake3(records) == batch.batch_hash` (integrity)
2. `ed25519.verify(batch.batch_signature, batch.batch_hash) == true`
   (authenticity)

If either check fails, the receiver closes the socket without
acking the batch and the runner logs a `tracing::error!`. The
sender's public key is bound to its `node_id` at handshake time, so
an attacker cannot substitute a different key mid-session.

## Encryption

Encryption is **opt-in**. Set `encrypt: true` in the `Hello`
message; if the server agrees, it replies with its X25519 ephemeral
public key, both sides derive a shared secret, and from that point
on the entire payload of every frame is wrapped in ChaCha20-Poly1305
with a per-frame nonce.

To enable encryption in the CLI today, use a `tcp://` or `quic://`
URI; the adapters (`postgres://`, `mysql://`, …) are direct
backend connections and the backend itself is responsible for
transport security (TLS, SSH tunnel, …).

A future release will surface `encrypt=true` from the topology file
and from the CLI flags for the P2P transport.

## Watch & topology

The `Topology` file is a TOML document. The CLI does not currently
sign or verify it; an attacker that can write to the file can
redirect pipelines. Mitigations:

- Run the daemon under a dedicated user (`tos` in the `.deb`) and
  make the file readable only by that user.
- Run the daemon on the same host as the source and destination so
  the file is not shared with anyone else.

## Audit pipeline

The repo includes an `audit` workflow (`.github/workflows/audit.yml`)
and a local script (`./scripts/audit.sh`) that runs:

- `cargo audit` (RustSec Advisory Database)
- `cargo deny` (licenses, bans, sources)

on every push to `main` and weekly. Reports are written to
`SECURITY.md`.

## Reporting a vulnerability

Email `security@opceanai.com` with a description and a
reproduction. GPG key on request. Please do **not** open public
issues for unpatched vulnerabilities; we will coordinate disclosure
on a 90-day timeline (or sooner if you prefer).

## Where to go next

- [Wire protocol](./04-protocol.md) — the exact byte-level
  protections.
- [Operations](./08-ops.md#audit) — running the audit locally.
