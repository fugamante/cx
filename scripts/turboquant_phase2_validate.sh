#!/usr/bin/env bash

set -euo pipefail

cx_tqv_die() {
  printf 'turboquant_phase2_validate: %s\n' "$*" >&2
  exit 1
}

cx_tqv_parse() {
  local out_stdout="$1"
  local out_stderr="$2"
  local prompt_label="$3"
  local context_size="$4"
  local predict_n="$5"
  local mode="$6"
  local prompt_name="$7"

  python3 - "$out_stdout" "$out_stderr" "$prompt_label" "$context_size" "$predict_n" "$mode" "$prompt_name" <<'PY'
import json
import pathlib
import re
import sys

stdout_path = pathlib.Path(sys.argv[1])
stderr_path = pathlib.Path(sys.argv[2])
prompt_label = sys.argv[3]
context_target = int(sys.argv[4])
predict_n = int(sys.argv[5])
mode = sys.argv[6]
prompt_name = sys.argv[7]
stdout_text = stdout_path.read_text()
stderr_text = stderr_path.read_text()
metrics_text = stderr_text + "\n" + stdout_text

def cap(pattern, cast=float):
    m = re.search(pattern, metrics_text, re.M)
    if not m:
        return None
    return cast(m.group(1))

prompt_tps = cap(r"\[ Prompt: ([0-9.]+) t/s \|")
decode_tps = cap(r"\| Generation: ([0-9.]+) t/s \]")
wall_s = cap(r"^real ([0-9.]+)$", float)

report_rows = []
report_re = re.compile(
    r"turboquant_report: layer=(?P<layer>\d+) rows=(?P<rows>\d+) raw=(?P<raw>\d+) "
    r"sidecar=(?P<sidecar>\d+) simulated=(?P<simulated>\d+) "
    r"raw_ratio=(?P<raw_ratio>[0-9.]+) sim_ratio=(?P<sim_ratio>[0-9.]+) "
    r"bypassed=(?P<bypassed>\d+)"
    r"(?: evicted_rows=(?P<evicted_rows>\d+) evicted_bytes=(?P<evicted_bytes>\d+) "
    r"decode_calls=(?P<decode_calls>\d+) decode_rows=(?P<decode_rows>\d+))?"
    r"(?: codec=(?P<codec>[a-z_]+))?"
)
read_rows = []
read_re = re.compile(
    r"turboquant_read: layer=(?P<layer>\d+) codec=(?P<codec>[a-z_]+) "
    r"decode_calls=(?P<decode_calls>\d+) decode_rows=(?P<decode_rows>\d+) "
    r"decode_groups=(?P<decode_groups>\d+) raw_evicted=(?P<raw_evicted>\d+) rows=(?P<rows>\d+)"
)
for match in report_re.finditer(stderr_text):
    gd = match.groupdict()
    report_rows.append({
        "layer": int(gd["layer"]),
        "rows": int(gd["rows"]),
        "raw": int(gd["raw"]),
        "sidecar": int(gd["sidecar"]),
        "simulated": int(gd["simulated"]),
        "raw_ratio": float(gd["raw_ratio"]),
        "sim_ratio": float(gd["sim_ratio"]),
        "bypassed": int(gd["bypassed"]),
        "codec": gd.get("codec"),
        "evicted_rows": None if gd.get("evicted_rows") is None else int(gd["evicted_rows"]),
        "evicted_bytes": None if gd.get("evicted_bytes") is None else int(gd["evicted_bytes"]),
        "decode_calls": None if gd.get("decode_calls") is None else int(gd["decode_calls"]),
        "decode_rows": None if gd.get("decode_rows") is None else int(gd["decode_rows"]),
    })
for match in read_re.finditer(stderr_text):
    gd = match.groupdict()
    read_rows.append({
        "layer": int(gd["layer"]),
        "codec": gd["codec"],
        "decode_calls": int(gd["decode_calls"]),
        "decode_rows": int(gd["decode_rows"]),
        "decode_groups": int(gd["decode_groups"]),
        "raw_evicted": int(gd["raw_evicted"]),
        "rows": int(gd["rows"]),
    })

report_summary = None
if report_rows:
    totals = {
        "layers": len(report_rows),
        "raw": sum(r["raw"] for r in report_rows),
        "sidecar": sum(r["sidecar"] for r in report_rows),
        "simulated": sum(r["simulated"] for r in report_rows),
        "rows": sum(r["rows"] for r in report_rows),
        "bypassed": sum(r["bypassed"] for r in report_rows),
        "evicted_rows": sum((r["evicted_rows"] or 0) for r in report_rows),
        "evicted_bytes": sum((r["evicted_bytes"] or 0) for r in report_rows),
        "decode_calls": sum((r["decode_calls"] or 0) for r in report_rows),
        "decode_rows": sum((r["decode_rows"] or 0) for r in report_rows),
    }
    totals["raw_ratio"] = None if totals["raw"] == 0 else round(100.0 * totals["sidecar"] / totals["raw"], 2)
    totals["sim_ratio"] = None if totals["simulated"] == 0 else round(100.0 * totals["sidecar"] / totals["simulated"], 2)
    codecs = sorted({r["codec"] for r in report_rows if r.get("codec")})
    totals["codecs"] = codecs
    report_summary = totals
if read_rows:
    read_summary = {
        "layers": len(read_rows),
        "decode_calls": sum(r["decode_calls"] for r in read_rows),
        "decode_rows": sum(r["decode_rows"] for r in read_rows),
        "decode_groups": sum(r["decode_groups"] for r in read_rows),
        "rows": sum(r["rows"] for r in read_rows),
        "codecs": sorted({r["codec"] for r in read_rows}),
    }
    if report_summary is None:
        report_summary = {}
    report_summary["read"] = read_summary

response = stdout_text
lines = []
capture = False
for raw in response.splitlines():
    line = raw.rstrip()
    if line.startswith("turboquant_report:"):
        continue
    if line.startswith("> "):
        capture = True
        continue
    if capture:
        if line.startswith("[ Prompt:"):
            break
        if line == "Exiting...":
            break
        lines.append(line)
clean = "\n".join(lines).strip()
blocks = [blk.strip() for blk in re.split(r"\n\s*\n", clean) if blk.strip()]
if blocks:
    clean = blocks[-1]

quality = "observe"
passed = None
if prompt_name == "smoke":
    quality = "exact_OK"
    passed = clean == "OK"
elif prompt_name == "retrieval":
    quality = "exact_TURBO-314159"
    passed = clean == "TURBO-314159"
elif prompt_name == "instruct":
    quality = "exact_JSON_contract"
    try:
        obj = json.loads(clean)
        passed = obj == {
            "status": "ready",
            "focus": "turboquant-baseline",
            "next_step": "phase2-v-cache",
        }
    except Exception:
        passed = False
elif prompt_name == "context_fill":
    quality = "non_empty_summary"
    passed = bool(clean)

result = {
    "mode": mode,
    "prompt_name": prompt_name,
    "prompt_file": prompt_label,
    "context_target": context_target,
    "predict_n": predict_n,
    "prompt_tokens_per_sec": prompt_tps,
    "decode_tokens_per_sec": decode_tps,
    "wall_ms": None if wall_s is None else round(wall_s * 1000),
    "quality_rule": quality,
    "passed": passed,
    "response_text": clean,
    "turboquant_report": report_summary,
}
print(json.dumps(result))
PY
}

