#!/usr/bin/env bash

set -euo pipefail

cx_tq_die() {
  printf 'turboquant_phase1: %s\n' "$*" >&2
  exit 1
}

cx_tq_backend_ref() {
  command -v llama-cli >/dev/null 2>&1 || cx_tq_die "llama-cli not found"
  llama-cli --version 2>&1 | tail -n 2
}

cx_tq_mem_gb() {
  local mem_bytes
  mem_bytes="$(sysctl -n hw.memsize 2>/dev/null || printf '0')"
  awk -v bytes="$mem_bytes" 'BEGIN { printf "%.0f\n", bytes / 1024 / 1024 / 1024 }'
}

cx_tq_accel() {
  if llama-cli --list-devices 2>&1 | grep -Eq 'MTL[0-9]|Apple M[0-9]'; then
    printf 'metal\n'
    return
  fi
  if [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
    printf 'metal\n'
    return
  fi
  printf 'unknown\n'
}

cx_tq_init_artifact() {
  local out="${1:-}"
  [[ -n "$out" ]] || cx_tq_die "init-artifact requires an output path"
  local backend_ref
  backend_ref="$(cx_tq_backend_ref | tail -n 2 | tr '\n' ' ' | sed 's/  */ /g; s/ $//')"
  cat >"$out" <<EOF
{
  "contract_version": "turboquant-baseline.v1",
  "phase": "phase1",
  "backend": {
    "name": "llama.cpp",
    "ref": "$(printf '%s' "$backend_ref" | sed 's/"/\\"/g')"
  },
  "hardware": {
    "label": "$(uname -m)-darwin",
    "memory_gb": $(cx_tq_mem_gb),
    "accel": "$(cx_tq_accel)"
  },
  "profiles": []
}
EOF
}

cx_tq_probe() {
  command -v llama-cli >/dev/null 2>&1 || cx_tq_die "llama-cli not found"
  local model_path=""
  local hf_repo=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --model)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--model requires a path"
        model_path="$1"
        ;;
      --hf-repo)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--hf-repo requires a repo spec"
        hf_repo="$1"
        ;;
      *)
        cx_tq_die "unknown probe arg: $1"
        ;;
    esac
    shift
  done

  if [[ -n "$model_path" && -n "$hf_repo" ]]; then
    cx_tq_die "choose either --model or --hf-repo"
  fi
  if [[ -z "$model_path" && -z "$hf_repo" ]]; then
    cx_tq_die "probe requires --model or --hf-repo"
  fi

  local -a cmd=(
    llama-cli
    -c 512
    -n 8
    --temp 0
    --perf
    --single-turn
    --simple-io
    --no-display-prompt
    -p "Reply with exactly: OK"
  )
  if [[ -n "$model_path" ]]; then
    cmd+=(--model "$model_path")
  else
    cmd+=(--hf-repo "$hf_repo")
  fi

  /usr/bin/time -p "${cmd[@]}"
}

cx_tq_token_count() {
  command -v llama-tokenize >/dev/null 2>&1 || cx_tq_die "llama-tokenize not found"
  local model_path=""
  local prompt_file=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --model)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--model requires a path"
        model_path="$1"
        ;;
      --file)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--file requires a path"
        prompt_file="$1"
        ;;
      *)
        cx_tq_die "unknown token-count arg: $1"
        ;;
    esac
    shift
  done
  [[ -n "$model_path" ]] || cx_tq_die "token-count requires --model"
  [[ -n "$prompt_file" ]] || cx_tq_die "token-count requires --file"
  [[ -f "$prompt_file" ]] || cx_tq_die "prompt file not found: $prompt_file"

  llama-tokenize --model "$model_path" -f "$prompt_file" --show-count --log-disable 2>/dev/null \
    | awk -F': ' '/^Total number of tokens:/ { print $2 }'
}

