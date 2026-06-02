# Workspace layout

The repository is a single Cargo workspace rooted at the top of the
repo. Every member is a small, focused crate.

```
tos/
├── Cargo.toml                 # workspace manifest
├── Cargo.lock
├── rust-toolchain.toml        # channel = "stable"
├── rustfmt.toml
├── clippy.toml
├── deny.toml                  # cargo-deny policy
├── README.md
├── PROJECT.md                 # full spec
├── LICENSE                    # Apache-2.0
│
├── tos-core/                  # L1
│   └── src/
│       ├── lib.rs
│       ├── adapter/           # TosAdapter trait + MockAdapter + TosValue
│       ├── error.rs
│       ├── resolve.rs
│       ├── sdl/               # parser, schema, serializer, infer, validate
│       └── types/             # primitive, compound
│
├── tos-wire/                  # L2
│   └── src/lib.rs
│
├── tos-crypto/                # L0
│   └── src/identity.rs
│
├── tos-proto/                 # L3
│   ├── Cargo.toml
│   ├── examples/
│   │   └── wire_capture.rs    # live hexdump demo
│   └── src/
│       ├── error.rs
│       ├── messages.rs        # 9 protocol messages
│       ├── handshake.rs
│       ├── transport.rs       # TcpTransport, frame helpers
│       ├── quic.rs            # QuicTransport
│       ├── stream.rs          # BatchStream
│       ├── session.rs         # server-side state machine
│       ├── runner.rs          # SessionRunner
│       ├── watch.rs
│       └── topology.rs
│
├── tos-adapters/              # L4
│   ├── postgres/   mysql/   sqlite/   mongodb/   redis/
│   ├── json/       yaml/    txt/
│   └── (jsonl is a scheme on top of the json adapter)
│
├── tos-cli/                   # L5
│   ├── Cargo.toml             # [lib] tos_cli, [[bin]] tos
│   ├── src/
│   │   ├── main.rs            # clap derive, 7 subcommands
│   │   ├── lib.rs
│   │   ├── uri.rs             # 10-scheme dispatch
│   │   ├── cmd.rs             # push_one, sync, builders
│   │   ├── schema.rs          # 5 sub-commands
│   │   ├── daemon.rs          # TOML topology runner
│   │   └── topology.rs        # DaemonConfig + PipelineConfig
│   └── tests/                 # 14+9 E2E cli tests
│
├── tests/                     # cross-crate integration
│   ├── integration/
│   ├── unit/
│   └── fixtures/
│
├── docs/
│   └── book/                  # this mdBook
│       ├── book.toml
│       └── src/
│           ├── SUMMARY.md
│           ├── introduction.md
│           ├── quickstart.md
│           ├── design-goals.md
│           ├── architecture.md
│           ├── workspace.md
│           ├── adapter-contract.md
│           ├── 03-sdl.md
│           ├── 04-protocol.md
│           ├── 05-adapters.md
│           ├── 06-cli.md
│           ├── 07-security.md
│           ├── 08-ops.md
│           └── reference.md
│
├── scripts/
│   ├── cross.sh               # musl cross-compile helper
│   ├── make-deb.sh            # .deb builder
│   ├── audit.sh               # cargo audit + deny
│   └── tos.1                  # manpage source
│
└── .github/
    └── workflows/
        ├── ci.yml             # fmt, clippy, test, musl, coverage
        ├── release.yml        # 8 targets + .deb
        ├── audit.yml          # weekly advisory check
        └── book.yml           # mdBook → GitHub Pages
```

## Adding a new crate

To add a new layer (for example, a new `tos-adapter-snowflake`):

1. Create the directory and `Cargo.toml` with `version.workspace = true`
   and the same edition / license / authors / msrv block.
2. Add the crate to the root `Cargo.toml` `[workspace] members = […]`.
3. Implement `tos_core::adapter::TosAdapter`.
4. Register the URI scheme in `tos-cli/src/uri.rs` and add a
   `build_…_adapter` function in `tos-cli/src/cmd.rs`.
5. Add at least one integration test under
   `tos-adapter-snowflake/tests/`.

The change is local: nothing else in the workspace needs to be touched.

## Crate count and sizes

| Crate | LoC (approx) | Public surface |
|-------|--------------|----------------|
| `tos-core` | ~2000 | `TosValue`, `TosSchema`, SDL, `TosAdapter` |
| `tos-wire` | ~150 | `RecordStream`, `BatchStream` |
| `tos-crypto` | ~300 | `Identity`, hash & cipher helpers |
| `tos-proto` | ~1500 | `Transport`, `Message`, `SessionRunner` |
| `tos-adapters/*` | 150–500 each | adapter + tests |
| `tos-cli` | ~1200 | 7 subcommands |

## Where to go next

- [Adapter contract](./adapter-contract.md)
- [Architecture overview](./architecture.md)
