#!/usr/bin/env bash
# Build a Debian package for the `tos` binary.
# Usage: ./scripts/make-deb.sh [version]
#   version defaults to "0.1.0" or the value of $VERSION

set -euo pipefail

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE"

VERSION="${1:-${VERSION:-0.1.0}}"
ARCH=$(dpkg --print-architecture 2>/dev/null || echo "amd64")
PKG_NAME="tos"
PKG_FILE="${PKG_NAME}_${VERSION}_${ARCH}.deb"

DIST=dist
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/DEBIAN"
mkdir -p "$STAGE/usr/local/bin"
mkdir -p "$STAGE/usr/local/share/man/man1"
mkdir -p "$STAGE/etc/bash_completion.d"
mkdir -p "$STAGE/usr/lib/systemd/system"

echo ">>> building release binary"
cargo build --release --bin tos

echo ">>> staging tree at $STAGE"
install -m 0755 target/release/tos "$STAGE/usr/local/bin/tos"
install -m 0644 scripts/tos.1 "$STAGE/usr/local/share/man/man1/tos.1"
gzip -9 "$STAGE/usr/local/share/man/man1/tos.1"

# bash completion
{
  echo "_tos_complete() {"
  echo "  local cur=\"\${COMP_WORDS[COMP_CLINE]}\""
  echo "  COMPREPLY=( \$(compgen -W \"push sync schema topology node status log help\" -- \"\$cur\") )"
  echo "}"
  echo "complete -F _tos_complete tos"
} > "$STAGE/etc/bash_completion.d/tos"

# systemd unit
{
  echo "[Unit]"
  echo "Description=ToS node (P2P data sync daemon)"
  echo "After=network.target"
  echo
  echo "[Service]"
  echo "Type=simple"
  echo "ExecStart=/usr/local/bin/tos node start --config /etc/tos/topology.toml"
  echo "Restart=on-failure"
  echo "RestartSec=5"
  echo "User=tos"
  echo "Group=tos"
  echo
  echo "[Install]"
  echo "WantedBy=multi-user.target"
} > "$STAGE/usr/lib/systemd/system/tos.service"

# control file
cat > "$STAGE/DEBIAN/control" <<EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: net
Priority: optional
Architecture: ${ARCH}
Depends: libc6
Maintainer: OpceanAI <ag@opceanai.com>
Description: Translation of Service — P2P data sync
 ToS moves typed rows between any source and any destination
 (PostgreSQL, MySQL, SQLite, MongoDB, Redis, JSON, YAML, TXT)
 with a single command or a long-running daemon.
EOF

cat > "$STAGE/DEBIAN/conffiles" <<EOF
/etc/tos/topology.toml
EOF

mkdir -p "$DIST"
dpkg-deb --build --root-owner-group "$STAGE" "$DIST/$PKG_FILE"

echo "✓ built $DIST/$PKG_FILE"
ls -lh "$DIST/$PKG_FILE"
dpkg-deb -I "$DIST/$PKG_FILE" | head -20
