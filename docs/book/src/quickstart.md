# Quickstart

## Install

From crates.io (once published):

```bash
cargo install tos-cli
```

From source:

```bash
git clone https://github.com/OpceanAI/ToS
cd ToS
cargo install --path tos-cli
```

On Debian/Ubuntu a `.deb` is provided by the
[release pipeline](https://github.com/OpceanAI/ToS/releases):

```bash
sudo dpkg -i tos_1.0.0_amd64.deb
```

A static musl binary needs no installation:

```bash
curl -L https://github.com/OpceanAI/ToS/releases/download/v1.0.0/tos-x86_64-unknown-linux-musl.tar.xz \
  | tar -xJ
sudo mv tos /usr/local/bin/
```

## Your first push

Spin up a SQLite database with some data, then push it to JSON:

```bash
# Create a tiny SQLite db with 5 rows
sqlite3 /tmp/users.db <<'SQL'
CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT, age INTEGER, active INTEGER);
INSERT INTO users VALUES
  (1, 'ada',    'ada@x.io',    36, 1),
  (2, 'alan',   'alan@x.io',   41, 1),
  (3, 'grace',  'grace@x.io',  85, 0),
  (4, 'linus',  'linus@x.io',  55, 1),
  (5, 'dennis', 'dennis@x.io', 80, 0);
SQL

# Push to JSON
tos push \
  --from sqlite:///tmp/users.db?table=users \
  --to   json:///tmp/users.json

cat /tmp/users.json
# [{"id":1,"name":"ada",...}, ...]
```

## Fan-out to three destinations at once

```bash
tos sync \
  --from sqlite:///tmp/users.db?table=users \
  --to   json:///tmp/users.json \
  --to   redis://localhost:6379?prefix=tos:u \
  --to   postgres://app:secret@localhost/app?table=users_copy
```

The three writes run concurrently. Per-destination errors do not abort the
others; the CLI prints a per-destination summary at the end.

## Run as a daemon

Create `topology.toml`:

```toml
log_level = "info"
sync_interval_secs = 5
node_id = "edge-1"

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

Then:

```bash
tos node start --config topology.toml
# Ctrl-C to stop
```

## See the wire protocol in action

ToS ships with a live demo that hexdumps every frame between a fake
client and a fake server:

```bash
cargo run --example wire_capture -p tos-proto
```

The output is 14 framed messages, 1675 bytes total, sent in &lt;100 ms.

## Next steps

- [Schema (SDL)](./03-sdl.md) — learn the typed description language.
- [Wire protocol](./04-protocol.md) — how the bytes are framed.
- [Adapters](./05-adapters.md) — every supported backend.
- [CLI reference](./06-cli.md) — every subcommand, every flag.
