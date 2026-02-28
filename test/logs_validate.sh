#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$TMP"
git init -q

$ROOT/bin/cx cxo git status >/dev/null 2>/dev/null || true
$ROOT/bin/cx logs validate >/dev/null

printf '{"bad":1}\n' >> .codex/cxlogs/runs.jsonl
if ! $ROOT/bin/cx logs validate >/dev/null 2>/dev/null; then
  echo "expected logs validate default mode to pass with warnings" >&2
  exit 1
fi

if $ROOT/bin/cx logs validate --strict >/dev/null 2>/dev/null; then
  echo "expected logs validate --strict to fail on malformed contract" >&2
  exit 1
fi

echo "logs_validate_ok"
