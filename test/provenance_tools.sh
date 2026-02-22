#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

where_out="$(./bin/cx where cxversion _cx_git_root 2>/dev/null)" || fail "bin/cx where failed"
printf '%s\n' "$where_out" | rg -q '^== cxwhere ==$' || fail "where missing heading"
printf '%s\n' "$where_out" | rg -q '^bin_cx: ' || fail "where missing bin_cx"
printf '%s\n' "$where_out" | rg -q '^repo_root: ' || fail "where missing repo_root"


diag_out="$(./bin/cx diag 2>/dev/null)" || fail "bin/cx diag failed"
printf '%s\n' "$diag_out" | rg -q '^== cxdiag ==$' || fail "diag missing heading"
printf '%s\n' "$diag_out" | rg -q '^timestamp: ' || fail "diag missing timestamp"
printf '%s\n' "$diag_out" | rg -q '^version: ' || fail "diag missing version"
printf '%s\n' "$diag_out" | rg -q '^log_file: ' || fail "diag missing log_file"

set +e
parity_out="$(./bin/cx cxparity 2>&1)"
parity_status=$?
set -e
if printf '%s\n' "$parity_out" | rg -q ' FAIL$'; then
  [[ $parity_status -ne 0 ]] || fail "cxparity reported FAIL row but exited 0"
else
  [[ $parity_status -eq 0 ]] || fail "cxparity had no FAIL rows but exited non-zero"
fi

echo "PASS: provenance/parity tools checks"
