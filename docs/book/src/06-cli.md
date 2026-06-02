# CLI reference

The `tos` binary is a single command with seven subcommands. Every
subcommand has `--help`.

## Synopsis

```
tos <COMMAND>

Commands:
  push        One-shot source → destination
  sync        Fan-out to multiple destinations
  schema      Inspect, propagate, derive, compare, validate SDL
  topology    Show or validate a TOML topology file
  node        Run the long-lived daemon
  status      Summarize the loaded topology
  log         Tail the tracing log
  help        Print help
```

## `push`

Read every row from a source URI and write it to a destination URI.

```
tos push --from <URI> --to <URI> [--table <NAME>] [--batch-size N]
```

| Flag          | Required | Notes |
|---------------|:--------:|-------|
| `--from`      |    ✓     | source URI (any scheme) |
| `--to`        |    ✓     | destination URI (any scheme) |
| `--table`     |          | override the table name on either side (when the URI does not already carry one) |
| `--batch-size`|          | default 100; ignored by some adapters |
| `--quiet`     |          | suppress per-batch logs |

Example:

```bash
tos push \
  --from postgres://app:secret@db/app?table=orders \
  --to   sqlite:///var/cache/orders.db?table=orders
```

Exit code is `0` on success, `1` on protocol / adapter error,
`2` on usage error.

## `sync`

Like `push`, but writes to multiple destinations concurrently and
continues on per-destination errors. The output is a per-destination
summary.

```
tos sync --from <URI> [--to <URI>]... [--watch] [--interval 5]
```

| Flag        | Required | Notes |
|-------------|:--------:|-------|
| `--from`    |    ✓     | source URI |
| `--to`      |    ✓ ×N  | one or more destination URIs |
| `--table`   |          | table name (used by backends that need it) |
| `--watch`   |          | re-run every `--interval` seconds |
| `--interval`|          | default 5 |
| `--batch-size`|        | default 100 |

Example:

```bash
tos sync \
  --from sqlite:///var/db.sqlite?table=events \
  --to   redis://cache:6379?prefix=tos:events \
  --to   json:///var/log/events.json \
  --to   postgres://app:secret@db/app?table=events_copy
```

If the Redis write fails, the JSON and PostgreSQL writes still
proceed; the CLI prints a per-destination status block at the end.

## `schema`

Five sub-commands: `pull`, `push`, `infer`, `diff`, `validate`.

```
tos schema pull   <URI>...
tos schema push   --from <URI> --to <URI> [--dry-run]
tos schema infer  <URI>
tos schema diff   <A> <B>
tos schema validate <FILE>
```

See the [SDL chapter](./03-sdl.md#commands) for the full reference.

## `topology`

Show or validate a TOML topology file.

```
tos topology show    [--config <PATH>]
tos topology validate [--config <PATH>]
```

Default path: `/etc/tos/topology.toml` if the file exists, else
`./topology.toml`.

## `node`

Run the daemon.

```
tos node start  [--config <PATH>]
tos node stop
tos node status
tos node id
```

`start` reads the topology, launches one task per pipeline, and
blocks on `SIGINT` / `SIGTERM`. The systemd unit installed by the
`.deb` package runs it under user `tos` with `Restart=on-failure`.

## `status`

Print a one-line summary per pipeline from the loaded topology:

```
$ tos status
pg-to-json       postgres → json       batch=100   watch=false
redis-fanout     postgres → redis      batch=100   watch=true
```

## `log`

Tail the tracing log file (`/var/log/tos.log` if running under
systemd, else stderr). Use `--lines N` to limit the output.

## TOML topology file

```toml
log_level = "info"           # or "debug", "warn", "error"
sync_interval_secs = 5
node_id = "edge-1"           # overrides the on-disk identity

[[pipeline]]
name = "pg-to-json"
from = "postgres://app:secret@db/app?table=orders"
to   = ["json:///var/cache/orders.json"]
table = "orders"
batch_size = 500
watch = true
interval_secs = 5

[[pipeline]]
name = "redis-fanout"
from = "postgres://app:secret@db/app?table=events"
to   = [
  "redis://cache:6379?prefix=tos:events",
  "sqlite:///var/cache/events.db?table=events",
]
table = "events"
batch_size = 100
watch = true
interval_secs = 2
```

Field reference:

- `log_level` — `error` | `warn` | `info` | `debug` | `trace`
- `sync_interval_secs` — how often the daemon tick fires
- `node_id` — stable identifier (Ed25519 public key hex if you want
  P2P). The default is a fresh key generated at first run.
- `[[pipeline]]` (one or more) — one task per pipeline
  - `name` — required, used in logs
  - `from` — required, single URI
  - `to`   — required, array of one or more URIs
  - `table` — required when the URI does not carry `?table=`
  - `batch_size` — default 100
  - `watch` — default false; if true, the pipeline re-runs every
    `interval_secs`
  - `interval_secs` — default 5

## Exit codes

| Code | Meaning |
|:----:|---------|
| 0    | success |
| 1    | protocol / adapter / DB error |
| 2    | usage error (unknown flag, bad URI, …) |
| 130  | SIGINT (Ctrl-C) |

## Where to go next

- [Wire protocol](./04-protocol.md) — what happens between the CLI
  and a remote peer.
- [Adapters](./05-adapters.md) — every URI scheme.
