#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$TMP"
git init -q

t1="$($ROOT/bin/cx task add "cxnext" --role implementer)"
t2="$($ROOT/bin/cx task add "cxo git status" --role reviewer --parent "$t1")"

$ROOT/bin/cx task run "$t1" >/dev/null || true
$ROOT/bin/cx task run-all >/dev/null || true

s1="$($ROOT/bin/cx task show "$t1" | jq -r '.status')"
s2="$($ROOT/bin/cx task show "$t2" | jq -r '.status')"
[[ "$s1" == "failed" || "$s1" == "complete" ]]
[[ "$s2" == "failed" || "$s2" == "complete" ]]

tail -n 5 .codex/cxlogs/runs.jsonl | jq -e 'select(.task_id!=null and .task_id!="")' >/dev/null

echo "task_run_ok"
