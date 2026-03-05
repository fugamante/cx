#!/usr/bin/env bash
set -euo pipefail

MAX_LINES="${1:-600}"
ROOT_DIR="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
ALLOWLIST_FILE="${3:-$ROOT_DIR/rust/cxrs/config/line_length_allowlist.txt}"

if ! [[ "$MAX_LINES" =~ ^[0-9]+$ ]]; then
  echo "error: MAX_LINES must be an integer (got '$MAX_LINES')" >&2
  exit 2
fi

declare -A ALLOW
if [[ -f "$ALLOWLIST_FILE" ]]; then
  while IFS= read -r line; do
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    ALLOW["$line"]=1
  done < "$ALLOWLIST_FILE"
fi

violations=0
while IFS= read -r -d '' file; do
  rel="${file#$ROOT_DIR/}"
  lines=$(wc -l < "$file" | tr -d ' ')
  if (( lines > MAX_LINES )); then
    if [[ -n "${ALLOW[$rel]:-}" ]]; then
      continue
    fi
    echo "line-limit violation: $rel has $lines lines (max $MAX_LINES)" >&2
    violations=$((violations + 1))
  fi
done < <(find "$ROOT_DIR/rust/cxrs" -type f -name '*.rs' -not -path '*/target/*' -print0)

if (( violations > 0 )); then
  echo "failed: $violations Rust file(s) exceed $MAX_LINES lines" >&2
  exit 1
fi

echo "ok: all Rust files are within $MAX_LINES lines (allowlist applied: $ALLOWLIST_FILE)"
