#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

out1="$($ROOT/bin/cx core)"
out2="$($ROOT/bin/cx where cxo)"

printf '%s' "$out1" | grep -q "== cxcore =="
printf '%s' "$out2" | grep -q "route"

echo "core_pipeline_ok"
