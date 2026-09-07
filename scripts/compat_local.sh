#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="quick"
JSON_STDOUT=0
OUT_FILE=".cx/compat/latest.json"

if ! command -v cargo >/dev/null 2>&1; then
  for cargo_bin in "$HOME/.cargo/bin" /usr/local/cargo/bin; do
    if [[ -x "$cargo_bin/cargo" ]]; then
      export PATH="$cargo_bin:$PATH"
      break
    fi
  done
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --full)
      MODE="full"
      shift
      ;;
    --quick)
      MODE="quick"
      shift
      ;;
    --json)
      JSON_STDOUT=1
      shift
      ;;
    --out)
      OUT_FILE="${2:-}"
      if [[ -z "$OUT_FILE" ]]; then
        echo "error: --out requires a path" >&2
        exit 2
      fi
      shift 2
      ;;
    *)
      echo "usage: ./scripts/compat_local.sh [--quick|--full] [--json] [--out <path>]" >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required" >&2
  exit 2
fi

now_ms() {
  python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

TSV_FILE="$(mktemp)"
trap 'rm -f "$TSV_FILE"' EXIT

OVERALL_RC=0

run_step() {
  local name="$1"
  local cmd="$2"
  local start_ms end_ms dur_ms rc
  start_ms="$(now_ms)"
  set +e
  bash -c "$cmd" 1>&2 2>&2
  rc=$?
  set -e
  end_ms="$(now_ms)"
  dur_ms=$((end_ms - start_ms))
  printf '%s\t%s\t%s\t%s\n' "$name" "$rc" "$dur_ms" "$cmd" >>"$TSV_FILE"
  if [[ "$rc" -ne 0 ]]; then
    OVERALL_RC=1
  fi
}

run_step "cargo_check_tests" "cd rust/cxrs && cargo check --locked --tests"
run_step "release_metadata_guard_tests" "cd rust/cxrs && python3 -m unittest tools.test_release_check"
run_step "release_metadata_check" "cd rust/cxrs && python3 tools/release_check.py --repo-root \"$ROOT_DIR\" --max-version-age-days 14 --require-published-status-docs"
run_step "codex_hook" "python3 test/codex_hook_test.py"
run_step "entrypoint_tests" "cd rust/cxrs && cargo test --locked --test entrypoint_integration -- --test-threads=1"
if [[ "$MODE" == "quick" ]]; then
  run_step "reliability_smoke" "cd rust/cxrs && cargo test --locked --test reliability_integration capture_pipeline_native_logs_provider_fields -- --exact --test-threads=1 && cargo test --locked --test reliability_integration http_openai_cov -- --exact --test-threads=1 && cargo test --locked --test reliability_integration schema_injection_creates_quarantine_run_flags -- --exact --test-threads=1"
  run_step "scheduler_smoke" "cd rust/cxrs && cargo test --locked --test scheduler_tests plan_json_contract -- --exact --test-threads=1 && cargo test --locked --test scheduler_tests run_all_contract -- --exact --test-threads=1 && cargo test --locked --test scheduler_tests task_check_contract -- --exact --test-threads=1"
else
  run_step "reliability_tests" "cd rust/cxrs && cargo test --locked --test reliability_integration -- --test-threads=1"
  run_step "scheduler_tests" "cd rust/cxrs && cargo test --locked --test scheduler_tests -- --test-threads=1"
fi
run_step "root_provenance" "./test/provenance_tools.sh"
run_step "root_schema" "./test/schema_registry.sh"
run_step "root_pipeline" "./test/core_pipeline.sh"

if [[ "$MODE" == "full" ]]; then
  run_step "guardrails_full" "cd rust/cxrs && CX_GUARDRAILS_SKIP_TESTS=1 ./scripts/guardrails.sh"
  run_step "compat_check_full" "cd rust/cxrs && ./scripts/compat_check.sh 50"
fi

mkdir -p "$(dirname "$OUT_FILE")"

COMPAT_MODE="$MODE" \
COMPAT_ROOT="$ROOT_DIR" \
COMPAT_OUT="$OUT_FILE" \
COMPAT_TSV="$TSV_FILE" \
COMPAT_RC="$OVERALL_RC" \
python3 - <<'PY'
import json
import os
import platform
import subprocess
from datetime import datetime, timezone

tsv = os.environ["COMPAT_TSV"]
out_path = os.environ["COMPAT_OUT"]
mode = os.environ["COMPAT_MODE"]
root = os.environ["COMPAT_ROOT"]
overall_rc = int(os.environ["COMPAT_RC"])

def sh(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, cwd=root).strip()
    except Exception:
        return ""

steps = []
with open(tsv, "r", encoding="utf-8") as fh:
    for line in fh:
        line = line.rstrip("\n")
        if not line:
            continue
        name, rc, dur_ms, cmd = line.split("\t", 3)
        steps.append(
            {
                "name": name,
                "ok": int(rc) == 0,
                "exit_code": int(rc),
                "duration_ms": int(dur_ms),
                "command": cmd,
            }
        )

report = {
    "status": "ok" if overall_rc == 0 else "failed",
    "mode": mode,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "host": {
        "os": platform.system(),
        "release": platform.release(),
        "arch": platform.machine(),
    },
    "toolchain": {
        "rustc": sh("rustc --version"),
        "cargo": sh("cargo --version"),
        "bash": sh("bash --version | head -n 1"),
    },
    "git": {
        "branch": sh("git rev-parse --abbrev-ref HEAD"),
        "head": sh("git rev-parse --short HEAD"),
    },
    "summary": {
        "steps_total": len(steps),
        "steps_failed": len([s for s in steps if not s["ok"]]),
    },
    "steps": steps,
}

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2, sort_keys=True)
PY

if [[ "$JSON_STDOUT" -eq 1 ]]; then
  cat "$OUT_FILE"
else
  echo "compat-local: mode=$MODE status=$([[ "$OVERALL_RC" -eq 0 ]] && echo PASS || echo FAIL)"
  echo "compat-local: report=$OUT_FILE"
  while IFS=$'\t' read -r name rc dur cmd; do
    status="ok"
    if [[ "$rc" -ne 0 ]]; then
      status="fail"
    fi
    echo " - [$status] $name (${dur}ms)"
  done <"$TSV_FILE"
fi

exit "$OVERALL_RC"
