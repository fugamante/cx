#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
WORKFLOW_DIR="$ROOT_DIR/.github/workflows"

if [[ ! -d "$WORKFLOW_DIR" ]]; then
  echo "action pin guardrail: no workflow directory at $WORKFLOW_DIR" >&2
  exit 0
fi

rc=0
while IFS= read -r -d '' wf; do
  while IFS= read -r line; do
    use_ref="${line#*uses: }"
    use_ref="${use_ref%%#*}"
    use_ref="$(printf '%s' "$use_ref" | xargs)"

    [[ -z "$use_ref" ]] && continue
    [[ "$use_ref" == ./* ]] && continue
    [[ "$use_ref" == docker://* ]] && continue

    if [[ "$use_ref" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(/[^@[:space:]]+)?@[0-9a-f]{40}$ ]]; then
      continue
    fi

    echo "action pin guardrail violation: $wf uses '$use_ref' (expected full 40-char commit SHA)" >&2
    rc=1
  done < <(grep -E '^[[:space:]]*uses:[[:space:]]+' "$wf" || true)
done < <(find "$WORKFLOW_DIR" -type f \( -name '*.yml' -o -name '*.yaml' \) -print0)

if [[ "$rc" -ne 0 ]]; then
  echo "failed: third-party workflow actions must be pinned by commit SHA" >&2
  exit 1
fi

echo "ok: workflow action pin guardrail passed"
