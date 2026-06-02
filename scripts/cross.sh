#!/usr/bin/env bash
# Cross-compile the `tos` binary to a musl static target.
# Usage: ./scripts/cross.sh [target]
#   target ∈ {aarch64-unknown-linux-musl, x86_64-unknown-linux-musl, armv7-unknown-linux-musleabihf}

set -euo pipefail

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE"

TARGET="${1:-x86_64-unknown-linux-musl}"

case "$TARGET" in
  aarch64-unknown-linux-musl)
    HOST_GCC="aarch64-linux-gnu-gcc"
    PKGS="gcc-aarch64-linux-gnu"
    ;;
  x86_64-unknown-linux-musl)
    HOST_GCC="musl-gcc"
    PKGS="musl-tools"
    ;;
  armv7-unknown-linux-musleabihf)
    HOST_GCC="arm-linux-gnueabihf-gcc"
    PKGS="gcc-arm-linux-gnueabihf"
    ;;
  *)
    echo "unknown target: $TARGET" >&2
    echo "supported: aarch64-unknown-linux-musl, x86_64-unknown-linux-musl, armv7-unknown-linux-musleabihf" >&2
    exit 2
    ;;
esac

if ! command -v "$HOST_GCC" >/dev/null 2>&1; then
  echo ">>> installing cross toolchain ($PKGS)"
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y "$PKGS"
  elif command -v pkg >/dev/null 2>&1; then
    sudo pkg install -y "$PKGS"
  else
    echo "no package manager found; install $HOST_GCC manually" >&2
    exit 1
  fi
fi

if ! rustup target list --installed | grep -q "^$TARGET$"; then
  echo ">>> installing rustup target $TARGET"
  rustup target add "$TARGET"
fi

mkdir -p ~/.cargo
CFG=~/.cargo/config.toml
{
  echo "[target.$TARGET]"
  echo "linker = \"$HOST_GCC\""
} >> "$CFG"

echo ">>> building tos for $TARGET"
cargo build --release --target "$TARGET" --bin tos

OUT="target/$TARGET/release/tos"
SIZE=$(du -h "$OUT" | awk '{print $1}')
echo "✓ built $OUT ($SIZE)"

if file "$OUT" 2>/dev/null | grep -qi "statically linked"; then
  echo "✓ statically linked"
else
  echo "⚠ not statically linked (check libc dependency)"
  file "$OUT"
fi
