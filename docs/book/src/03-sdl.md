# Schema (SDL)

The ToS Schema Description Language is the typed lingua franca that
travels alongside every batch. It is small, declarative, and serializes
to a stable text format that round-trips losslessly.

## Types

SDL is built on a closed set of primitive and compound types.

### Primitives

| SDL token        | Rust type                  | Notes |
|------------------|----------------------------|-------|
| `int32`          | `i32`                      | 4-byte signed |
| `int64`          | `i64`                      | 8-byte signed |
| `float32`        | `f32`                      | IEEE 754 |
| `float64`        | `f64`                      | IEEE 754 |
| `decimal(p,s)`   | `Decimal` (via string)     | arbitrary precision |
| `bool`           | `bool`                     | |
| `text`           | `String`                   | optional `max = N` |
| `bytes`          | `Vec<u8>`                  | optional `max = N` |
| `uuid`           | `Uuid`                     | 128-bit |
| `timestamp`      | `DateTime<Utc>`            | optional `with_tz = true` |
| `date`           | `NaiveDate`                | |
| `time`           | `NaiveTime`                | |
| `json`           | `serde_json::Value`        | untyped blob |
| `null`           | `()`                       | a null literal field |

### Compounds

```text
optional<T>      Nullable T
array<T>         List<T> (homogeneous)
map<K, V>        Record with K and V as types
enum("a","b",…)  One of a fixed set of strings
union(A, B, C)   Tag-discriminated sum
```

The first version of ToS ships with `optional` and `array` end-to-end;
`map`, `enum` and `union` are part of the SDL grammar and round-trip
through the parser / serializer, but only `optional` is fully wired
into every adapter.

## Writing a schema

A schema is a file. The CLI convention is `.tos`, the test fixtures
use it, and `tos schema push` emits it.

```toml
# users.tos
# ToS schema: users
# version: 0.1.0

[schema.users]
key = ["id"]
id     = { type = "int64", primary = true }
name   = { type = "text",  required = true, max = 200 }
email  = { type = "text" }
age    = { type = "int64" }
active = { type = "bool" }
```

Field-level keys:

- `type` (required) — one of the SDL tokens above
- `primary` (bool) — column is part of the primary key
- `required` / `nullable` (bool, mutually exclusive) — nullability
- `unique` (bool) — column is unique
- `max` (int) — text / bytes max length
- `default` (string) — default literal
- `comment` (string) — free-form annotation

Table-level keys:

- `key` (array of strings) — natural key, written as `key = ["id"]`
  on its own line

## Commands

`tos schema` has five sub-commands.

### `schema pull <URI>`

Connect to the source, read its `TosSchema`, and print it as SDL.

```bash
tos schema pull postgres://app:secret@db/app?table=users
# writes the SDL of the `users` table to stdout
```

### `schema push --from <URI> --to <URI>`

Read the source schema and propagate it to the destination. For SQL
backends this emits a `CREATE TABLE` (or `ALTER TABLE`) and prints
the DDL. For JSON, it writes a sidecar file `<basename>.tos` next to
the data file. For MongoDB and Redis, the schema is recorded in
metadata and applied on the next write.

```bash
tos schema push --from pg_uri?table=v2_users --to mysql_uri?table=users --dry-run
```

### `schema infer <URI>`

Read a sample of records from the source and derive a schema from the
data. For JSON / YAML / TXT the inference is straightforward. For SQL
backends the existing `INFORMATION_SCHEMA` is preferred; `infer` is
useful for unstructured files.

```bash
tos schema infer json:///var/log/events.json
```

### `schema diff <URI_A> <URI_B>`

Print a structural diff between two schemas. The output groups
fields by `added in right`, `removed from right`, `type changed`,
`nullability changed`, `primary changed`.

```bash
tos schema diff users_v1.tos users_v2.tos
```

### `schema validate <FILE>`

Parse a `.tos` file and check it for unknown types, missing keys,
duplicate fields, mismatched nullability, etc.

```bash
tos schema validate users_v2.tos
```

## Where to go next

- [Wire protocol](./04-protocol.md) — the schema travels with every
  batch, signed.
- [Adapters](./05-adapters.md) — per-backend type mappings.
