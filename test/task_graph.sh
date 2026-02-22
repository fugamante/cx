#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$TMP"
git init -q

id1="$($ROOT/bin/cx task add "Implement parser" --role implementer)"
id2="$($ROOT/bin/cx task add "Review parser" --role reviewer --parent "$id1")"

$ROOT/bin/cx task claim "$id1" >/dev/null
$ROOT/bin/cx task complete "$id1" >/dev/null
$ROOT/bin/cx task show "$id2" | jq -e '.role=="reviewer"' >/dev/null
$ROOT/bin/cx task fanout "Ship feature" >/dev/null

jq -e 'type=="array" and length>=4' .codex/tasks.json >/dev/null
jq -e 'map(has("id") and has("status") and has("role")) | all' .codex/tasks.json >/dev/null

echo "task_graph_ok"
