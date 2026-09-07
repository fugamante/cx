#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
model="${CX_LLAMA_CPP_SMOKE_MODEL:-ggml-org/Qwen3-0.6B-GGUF:Q4_0}"
args="${CX_LLAMA_CPP_SMOKE_ARGS:-}"
timeout_secs="${CX_LLAMA_CPP_SMOKE_TIMEOUT_SECS:-600}"
if [ -z "$args" ]; then
  args="-n 64 --temp 0 -c 2048 --simple-io"
fi

if ! command -v "${CX_LLAMA_CPP_BIN:-llama-cli}" >/dev/null 2>&1; then
  printf 'llamacpp smoke: missing %s on PATH\n' "${CX_LLAMA_CPP_BIN:-llama-cli}" >&2
  exit 127
fi

printf 'llamacpp smoke model: %s\n' "$model"
"$root/bin/xshelf" llm use llamacpp "$model"
CX_CMD_TIMEOUT_SECS="$timeout_secs" CX_LLAMA_CPP_ARGS="$args" \
  "$root/bin/xshelf" cxo printf 'xshelf llamacpp smoke\n'
