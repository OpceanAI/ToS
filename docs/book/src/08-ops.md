# Operations

This chapter is for people running ToS in production: building,
cross-compiling, packaging, integrating with CI, and writing
documentation.

## Building

From a fresh clone:

```bash
git clone https://github.com/OpceanAI/ToS
cd ToS
cargo build --workspace --release
target/release/tos --version
```

Typical release binary size on `x86_64-unknown-linux-gnu`:

```
$ ls -lh target/release/tos
-rwxr-xr-x 1 user user  6.4M  target/release/tos
```

The release profile uses `lto = "thin"`, `codegen-units = 1`,
`strip = true`, and `opt-level = 3` (see root `Cargo.toml`).

## Cross

ToS supports three static-musl targets out of the box. Use the
`scripts/cross.sh` helper to do the toolchain dance in one command:

```bash
./scripts/cross.sh aarch64-unknown-linux-musl
./scripts/cross.sh x86_64-unknown-linux-musl
./scripts/cross.sh armv7-unknown-linux-musleabihf
```

The helper will:

1. `apt-get install` the cross toolchain (or `pkg install` on
   Termux).
2. `rustup target add <triple>`.
3. Configure `~/.cargo/config.toml` with the right `linker = "…"` for
   the target.
4. `cargo build --release --target <triple> --bin tos`.
5. Verify that the resulting binary is statically linked with
   `file(1)`.

The output is a single static binary of about 6 MB. No glibc
dependency, no interpreter, just a Linux ELF.

## Packaging

### Debian / Ubuntu

`scripts/make-deb.sh` builds a `.deb` for the host architecture:

```bash
./scripts/make-deb.sh 1.0.0
# produces dist/tos_1.0.0_amd64.deb
```

The package installs:

- `/usr/local/bin/tos` (the binary)
- `/usr/local/share/man/man1/tos.1.gz` (the man page)
- `/etc/bash_completion.d/tos` (bash completion)
- `/usr/lib/systemd/system/tos.service` (the systemd unit)
- `/etc/tos/topology.toml` (default config, marked conffile)

### tarball

The release workflow also produces `.tar.xz` archives for every
target. The archive contains `tos` and `tos.1`; install with:

```bash
tar -xJf tos-x86_64-unknown-linux-musl.tar.xz
sudo install -m 0755 tos /usr/local/bin/
sudo install -m 0644 tos.1 /usr/local/share/man/man1/
sudo mandb
```

## CI

GitHub Actions runs four workflows.

### `ci.yml`

- `fmt` — `cargo fmt --all -- --check` on Ubuntu.
- `clippy` — `cargo clippy --workspace --all-targets -- -D warnings`
  on Ubuntu **and** macOS.
- `test` — `cargo test --workspace` on Ubuntu **and** macOS, with
  MariaDB, PostgreSQL, and Redis installed and started on Ubuntu.
- `musl` — build the release binary for `aarch64-unknown-linux-musl`,
  `x86_64-unknown-linux-musl`, and `armv7-unknown-linux-musleabihf`,
  verify static linkage, and upload the binary as an artifact.
- `coverage` — `cargo tarpaulin --workspace` and upload
  `cobertura.xml`.

### `release.yml`

- Triggered on `v*.*.*` tags and on `workflow_dispatch`.
- Builds 8 targets (5 Linux + 2 macOS + 1 Windows), packages each
  as `.tar.xz` or `.zip`, builds the `.deb`, and creates a GitHub
  Release with the artifacts and a generated `release-notes.md`.

### `audit.yml`

- Runs on every push to `main` and weekly.
- Runs `cargo audit --deny warnings` and `cargo deny check
  advisories|licenses|bans|sources`.

### `book.yml`

- Builds the mdBook on every push to `main`.
- Deploys the result to GitHub Pages at `/tos/`.

## Book

The user guide is a single `mdbook` crate. Build locally:

```bash
cargo install mdbook --locked
mdbook build docs/book
mdbook serve docs/book   # live preview on :3000
```

The output is in `docs/book/book/`. GitHub Pages is wired up via
`.github/workflows/book.yml`.

## Audit

To run the security audit locally:

```bash
./scripts/audit.sh
# produces SECURITY.md
```

Required tools: `cargo-audit`, `cargo-deny`. The script installs
them if absent.

## Coverage

```bash
cargo install cargo-tarpaulin --locked
cargo tarpaulin --workspace --out Html --output-dir coverage
# open coverage/index.html
```

## Where to go next

- [Security](./07-security.md) — what is and is not protected.
- [Wire protocol](./04-protocol.md) — what travels on the wire.
