#!/usr/bin/env bash
# Run security audits and produce SECURITY.md.
# Usage: ./scripts/audit.sh

set -euo pipefail

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE"

if ! command -v cargo-audit >/dev/null 2>&1; then
  echo ">>> installing cargo-audit"
  cargo install cargo-audit --locked
fi
if ! command -v cargo-deny >/dev/null 2>&1; then
  echo ">>> installing cargo-deny"
  cargo install cargo-deny --locked
fi

echo ">>> running cargo audit"
cargo audit --json > audit.json 2>audit.stderr || true
AUDIT_EXIT=$?
AUDIT_HITS=$(jq '.vulnerabilities.found // 0' audit.json 2>/dev/null || echo "n/a")

echo ">>> running cargo deny (advisories)"
cargo deny check advisories 2>&1 | tail -50
DENY_HITS=$(cargo deny check advisories 2>&1 | grep -c "^warning\|^error" || true)

cat > SECURITY.md <<EOF
# Security

_Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) by \`./scripts/audit.sh\`._

## Advisories (\`cargo audit\`)

\`\`\`json
$(cat audit.json | jq '{vulnerabilities: .vulnerabilities, warnings: (.list.metadata.warnings // [])}' 2>/dev/null || cat audit.json)
\`\`\`

## License compliance (\`cargo deny check licenses\`)

$(cargo deny check licenses 2>&1 | tail -30)

## Bans / sources

- multiple versions: \`warn\`
- wildcards:        \`deny\`
- unknown registry: \`deny\` (only \`crates-io\`)
- unknown git:      \`warn\`

See \`deny.toml\` in the repo root for the full policy.

## Reporting vulnerabilities

Email \`security@opceanai.com\` with a description and reproduction.
GPG key on request. Please do not open public issues for vulns.
EOF

echo "✓ wrote SECURITY.md"
echo
echo "  audit hits:  $AUDIT_HITS"
echo "  deny hits:   $DENY_HITS"
exit $AUDIT_EXIT
