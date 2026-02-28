#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

has_line() {
  local pattern="$1"
  local text="$2"
  if command -v rg >/dev/null 2>&1; then
    printf '%s\n' "$text" | rg -q "$pattern"
  else
    printf '%s\n' "$text" | grep -Eq "$pattern"
  fi
}

where_out="$(./bin/cx where cxversion _cx_git_root 2>/dev/null)" || fail "bin/cx where failed"
has_line '^== cxwhere ==$' "$where_out" || fail "where missing heading"
has_line '^bin_cx: ' "$where_out" || fail "where missing bin_cx"
has_line '^repo_root: ' "$where_out" || fail "where missing repo_root"


diag_out="$(./bin/cx diag 2>/dev/null)" || fail "bin/cx diag failed"
has_line '^== cxdiag ==$' "$diag_out" || fail "diag missing heading"
has_line '^timestamp: ' "$diag_out" || fail "diag missing timestamp"
has_line '^version: ' "$diag_out" || fail "diag missing version"
has_line '^log_file: ' "$diag_out" || fail "diag missing log_file"

set +e
parity_out="$(./bin/cx cxparity 2>&1)"
parity_status=$?
set -e
if has_line ' FAIL$' "$parity_out"; then
  [[ $parity_status -ne 0 ]] || fail "cxparity reported FAIL row but exited 0"
else
  [[ $parity_status -eq 0 ]] || fail "cxparity had no FAIL rows but exited non-zero"
fi

echo "PASS: provenance/parity tools checks"
