#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
TEST_DIR="$ROOT_DIR/rust/cxrs/tests"
MAX_TEST_LINES="${2:-500}"

if ! [[ "$MAX_TEST_LINES" =~ ^[0-9]+$ ]]; then
  echo "error: MAX_TEST_LINES must be an integer (got '$MAX_TEST_LINES')" >&2
  exit 2
fi

required_files=(
  "$TEST_DIR/common/fixture_http.rs"
  "$TEST_DIR/common/json_contract.rs"
  "$TEST_DIR/common/telemetry_helpers.rs"
  "$TEST_DIR/task_lifecycle_tests.rs"
  "$TEST_DIR/scheduler_tests.rs"
  "$TEST_DIR/backend_fairness_tests.rs"
  "$TEST_DIR/retry_timeout_tests.rs"
  "$TEST_DIR/telemetry_contract_tests.rs"
  "$TEST_DIR/adapter_telemetry_tests.rs"
  "$TEST_DIR/schema_failure_tests.rs"
  "$TEST_DIR/cli_edge_case_tests.rs"
)

violations=0

for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "guardrail violation: missing required test/helper file: ${file#$ROOT_DIR/}" >&2
    violations=$((violations + 1))
  fi
done

while IFS= read -r -d '' file; do
  rel="${file#$ROOT_DIR/}"
  lines=$(wc -l < "$file" | tr -d ' ')
  if (( lines > MAX_TEST_LINES )); then
    echo "guardrail violation: $rel has $lines lines (max $MAX_TEST_LINES)" >&2
    violations=$((violations + 1))
  fi
  if ! rg -q '^mod common;$' "$file"; then
    echo "guardrail violation: $rel must import 'mod common;'" >&2
    violations=$((violations + 1))
  fi
  if rg -q '^\s*fn\s+.*\{\s*$' "$file" && ! rg -q '^\#\[test\]$' "$file"; then
    echo "guardrail warning: $rel has functions but no #[test] markers" >&2
  fi
done < <(find "$TEST_DIR" -maxdepth 1 -type f -name '*_tests.rs' -print0)

if (( violations > 0 )); then
  echo "failed: integration guardrails detected $violations violation(s)" >&2
  exit 1
fi

echo "ok: integration guardrails passed (max per *_tests.rs: $MAX_TEST_LINES lines)"
