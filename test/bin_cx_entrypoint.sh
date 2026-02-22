#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

out="$(./bin/cx cxversion)" || fail "bin/cx cxversion failed"
printf '%s\n' "$out" | rg -q '^version: ' || fail "cxversion missing version field"
printf '%s\n' "$out" | rg -q '^execution_path: rust:bin/cx$' || fail "expected rust execution path"
printf '%s\n' "$out" | rg -q '^log_file: ' || fail "cxversion missing log_file"

# Rust should handle known compat command.
./bin/cx cxversion >/dev/null || fail "rust routing for cxversion failed"

# Fallback should handle bash-only helper (not in rust compat map).
root_out="$(./bin/cx _cx_git_root)" || fail "bash fallback command failed"
[[ "$root_out" == "$REPO_ROOT" ]] || fail "unexpected fallback output: '$root_out'"

echo "PASS: bin/cx routing and cxversion checks"
