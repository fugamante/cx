#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DEFAULT_IMAGE_TAG="xshelf-compat:local"
IMAGE_TAG="${CX_COMPAT_IMAGE:-$DEFAULT_IMAGE_TAG}"
IMAGE_OVERRIDDEN=0
if [[ "${IMAGE_TAG}" != "$DEFAULT_IMAGE_TAG" ]]; then
  IMAGE_OVERRIDDEN=1
fi
MODE="quick"
JSON_STDOUT=0
OUT_FILE=""
REBUILD=0
PASS_TTY=0
declare -a EXTRA_ARGS=()

usage() {
  cat >&2 <<'USAGE'
usage: ./scripts/compat_docker.sh [--smoke|--quick|--full|--ci] [--json] [--out <path>] [--image <tag>] [--rebuild] [--tty] [--docker-arg <arg>]

Builds the local compat image when needed, bind-mounts the current repository,
and runs either a Docker smoke report or scripts/compat_local.sh inside Docker.
The default image is xshelf-compat:local. --image or CX_COMPAT_IMAGE selects an
explicit existing image and never pulls remotely; use --rebuild to build the
repo Dockerfile into the selected tag.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --smoke)
      MODE="smoke"
      shift
      ;;
    --quick)
      MODE="quick"
      shift
      ;;
    --full)
      MODE="full"
      shift
      ;;
    --ci)
      MODE="ci"
      shift
      ;;
    --json)
      JSON_STDOUT=1
      shift
      ;;
    --out)
      OUT_FILE="${2:-}"
      [[ -n "$OUT_FILE" ]] || { echo "compat-docker: --out requires a path" >&2; exit 2; }
      shift 2
      ;;
    --image)
      IMAGE_TAG="${2:-}"
      [[ -n "$IMAGE_TAG" ]] || { echo "compat-docker: --image requires a tag" >&2; exit 2; }
      IMAGE_OVERRIDDEN=1
      shift 2
      ;;
    --rebuild)
      REBUILD=1
      shift
      ;;
    --tty)
      PASS_TTY=1
      shift
      ;;
    --docker-arg)
      EXTRA_ARGS+=("${2:-}")
      [[ -n "${EXTRA_ARGS[-1]}" ]] || { echo "compat-docker: --docker-arg requires a value" >&2; exit 2; }
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "compat-docker: unknown argument '$1'" >&2
      usage
      exit 2
      ;;
  esac
done

command -v docker >/dev/null 2>&1 || {
  echo "compat-docker: docker is required" >&2
  exit 2
}

if [[ "$REBUILD" -eq 1 ]]; then
  echo "compat-docker: building $IMAGE_TAG" >&2
  docker build -t "$IMAGE_TAG" "$ROOT_DIR" >&2
elif ! docker image inspect "$IMAGE_TAG" >/dev/null 2>&1; then
  if [[ "$IMAGE_OVERRIDDEN" -eq 1 ]]; then
    echo "compat-docker: image '$IMAGE_TAG' not found; pull/build it explicitly or rerun with --rebuild to build the repo Dockerfile into that tag" >&2
    exit 2
  fi
  echo "compat-docker: building $IMAGE_TAG" >&2
  docker build -t "$IMAGE_TAG" "$ROOT_DIR" >&2
fi

uid_gid="$(id -u):$(id -g)"
tty_args=()
if [[ "$PASS_TTY" -eq 1 ]] && [[ -t 0 ]] && [[ -t 1 ]]; then
  tty_args=(-it)
fi

docker_exec() {
  docker run --rm \
    "${tty_args[@]}" \
    --user "$uid_gid" \
    --workdir /work \
    -e HOME=/tmp/cx-home \
    -e CARGO_TARGET_DIR=/work/.cx/compat/docker-target \
    -e PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    -v "$ROOT_DIR":/work \
    "${EXTRA_ARGS[@]}" \
    "$IMAGE_TAG" \
    bash -c 'mkdir -p "$HOME" /work/.cx/compat/docker-target && "$@"' bash "$@"
}

