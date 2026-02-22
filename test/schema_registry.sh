#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

out="$($ROOT/bin/cx schema list --json)"
count="$(printf '%s' "$out" | jq -r '.file_count')"
[[ "$count" -ge 4 ]]

for name in commitjson.schema.json diffsum.schema.json next.schema.json fixrun.schema.json; do
  printf '%s' "$out" | jq -e --arg n "$name" '.schemas[] | select(.name == $n)' >/dev/null
  [[ -f "$ROOT/.codex/schemas/$name" ]]
done

$ROOT/bin/cx supports commitjson >/dev/null
$ROOT/bin/cx supports diffsum >/dev/null
$ROOT/bin/cx supports next >/dev/null
$ROOT/bin/cx supports fix-run >/dev/null

echo "schema_registry_ok"
