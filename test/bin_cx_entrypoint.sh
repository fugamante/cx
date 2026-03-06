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
  printf '%s\n' "$out" | grep -Eq "$pattern"
}

contains_line '^version: ' || fail "cxversion missing version field"
contains_line '^execution_path: rust:bin/cx$' || fail "expected rust execution path"
contains_line '^log_file: ' || fail "cxversion missing log_file"

# Rust should handle known compat command.
./bin/cx cxversion >/dev/null || fail "rust routing for cxversion failed"

# Unsupported internal helpers should fail cleanly in rust-only mode.
set +e
helper_out="$(./bin/cx _cx_git_root 2>&1)"
helper_status=$?
set -e
[[ $helper_status -ne 0 ]] || fail "internal helper unexpectedly succeeded"
printf '%s\n' "$helper_out" | grep -Eq "^cx: unsupported command '_cx_git_root'" \
  || fail "unexpected unsupported-helper output: '$helper_out'"

echo "PASS: bin/cx routing and cxversion checks"