now_ms() {
  python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

emit_report() {
  local mode="$1"
  local out_file="$2"
  local tsv_file="$3"
  local overall_rc="$4"
  local image_tag="$5"
  local image_overridden="$6"
  mkdir -p "$(dirname "$out_file")"
  COMPAT_MODE="$mode" \
  COMPAT_ROOT="$ROOT_DIR" \
  COMPAT_OUT="$out_file" \
  COMPAT_TSV="$tsv_file" \
  COMPAT_RC="$overall_rc" \
  COMPAT_IMAGE_TAG="$image_tag" \
  COMPAT_IMAGE_OVERRIDDEN="$image_overridden" \
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
image_tag = os.environ["COMPAT_IMAGE_TAG"]
image_overridden = os.environ["COMPAT_IMAGE_OVERRIDDEN"] == "1"

def sh(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, cwd=root).strip()
    except Exception:
        return ""

def run_args(args):
    try:
        return subprocess.check_output(args, text=True, cwd=root).strip()
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
    "ci_parity": {
        "scope": "linux-core-guardrail-subset" if mode == "ci" else "not-ci-parity",
        "workflow": ".github/workflows/cxrs-compat.yml",
        "intentional_deltas": [
            "github-event-payload-dependent command-surface diff checks",
            "hosted-runner setup and cache action behavior",
            "workflow inline contract smoke snippets",
            "artifact upload/download behavior",
            "cargo-audit install and advisory gate",
            "cargo-deny install and advisory gate",
            "cargo-outdated drift report",
        ] if mode == "ci" else [],
    },
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "host": {
        "os": platform.system(),
        "release": platform.release(),
        "arch": platform.machine(),
    },
    "docker": {
        "image_tag": image_tag,
        "image_source": "explicit_image" if image_overridden else "local_default",
        "pull_policy": "never",
        "image_id": run_args(["docker", "image", "inspect", image_tag, "--format", "{{.Id}}"]),
        "docker_version": sh("docker --version"),
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
}

if [[ "$MODE" == "smoke" || "$MODE" == "ci" ]]; then
  if [[ "$MODE" == "smoke" ]]; then
    OUT_FILE="${OUT_FILE:-.cx/compat/docker_smoke_latest.json}"
  else
    OUT_FILE="${OUT_FILE:-.cx/compat/docker_ci_latest.json}"
  fi
  TSV_FILE="$(mktemp)"
  trap 'rm -f "$TSV_FILE"' EXIT
  OVERALL_RC=0

  run_report_step() {
    local name="$1"
    local cmd="$2"
    local start_ms end_ms dur_ms rc
    start_ms="$(now_ms)"
    set +e
    docker_exec bash -c "$cmd" 1>&2 2>&2
    rc=$?
    set -e
    end_ms="$(now_ms)"
    dur_ms=$((end_ms - start_ms))
    printf '%s\t%s\t%s\t%s\n' "$name" "$rc" "$dur_ms" "$cmd" >>"$TSV_FILE"
    if [[ "$rc" -ne 0 ]]; then
      OVERALL_RC=1
    fi
  }

  if [[ "$MODE" == "smoke" ]]; then
    run_report_step \
      "release_metadata_guard_tests" \
      "cd /work/rust/cxrs && python3 -m unittest tools.test_release_check"
    run_report_step \
      "release_metadata_check" \
      "cd /work/rust/cxrs && python3 tools/release_check.py --repo-root /work --max-version-age-days 14"
    run_report_step \
      "runtime_smoke" \
      "cargo --version >/dev/null && ./bin/cx version >/tmp/cxversion.txt && ./bin/xshelf version >/tmp/xshelf_version.txt && ./bin/cx schema list --json | jq -e '.file_count >= 4' >/dev/null && ./bin/cx core --json | jq -e '.contract_version == \"core.v1\"' >/dev/null"
  else
    run_report_step \
      "action_pin_guardrail" \
      "cd /work && ./scripts/check_action_pins.sh /work"
    run_report_step \
      "rust_toolchain_sync" \
      "cd /work && python3 ./scripts/check_rust_toolchain_sync.py --repo-root /work"
    run_report_step \
      "rust_fmt_check" \
      "cd /work/rust/cxrs && cargo fmt --check"
    run_report_step \
      "rust_check" \
      "cd /work/rust/cxrs && cargo check"
    run_report_step \
      "rust_clippy" \
      "cd /work/rust/cxrs && cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments"
    run_report_step \
      "rust_tests" \
      "cd /work/rust/cxrs && cargo test --tests -- --test-threads=1"
    run_report_step \
      "rust_file_line_guardrail" \
      "cd /work && ./rust/cxrs/scripts/check_rs_max_lines.sh 600 /work"
    run_report_step \
      "naming_guardrails" \
      "cd /work && python3 ./rust/cxrs/scripts/check_rust_naming.py --root rust/cxrs --max-fn-len 52 --max-type-len 48 --max-const-len 48 && python3 ./rust/cxrs/scripts/check_fn_name_length.py --root rust/cxrs --max-len 52 && python3 ./rust/cxrs/scripts/check_fn_name_length.py --root rust/cxrs/tests --max-len 52 --max-segments 7 && python3 ./rust/cxrs/scripts/check_test_naming.py --root rust/cxrs/tests --max-len 48 --max-segments 7"
    run_report_step \
      "integration_guardrails" \
      "cd /work && ./rust/cxrs/scripts/check_integration_guardrails.sh /work 500"
    run_report_step \
      "quality_gate" \
      "cd /work/rust/cxrs && python3 tools/quality_gate.py --max-file-lines 100000 --max-fn-lines 100000 --max-raw-eprintln 0"
    run_report_step \
      "release_metadata_guard_tests" \
      "cd /work/rust/cxrs && python3 -m unittest tools.test_release_check"
    run_report_step \
      "release_metadata_check" \
      "cd /work/rust/cxrs && python3 tools/release_check.py --repo-root /work --max-version-age-days 14"
    run_report_step \
      "compat_check" \
      "cd /work/rust/cxrs && ./scripts/compat_check.sh 50"
    run_report_step \
      "shell_regression_suite" \
      "cd /work && mockdir=\"\$(mktemp -d)\" && trap 'rm -rf \"\$mockdir\"' EXIT && cp ./scripts/mock_codex_jsonl.sh \"\$mockdir/codex\" && chmod +x \"\$mockdir/codex\" && PATH=\"\$mockdir:\$PATH\" && for t in test/*.sh; do bash \"\$t\"; done"
  fi

  emit_report "$MODE" "$OUT_FILE" "$TSV_FILE" "$OVERALL_RC" "$IMAGE_TAG" "$IMAGE_OVERRIDDEN"
  if [[ "$JSON_STDOUT" -eq 1 ]]; then
    cat "$OUT_FILE"
  else
    echo "compat-docker: mode=$MODE status=$([[ "$OVERALL_RC" -eq 0 ]] && echo PASS || echo FAIL)"
    echo "compat-docker: report=$OUT_FILE"
    while IFS=$'\t' read -r name rc dur cmd; do
      status="ok"
      if [[ "$rc" -ne 0 ]]; then
        status="fail"
      fi
      echo " - [$status] $name (${dur}ms)"
    done <"$TSV_FILE"
  fi
  exit "$OVERALL_RC"
fi

container_cmd=(./scripts/compat_local.sh "--$MODE")
if [[ "$JSON_STDOUT" -eq 1 ]]; then
  container_cmd+=(--json)
fi
if [[ -n "$OUT_FILE" ]]; then
  container_cmd+=(--out "$OUT_FILE")
fi

docker_exec "${container_cmd[@]}"
