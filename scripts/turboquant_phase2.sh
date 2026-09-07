#!/usr/bin/env bash

set -euo pipefail

cx_tq2_die() {
  printf 'turboquant_phase2: %s\n' "$*" >&2
  exit 1
}

cx_tq2_fetch() {
  local out_dir="${1:-/tmp/cx_llama_cpp}"
  local repo="${2:-https://github.com/ggerganov/llama.cpp}"
  if [[ -d "$out_dir/.git" ]]; then
    git -C "$out_dir" fetch --depth 1 origin HEAD
    git -C "$out_dir" reset --hard FETCH_HEAD
  else
    rm -rf "$out_dir"
    git clone --depth 1 "$repo" "$out_dir"
  fi
  git -C "$out_dir" rev-parse --short HEAD
}

cx_tq2_touch() {
  local root="${1:-/tmp/cx_llama_cpp}"
  [[ -d "$root" ]] || cx_tq2_die "backend path not found: $root"

  cat <<EOF
backend_root=$root
backend_ref=$(git -C "$root" rev-parse --short HEAD 2>/dev/null || printf 'unknown')
touchpoint=kv_ctor file=src/llama-kv-cache.cpp line=79
touchpoint=v_read file=src/llama-kv-cache.cpp line=1152
touchpoint=v_write file=src/llama-kv-cache.cpp line=1219
touchpoint=v_idxs_build file=src/llama-kv-cache.cpp line=1285
touchpoint=v_idxs_set file=src/llama-kv-cache.cpp line=1358
touchpoint=v_rot_build file=src/llama-kv-cache.cpp line=1322
touchpoint=v_rot_set file=src/llama-kv-cache.cpp line=1669
touchpoint=graph_store_read file=src/llama-graph.cpp line=2106
touchpoint=model_wiring file=src/llama-model.cpp line=8385
touchpoint=cli_cache_type_v file=common/arg.cpp line=2010
EOF
}

cx_tq2_init_proto() {
  local out="${1:-}"
  [[ -n "$out" ]] || cx_tq2_die "init-proto requires an output path"
  cat >"$out" <<'EOF'
{
  "contract_version": "turboquant-proto.v1",
  "phase": "phase2",
  "backend": {
    "name": "llama.cpp",
    "target_ref": null
  },
  "prototype": {
    "name": "tq_v0",
    "scope": "v_only",
    "read_mode": "dequant_on_read",
    "group_size": 64,
    "codebook_bits": 8,
    "scale_type": "fp16",
    "payload_type": "u8",
    "rotation": {
      "enabled": false,
      "hook": "attn_inp_v_rot"
    }
  }
}

cx_tq2_build_check() {
  local root="${1:-/tmp/cx_llama_cpp}"
  local build_dir="${2:-$root/build-cx-tq}"
  [[ -d "$root" ]] || cx_tq2_die "backend path not found: $root"
  cmake -S "$root" -B "$build_dir" -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TESTS=OFF
  cmake --build "$build_dir" -j2
}

cx_tq2_export_patch() {
  local root="${1:-/tmp/cx_llama_cpp}"
  local out="${2:-}"
  [[ -d "$root/.git" ]] || cx_tq2_die "backend git checkout not found: $root"
  [[ -n "$out" ]] || cx_tq2_die "export-patch requires an output path"
  mkdir -p "$(dirname "$out")"
  git -C "$root" diff >"$out"
}
EOF
}

cx_tq2_help() {
  cat <<'EOF'
usage:
  turboquant_phase2.sh fetch [out_dir] [repo_url]
  turboquant_phase2.sh touch [backend_root]
  turboquant_phase2.sh init-proto <path>
  turboquant_phase2.sh build-check [backend_root] [build_dir]
  turboquant_phase2.sh export-patch [backend_root] <out_path>
  turboquant_phase2.sh work
EOF
}

cx_tq2_work() {
  cat <<'EOF'
p2_state src/llama-kv-cache.h add turboquant config and sidecar V state
p2_args common/arg.cpp add explicit turboquant runtime args
p2_model src/llama-model.cpp plumb turboquant config into kv cache construction
p2_write_gate src/llama-kv-cache.cpp:1219 add enable/disable and fallback gate
p2_write_codec src/llama-kv-cache.cpp:1219 store tq_v0 compressed V payload
p2_read_gate src/llama-kv-cache.cpp:1152 add read fallback gate
p2_read_codec src/llama-kv-cache.cpp:1152 dequantize tq_v0 payload on read
p2_mem src/llama-kv-cache.cpp report compressed V memory footprint
EOF
}

main() {
  local sub="${1:-}"
  case "$sub" in
    fetch)
      shift
      cx_tq2_fetch "$@"
      ;;
    touch)
      shift
      cx_tq2_touch "$@"
      ;;
    init-proto)
      shift
      cx_tq2_init_proto "$@"
      ;;
    build-check)
      shift
      cx_tq2_build_check "$@"
      ;;
    export-patch)
      shift
      cx_tq2_export_patch "$@"
      ;;
    work)
      shift
      cx_tq2_work "$@"
      ;;
    ""|-h|--help|help)
      cx_tq2_help
      ;;
    *)
      cx_tq2_die "unknown subcommand: $sub"
      ;;
  esac
}

main "$@"