cx_tqv_run_one() {
  local binary="$1"
  local model="$2"
  local prompt_file="$3"
  local prompt_label="$4"
  local context_size="$5"
  local predict_n="$6"
  local mode="$7"
  local prompt_name="$8"
  local extra_args="${9:-}"
  local out_stdout out_stderr
  out_stdout="$(mktemp)"
  out_stderr="$(mktemp)"

  local -a cmd=(
    /usr/bin/time -p
    "$binary"
    --model "$model"
    -c "$context_size"
    -n "$predict_n"
    --temp 0
    --perf
    --single-turn
    --simple-io
    --no-display-prompt
    -f "$prompt_file"
  )

  if [[ "$mode" == "turboquant" ]]; then
    cmd+=(--turboquant-enable --turboquant-group-size 64 --turboquant-codebook-bits 8)
  fi

  local -a env_cmd=()
  if [[ "$mode" == "turboquant" ]]; then
    env_cmd+=(env LLAMA_TQ_REPORT="${LLAMA_TQ_REPORT:-1}")
  fi

  if [[ -n "$extra_args" ]]; then
    # shellcheck disable=SC2206
    local extra_array=( $extra_args )
    cmd+=("${extra_array[@]}")
  fi

  "${env_cmd[@]}" "${cmd[@]}" >"$out_stdout" 2>"$out_stderr"
  cx_tqv_parse "$out_stdout" "$out_stderr" "$prompt_label" "$context_size" "$predict_n" "$mode" "$prompt_name"
  rm -f "$out_stdout" "$out_stderr"
}

