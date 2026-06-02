# Adapters

ToS ships with one adapter per supported backend. Each adapter is a
single Rust crate that implements `tos_core::adapter::TosAdapter` and
ships its own tests.

## Supported backends

| Backend   | Crate                  | Schemes              | Read | Write | Watch | Auto-CREATE |
|-----------|------------------------|----------------------|:----:|:-----:|:-----:|:-----------:|
| Mock      | `tos-core`             | `mock://`            |  ✓   |   ✓   |   ✗   |     —       |
| JSON      | `tos-adapters/json`    | `json://` `jsonl://` |  ✓   |   ✓   |   ✓   |     —       |
| YAML      | `tos-adapters/yaml`    | `yaml://`            |  ✓   |   ✓   |   ✗   |     —       |
| TXT / CSV | `tos-adapters/txt`     | `txt://`             |  ✓   |   ✓   |   ✗   |     —       |
| SQLite    | `tos-adapters/sqlite`  | `sqlite://`          |  ✓   |   ✓   |   ✗   |     ✓       |
| PostgreSQL| `tos-adapters/postgres`| `postgres://`        |  ✓   |   ✓   |   ✗   |     ✗       |
| MySQL     | `tos-adapters/mysql`   | `mysql://`           |  ✓   |   ✓   |   ✗   |     ✗       |
| MongoDB   | `tos-adapters/mongodb` | `mongodb://`         |  ✓   |   ✓   |   ✗   |     —       |
| Redis     | `tos-adapters/redis`   | `redis://`           |  ✓   |   ✓   |   ✗   |     —       |

The CLI also accepts `tcp://` and `quic://` URIs for direct P2P
sessions (no adapter).

## URI schemes

The dispatcher is in `tos-cli/src/uri.rs` and supports these forms:

```
mock://<table>?records=N
json:///abs/or/rel/path.json
jsonl:///abs/or/rel/path.jsonl
yaml:///path/to/file.yaml?key=…
txt:///path/to/file.txt?delimiter=|&has_header=true
sqlite:///abs/path/to.db?table=NAME
postgres://USER:PASS@HOST[:PORT]/DB?table=NAME[&schema=SCHEMA]
mysql://USER:PASS@HOST[:PORT]/DB?table=NAME
mongodb://HOST[:PORT]/DB?table=COLLECTION
redis://HOST[:PORT]?prefix=KEY_PREFIX
```

## Type mappings

Each adapter has a `tos_type ↔ local` mapping. The adapter is
responsible for any lossy conversion: if a destination cannot
represent a source type, the CLI prints a warning at
`schema push` time and refuses to write.

### PostgreSQL / MySQL

| SDL        | PG             | MySQL          |
|------------|----------------|----------------|
| `int32`    | `int4`         | `INT`          |
| `int64`    | `int8`         | `BIGINT`       |
| `float32`  | `float4`       | `FLOAT`        |
| `float64`  | `float8`       | `DOUBLE`       |
| `bool`     | `bool`         | `TINYINT(1)`   |
| `text`     | `text`         | `TEXT`         |
| `bytes`    | `bytea`        | `BLOB`         |
| `timestamp`| `timestamptz`  | `DATETIME`     |
| `date`     | `date`         | `DATE`         |
| `uuid`     | `uuid`         | `CHAR(36)`     |
| `json`     | `jsonb`        | `JSON`         |

Both adapters introspect the column type via
`column.type_info().name()` so the round-trip is lossless for the
common numeric / text types.

### SQLite

| SDL        | SQLite       |
|------------|--------------|
| `int32`    | `INTEGER`    |
| `int64`    | `INTEGER`    |
| `float64`  | `REAL`       |
| `bool`     | `INTEGER`    |
| `text`     | `TEXT`       |
| `bytes`    | `BLOB`       |
| `timestamp`| `TEXT`       |
| `json`     | `TEXT`       |

`bool` is stored as `INTEGER` (0/1) since SQLite has no native bool.
The reader recovers `true`/`false` from the literal string when the
source JSON had a boolean.

### MongoDB

Documents in MongoDB are untyped. ToS writes one document per row
with the SDL field names as keys. The `table` URI parameter maps to
the collection name.

### Redis

Each row is a Redis hash. The key is composed of the configured
`prefix` and the value of the natural key (the first column in
`key = [...]`, or the first column otherwise).

```
HSET tos:users:1   id=1   name="ada"   email="ada@x.io"   age=36   active=true
```

`HGETALL` recovers the row at read time.

### JSON / JSONL

- `json://` — the file is a single JSON array of objects.
- `jsonl://` — one object per line (NDJSON). The reader uses
  `serde_json::Deserializer::from_str` with `.into_iter::<Value>()`
  for true streaming.
- The writer detects `.jsonl` / `.ndjson` extensions and emits one
  object per line; otherwise it emits a JSON array.

### YAML

A YAML sequence of mappings:

```yaml
- id: 1
  name: ada
  age: 36
```

### TXT / CSV

A header row plus N data rows, separated by the configured
`delimiter` (default `|`). `tos push --from txt:///data.txt?delimiter=,&has_header=true`.

## Auto-CREATE behavior

- **SQLite**: the writer auto-`CREATE TABLE`s if the table does not
  exist, using the SDL field types. This is the only backend that
  creates tables on push; everywhere else, run `tos schema push`
  first.
- **Postgres / MySQL**: the writer does not create tables. You must
  have pre-created them, or run `tos schema push --from src.tos
  --to pg_uri?table=NAME` to emit the DDL.
- **MongoDB / Redis**: no table to create.

## Tested in CI

The CI matrix installs MariaDB, PostgreSQL, and Redis on
`ubuntu-latest` and runs the full E2E test suite:

- `bash scripts/test-e2e.sh` (in the repo)
- or `cargo test --workspace`

## Where to go next

- [CLI reference](./06-cli.md) — invoking the adapters from the
  command line.
- [Adapter contract](./adapter-contract.md) — implementing a new
  backend.
