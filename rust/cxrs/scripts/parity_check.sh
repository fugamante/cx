#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CXRS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

help_out="$(cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- help)"

required_cmds=(
  version where doctor state policy bench
  cx cxj cxo cxol cxcopy fix budget log-tail health
  log-off alert-show alert-off chunk
  metrics prompt roles fanout promptlint cx-compat
  profile alert optimize worklog trace
  next diffsum diffsum-staged fix-run
  commitjson commitmsg replay quarantine
)

missing=0
for cmd in "${required_cmds[@]}"; do
  if ! grep -Eq "^[[:space:]]+$cmd([[:space:]]|$)" <<<"$help_out"; then
    echo "[parity] missing command in help: $cmd" >&2
    missing=1
  fi
done

compat_aliases=(
  cxversion cxwhere cxdoctor cxmetrics cxprofile cxtrace cxalert
  cxoptimize cxworklog cxstate cxpolicy cxbench cxprompt cxroles
  cxfanout cxpromptlint cxnext cxfix cxdiffsum cxdiffsum_staged
  cxcommitjson cxcommitmsg cxbudget cxlog_tail cxhealth cxfix_run
  cxreplay cxquarantine cxchunk cxalert_show cxalert_off cxlog_off
)

for alias in "${compat_aliases[@]}"; do
  out="$(cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- cx-compat "$alias" 2>&1 || true)"
  if grep -q "unsupported command" <<<"$out"; then
    echo "[parity] unsupported compat alias: $alias" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "[parity] FAIL" >&2
  exit 1
fi

echo "[parity] PASS"