cx_tqv_run() {
  local binary="${CX_TURBOQUANT_LLAMA_CLI:-}"
  local model=""
  local out=""
  local context_size="8192"
  local predict_n="64"
  local extra_args="${CX_TURBOQUANT_VALIDATE_EXTRA_ARGS:-}"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --binary)
        shift
        binary="${1:-}"
        ;;
      --model)
        shift
        model="${1:-}"
        ;;
      --out)
        shift
        out="${1:-}"
        ;;
      --ctx)
        shift
        context_size="${1:-}"
        ;;
      --predict)
        shift
        predict_n="${1:-}"
        ;;
      --extra-args)
        shift
        extra_args="${1:-}"
        ;;
      *)
        cx_tqv_die "unknown arg: $1"
        ;;
    esac
    shift
  done

  [[ -n "$binary" ]] || cx_tqv_die "--binary is required (or set CX_TURBOQUANT_LLAMA_CLI)"
  [[ -x "$binary" ]] || cx_tqv_die "binary not executable: $binary"
  [[ -n "$model" ]] || cx_tqv_die "--model is required"
  [[ -f "$model" ]] || cx_tqv_die "model not found: $model"
  [[ -n "$out" ]] || cx_tqv_die "--out is required"

  local repo_root
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  local prompts_root="$repo_root/docs/turboquant/prompts"
  local prompts=(
    "smoke:docs/turboquant/prompts/smoke.txt:$prompts_root/smoke.txt:8"
    "context_fill:docs/turboquant/prompts/context_fill.txt:$prompts_root/context_fill.txt:$predict_n"
    "retrieval:docs/turboquant/prompts/retrieval.txt:$prompts_root/retrieval.txt:16"
    "instruct:docs/turboquant/prompts/instruct.txt:$prompts_root/instruct.txt:48"
  )

  python3 - "$out" "$(basename "$binary")" "$(basename "$model")" "$context_size" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = {
    "contract_version": "turboquant-phase2-validate.v1",
    "binary": sys.argv[2],
    "model": sys.argv[3],
    "context_target": int(sys.argv[4]),
    "runs": [],
}
path.write_text(json.dumps(data, indent=2) + "\n")
PY

  local entry
  for entry in "${prompts[@]}"; do
    IFS=: read -r prompt_name prompt_label prompt_file prompt_predict <<<"$entry"
    local base_json tq_json
    base_json="$(cx_tqv_run_one "$binary" "$model" "$prompt_file" "$prompt_label" "$context_size" "$prompt_predict" baseline "$prompt_name" "$extra_args")"
    tq_json="$(cx_tqv_run_one "$binary" "$model" "$prompt_file" "$prompt_label" "$context_size" "$prompt_predict" turboquant "$prompt_name" "$extra_args")"
    python3 - "$out" "$base_json" "$tq_json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data["runs"].append(json.loads(sys.argv[2]))
data["runs"].append(json.loads(sys.argv[3]))
path.write_text(json.dumps(data, indent=2) + "\n")
PY
  done

  cat "$out"
}

cx_tqv_help() {
  cat <<'EOF'
usage:
  turboquant_phase2_validate.sh run --binary <llama-cli> --model <path.gguf> --out <path.json> [--ctx <n>] [--extra-args "..."]

env:
  CX_TURBOQUANT_LLAMA_CLI   optional default for --binary
  CX_TURBOQUANT_VALIDATE_EXTRA_ARGS   optional extra backend args
EOF
}

main() {
  local sub="${1:-}"
  case "$sub" in
    run)
      shift
      cx_tqv_run "$@"
      ;;
    ""|-h|--help|help)
      cx_tqv_help
      ;;
    *)
      cx_tqv_die "unknown subcommand: $sub"
      ;;
  esac
}

main "$@"