cx_tq_measure() {
  command -v llama-cli >/dev/null 2>&1 || cx_tq_die "llama-cli not found"
  local model_path=""
  local hf_repo=""
  local prompt_file=""
  local context_size=""
  local predict_n="64"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --model)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--model requires a path"
        model_path="$1"
        ;;
      --hf-repo)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--hf-repo requires a repo spec"
        hf_repo="$1"
        ;;
      --file)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--file requires a path"
        prompt_file="$1"
        ;;
      --ctx)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--ctx requires a value"
        context_size="$1"
        ;;
      --predict)
        shift
        [[ $# -gt 0 ]] || cx_tq_die "--predict requires a value"
        predict_n="$1"
        ;;
      *)
        cx_tq_die "unknown measure arg: $1"
        ;;
    esac
    shift
  done

  if [[ -n "$model_path" && -n "$hf_repo" ]]; then
    cx_tq_die "choose either --model or --hf-repo"
  fi
  if [[ -z "$model_path" && -z "$hf_repo" ]]; then
    cx_tq_die "measure requires --model or --hf-repo"
  fi
  [[ -n "$prompt_file" ]] || cx_tq_die "measure requires --file"
  [[ -f "$prompt_file" ]] || cx_tq_die "prompt file not found: $prompt_file"
  [[ -n "$context_size" ]] || cx_tq_die "measure requires --ctx"

  local prompt_tokens="null"
  if [[ -n "$model_path" ]]; then
    prompt_tokens="$(cx_tq_token_count --model "$model_path" --file "$prompt_file")"
  fi

  local out_file
  out_file="$(mktemp)"

  local -a cmd=(
    /usr/bin/time -p
    llama-cli
    -c "$context_size"
    -n "$predict_n"
    --temp 0
    --perf
    --single-turn
    --simple-io
    --no-display-prompt
    -f "$prompt_file"
  )
  if [[ -n "$model_path" ]]; then
    cmd+=(--model "$model_path")
  else
    cmd+=(--hf-repo "$hf_repo")
  fi

  "${cmd[@]}" >"$out_file" 2>&1

  python3 - "$out_file" "$prompt_file" "$context_size" "$predict_n" "$prompt_tokens" <<'PY'
import json
import math
import pathlib
import re
import sys

out_path = pathlib.Path(sys.argv[1])
prompt_file = pathlib.Path(sys.argv[2])
context_target = int(sys.argv[3])
predict_n = int(sys.argv[4])
prompt_tokens_raw = sys.argv[5]
prompt_tokens = None if prompt_tokens_raw == "null" else int(prompt_tokens_raw)
text = out_path.read_text()

def cap(pattern, cast=float):
    m = re.search(pattern, text, re.M)
    if not m:
        return None
    return cast(m.group(1))

prompt_tps = cap(r"\[ Prompt: ([0-9.]+) t/s \|")
decode_tps = cap(r"\| Generation: ([0-9.]+) t/s \]")
wall_s = cap(r"^real ([0-9.]+)$", float)
kv_total = None
kv_context = None
for line in text.splitlines():
    if "llama_memory_breakdown_print:" in line and "MTL0" in line:
        mt = re.search(r"\|\s*[0-9]+\s*=\s*[0-9]+\s+\+\s+\(([0-9]+)\s*=", line)
        mc = re.search(r"\(\s*[0-9]+\s*=\s*[0-9]+\s+\+\s+([0-9]+)\s+\+", line)
        if mt:
            kv_total = int(mt.group(1))
        if mc:
            kv_context = int(mc.group(1))
        break
prefill_ms = None
if prompt_tokens is not None and prompt_tps and prompt_tps > 0:
    prefill_ms = round((prompt_tokens / prompt_tps) * 1000)

response = text
if "llama_memory_breakdown_print:" in response:
    response = response.split("llama_memory_breakdown_print:", 1)[0]
if "available commands:" in response:
    response = response.split("available commands:", 1)[1]
lines = []
capture = False
for raw in response.splitlines():
    line = raw.rstrip()
    if line.startswith("> "):
        capture = True
        continue
    if capture:
        lines.append(line)
clean = "\n".join(lines).strip()
if "\n\n" in clean:
    clean = clean.split("\n\n")[-1].strip()

result = {
    "prompt_file": str(prompt_file),
    "context_target": context_target,
    "predict_n": predict_n,
    "prompt_tokens": prompt_tokens,
    "prompt_tokens_per_sec": prompt_tps,
    "decode_tokens_per_sec": decode_tps,
    "prefill_ms": prefill_ms,
    "wall_ms": None if wall_s is None else round(wall_s * 1000),
    "kv_cache_mem_bytes": None if kv_total is None else kv_total * 1024 * 1024,
    "kv_cache_context_bytes": None if kv_context is None else kv_context * 1024 * 1024,
    "response_text": clean,
}
print(json.dumps(result, indent=2, sort_keys=True))
PY
  rm -f "$out_file"
}

cx_tq_help() {
  cat <<'EOF'
usage:
  turboquant_phase1.sh backend-ref
  turboquant_phase1.sh init-artifact <path>
  turboquant_phase1.sh probe (--model <path.gguf> | --hf-repo <repo[:quant]>)
  turboquant_phase1.sh token-count --model <path.gguf> --file <prompt.txt>
  turboquant_phase1.sh measure (--model <path.gguf> | --hf-repo <repo[:quant]>) --file <prompt.txt> --ctx <n> [--predict <n>]
EOF
}

main() {
  local sub="${1:-}"
  case "$sub" in
    backend-ref)
      shift
      cx_tq_backend_ref "$@"
      ;;
    init-artifact)
      shift
      cx_tq_init_artifact "$@"
      ;;
    probe)
      shift
      cx_tq_probe "$@"
      ;;
    token-count)
      shift
      cx_tq_token_count "$@"
      ;;
    measure)
      shift
      cx_tq_measure "$@"
      ;;
    ""|-h|--help|help)
      cx_tq_help
      ;;
    *)
      cx_tq_die "unknown subcommand: $sub"
      ;;
  esac
}

main "$@"
