#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

out="$(./bin/cx cxversion)" || fail "bin/cx cxversion failed"
contains_line() {
  local pattern="$1"
  if command -v rg >/dev/null 2>&1; then
    printf '%s\n' "$out" | rg -q "$pattern"
  else
    printf '%s\n' "$out" | grep -Eq "$pattern"
  fi
}

contains_line '^version: ' || fail "cxversion missing version field"
contains_line '^execution_path: rust:bin/cx$' || fail "expected rust execution path"
contains_line '^log_file: ' || fail "cxversion missing log_file"

# Rust should handle known compat command.
./bin/cx cxversion >/dev/null || fail "rust routing for cxversion failed"

# Fallback should handle bash-only helper (not in rust compat map).
root_out="$(./bin/cx _cx_git_root)" || fail "bash fallback command failed"
[[ "$root_out" == "$REPO_ROOT" ]] || fail "unexpected fallback output: '$root_out'"

echo "PASS: bin/cx routing and cxversion checks"
