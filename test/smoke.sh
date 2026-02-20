#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck disable=SC1091
source "$REPO_ROOT/lib/cx.sh"

for fn in cx cxj cxo cxcopy cxdiffsum_staged cxcommitjson cxcommitmsg cxnext cxfix cxfix_run cxhealth cxversion cxdoctor cxmetrics cxprofile cxalert cxtrace cxbench cxworklog cxbudget cxstate cxpolicy cxoptimize cxprompt cxroles cxfanout cxpromptlint cxlog_tail; do
  if ! type "$fn" >/dev/null 2>&1; then
    echo "MISSING: $fn"
    exit 1
  fi
done

echo "PASS: all expected cx functions are loaded"
