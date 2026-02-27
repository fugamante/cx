#!/usr/bin/env bash

# cx core utilities: logging, alerts, codex JSONL capture, diagnostics.
# Source this file from interactive bash shells.

if [[ -n "${CX_CORE_LOADED:-}" ]] && declare -F _cx_log_file >/dev/null 2>&1 && declare -F _codex_text >/dev/null 2>&1; then
  return 0
fi
CX_CORE_LOADED=1

# Logging paths (repo-aware; fallback to global outside git repos).
export CXLOG_DIR="${CXLOG_DIR:-$HOME/.codex/cxlogs}"
export CXLOG_FILE="${CXLOG_FILE:-$CXLOG_DIR/runs.jsonl}"
export CXLOG_ENABLED="${CXLOG_ENABLED:-1}"

# Alert thresholds.
export CXALERT_ENABLED="${CXALERT_ENABLED:-1}"
export CXALERT_MAX_MS="${CXALERT_MAX_MS:-8000}"
export CXALERT_MAX_EFF_IN="${CXALERT_MAX_EFF_IN:-5000}"
export CXALERT_MAX_OUT="${CXALERT_MAX_OUT:-1000}"
export CX_MODE="${CX_MODE:-lean}"
export CX_QUARANTINE_ENABLED="${CX_QUARANTINE_ENABLED:-1}"
export CX_SCHEMA_RELAXED="${CX_SCHEMA_RELAXED:-0}"

_cx_has_rtk() {
  command -v rtk >/dev/null 2>&1
}

if [[ -z "${CX_RTK_ENABLED+x}" ]]; then
  if _cx_has_rtk; then
    export CX_RTK_ENABLED=1
  else
    export CX_RTK_ENABLED=0
  fi
fi
if [[ -z "${CX_RTK_SYSTEM+x}" ]]; then
  if _cx_has_rtk; then
    export CX_RTK_SYSTEM=1
  else
    export CX_RTK_SYSTEM=0
  fi
fi
export CX_RTK_MODE="${CX_RTK_MODE:-condense}"
export CX_RTK_MAX_CHARS="${CX_RTK_MAX_CHARS:-}"
export CX_RTK_LAST_ERROR=0
export CX_CONTEXT_BUDGET_CHARS="${CX_CONTEXT_BUDGET_CHARS:-12000}"
export CX_CONTEXT_BUDGET_LINES="${CX_CONTEXT_BUDGET_LINES:-300}"
export CX_CONTEXT_CLIP_MODE="${CX_CONTEXT_CLIP_MODE:-smart}"
export CX_CONTEXT_CLIP_FOOTER="${CX_CONTEXT_CLIP_FOOTER:-1}"

_cx_mode_normalize() {
  case "${CX_MODE:-lean}" in
    lean|deterministic|verbose) printf "%s" "${CX_MODE:-lean}" ;;
    *) printf "%s" "lean" ;;
  esac
}

_cx_mode_prompt_prefix() {
  local mode
  mode="$(_cx_mode_normalize)"
  case "$mode" in
    lean)
      cat <<'EOF'
Mode: lean
Return concise output with minimal prose.
EOF
      ;;
    deterministic)
      cat <<'EOF'
Mode: deterministic
Return stable, format-locked output only; avoid extra prose.
EOF
      ;;
    verbose)
      cat <<'EOF'
Mode: verbose
Return richer explanation with clear sections and explicit assumptions.
EOF
      ;;
  esac
}

_cx_prompt_preprocess() {
  local input
  input="$(cat)"
  {
    _cx_mode_prompt_prefix
    echo
    printf "%s" "$input"
  }
}

_cx_json_escape() {
  jq -Rs .
}

_cx_git_root() {
  git rev-parse --show-toplevel 2>/dev/null || true
}

_cx_default_repo_dir() {
  local script_path script_dir repo_guess
  if [[ -n "${CX_REPO_DIR:-}" ]]; then
    printf "%s" "$CX_REPO_DIR"
    return 0
  fi
  if [[ -d "$HOME/cx" ]]; then
    printf "%s" "$HOME/cx"
    return 0
  fi
  script_path="${BASH_SOURCE[0]:-}"
  if [[ -n "$script_path" ]]; then
    script_dir="$(cd "$(dirname "$script_path")" 2>/dev/null && pwd || true)"
    repo_guess="$(cd "$script_dir/../.." 2>/dev/null && pwd || true)"
    if [[ -n "$repo_guess" ]] && [[ -f "$repo_guess/cx.sh" ]]; then
      printf "%s" "$repo_guess"
      return 0
    fi
  fi
  printf "%s" "$HOME/cx"
}

_cx_rust_manifest() {
  local repo_root git_root candidate
  repo_root="${CX_REPO_ROOT:-}"
  if [[ -z "$repo_root" ]]; then
    repo_root="$(_cx_default_repo_dir)"
  fi
  candidate="$repo_root/rust/cxrs/Cargo.toml"
  if [[ -f "$candidate" ]]; then
    printf "%s" "$candidate"
    return 0
  fi
  git_root="$(_cx_git_root)"
  if [[ -n "$git_root" ]] && [[ -f "$git_root/rust/cxrs/Cargo.toml" ]]; then
    printf "%s" "$git_root/rust/cxrs/Cargo.toml"
    return 0
  fi
  printf "%s" "$candidate"
}

_cx_rust_supports() {
  local cmd="$1"
  local manifest repo_root
  manifest="$(_cx_rust_manifest)"
  [[ -f "$manifest" ]] || return 1
  command -v cargo >/dev/null 2>&1 || return 1
  repo_root="$(cd "$(dirname "$manifest")/../.." && pwd)"
  CX_REPO_ROOT="$repo_root" cargo run --quiet --manifest-path "$manifest" -- supports "$cmd" >/dev/null 2>&1
}

_cx_rust_exec() {
  local cmd="$1"
  shift
  local manifest repo_root
  manifest="$(_cx_rust_manifest)"
  [[ -f "$manifest" ]] || return 127
  command -v cargo >/dev/null 2>&1 || return 127
  repo_root="$(cd "$(dirname "$manifest")/../.." && pwd)"
  CX_REPO_ROOT="$repo_root" cargo run --quiet --manifest-path "$manifest" -- "$cmd" "$@"
}

_cx_log_file() {
  local root
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then
    echo "$root/.codex/cxlogs/runs.jsonl"
  else
    echo "$HOME/.codex/cxlogs/runs.jsonl"
  fi
}

_cx_resolve_source_location() {
  local src="$1"
  local repo_dir cx_sh path
  repo_dir="$(_cx_default_repo_dir)"
  cx_sh="$repo_dir/cx.sh"
  if [[ -z "$src" || "$src" == "local-shell" ]]; then
    if [[ -f "$cx_sh" ]]; then
      printf "repo:%s" "$cx_sh"
    else
      printf "local-shell"
    fi
    return 0
  fi
  case "$src" in
    repo:*)
      path="${src#repo:}"
      if [[ -f "$path" ]]; then
        printf "%s" "$src"
      elif [[ -f "$cx_sh" ]]; then
        printf "repo:%s" "$cx_sh"
      else
        printf "%s" "$src"
      fi
      ;;
    *)
      printf "%s" "$src"
      ;;
  esac
}

cxversion() {
  local root sha ts src logf version_file version_text
  local mode rtk_enabled rtk_system budget_chars budget_lines clip_mode
  local backend model capture_provider rtk_available execution_path
  root="$(_cx_git_root)"
  if [[ -n "$root" ]] && command -v git >/dev/null 2>&1; then
    sha="$(git -C "$root" rev-parse --short HEAD 2>/dev/null || true)"
  else
    sha=""
  fi
  ts="$(date -u +"%Y-%m-%d")"
  src="$(_cx_resolve_source_location "${CX_SOURCE_LOCATION:-local-shell}")"
  logf="$(_cx_log_file)"
  version_file="$(_cx_default_repo_dir)/VERSION"
  if [[ -f "$version_file" ]]; then
    version_text="$(tr -d '\n' < "$version_file")"
  else
    version_text="$ts"
  fi
  if [[ -n "$sha" ]]; then
    version_text="${version_text}+${sha}"
  fi
  mode="$(_cx_mode_normalize)"
  execution_path="${CX_EXECUTION_PATH:-bash}"
  backend="${CX_LLM_BACKEND:-codex}"
  model=""
  if [[ "$backend" == "ollama" ]]; then
    model="${CX_OLLAMA_MODEL:-}"
  else
    model="${CX_MODEL:-}"
  fi
  [[ -n "$model" ]] || model="<unset>"
  capture_provider="${CX_CAPTURE_PROVIDER:-auto}"
  rtk_enabled="${CX_RTK_ENABLED:-0}"
  rtk_system="${CX_RTK_SYSTEM:-0}"
  if command -v rtk >/dev/null 2>&1; then
    rtk_available=1
  else
    rtk_available=0
  fi
  budget_chars="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  budget_lines="${CX_CONTEXT_BUDGET_LINES:-300}"
  clip_mode="${CX_CONTEXT_CLIP_MODE:-smart}"
  echo "version: ${version_text}"
  echo "execution_path: ${execution_path}"
  echo "source=${src}"
  echo "log_file: ${logf}"
  echo "mode: ${mode}"
  echo "backend_resolution: backend=${backend} model=${model}"
  echo "capture_provider: ${capture_provider}"
  echo "rtk_available: ${rtk_available}"
  echo "rtk_enabled: ${rtk_enabled}"
  echo "rtk_system: ${rtk_system}"
  echo "budget_chars: ${budget_chars}"
  echo "budget_lines: ${budget_lines}"
  echo "clip_mode: ${clip_mode}"
}

cxcore() {
  if _cx_rust_supports "core"; then
    _cx_rust_exec "core"
    return $?
  fi
  echo "cxcore: rust runtime unavailable" >&2
  return 1
}

cxwhere() {
  local repo_root bin_cx rust_bin rust_manifest rust_ver bash_lib cmd
  repo_root="${CX_REPO_ROOT:-$(_cx_git_root)}"
  [[ -n "$repo_root" ]] || repo_root="$(_cx_default_repo_dir)"
  bin_cx="$repo_root/bin/cx"
  rust_bin="$repo_root/rust/cxrs/bin/cxrs"
  rust_manifest="$repo_root/rust/cxrs/Cargo.toml"
  bash_lib="$repo_root/lib/cx.sh"
  if [[ -x "$rust_bin" ]]; then
    rust_ver="$(CX_REPO_ROOT="$repo_root" "$rust_bin" version 2>/dev/null | awk -F': ' '/^version:/ {print $2; exit}')"
  elif [[ -f "$rust_manifest" ]] && command -v cargo >/dev/null 2>&1; then
    rust_ver="$(CX_REPO_ROOT="$repo_root" cargo run --quiet --manifest-path "$rust_manifest" -- version 2>/dev/null | awk -F': ' '/^version:/ {print $2; exit}')"
  else
    rust_ver="<unavailable>"
  fi

  echo "bin_cx: $bin_cx"
  echo "cxrs: $rust_bin"
  echo "cxrs_version: ${rust_ver:-<unknown>}"
  echo "bash_lib: $bash_lib"

  if [[ "$#" -gt 0 ]]; then
    echo "command_resolution:"
    for cmd in "$@"; do
      if [[ -f "$rust_manifest" ]] && command -v cargo >/dev/null 2>&1 && CX_REPO_ROOT="$repo_root" cargo run --quiet --manifest-path "$rust_manifest" -- supports "$cmd" >/dev/null 2>&1; then
        echo "- $cmd: rust"
        continue
      fi
      if [[ -x "$rust_bin" ]] && CX_REPO_ROOT="$repo_root" "$rust_bin" supports "$cmd" >/dev/null 2>&1; then
        echo "- $cmd: rust"
        continue
      fi
      if declare -F "$cmd" >/dev/null 2>&1; then
        echo "- $cmd: bash-fallback"
        type -a "$cmd" 2>/dev/null | sed 's/^/  /'
      else
        echo "- $cmd: unsupported"
      fi
    done
    return 0
  fi

  local fn
  for fn in _codex_text _codex_last _cx_codex_json _cx_log_schema_failure cxo cxdiffsum_staged cxcommitjson cxnext cxfix_run; do
    type -a "$fn" 2>/dev/null || echo "$fn: not found" >&2
  done
}

cxdiag() {
  local backend model provider rtk_version mode budget_chars budget_lines clip_mode logf
  backend="${CX_LLM_BACKEND:-codex}"
  if [[ "$backend" == "ollama" ]]; then
    model="${CX_OLLAMA_MODEL:-<unset>}"
  else
    model="${CX_MODEL:-<unset>}"
  fi
  provider="${CX_CAPTURE_PROVIDER:-auto}"
  mode="${CX_MODE:-lean}"
  budget_chars="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  budget_lines="${CX_CONTEXT_BUDGET_LINES:-300}"
  clip_mode="${CX_CONTEXT_CLIP_MODE:-smart}"
  logf="$(_cx_log_file)"
  if command -v rtk >/dev/null 2>&1; then
    rtk_version="$(rtk --version 2>/dev/null | head -n 1)"
  else
    rtk_version="<unavailable>"
  fi
  echo "== cxdiag =="
  echo "backend: $backend"
  echo "active_model: $model"
  echo "capture_provider: $provider"
  echo "rtk_available: $([[ -n "$rtk_version" && "$rtk_version" != "<unavailable>" ]] && echo true || echo false)"
  echo "rtk_version: $rtk_version"
  echo "mode: $mode"
  echo "budget_chars: $budget_chars"
  echo "budget_lines: $budget_lines"
  echo "clip_mode: $clip_mode"
  echo "log_file: $logf"
  echo "schema_registry: present (embedded)"
  echo "routing_trace: sample='status' rust=false bash_fallback=false"
}

cxparity() {
  if [[ -n "${CX_REPO_ROOT:-}" && -f "${CX_REPO_ROOT}/rust/cxrs/Cargo.toml" ]] && command -v cargo >/dev/null 2>&1; then
    CX_REPO_ROOT="${CX_REPO_ROOT}" cargo run --quiet --manifest-path "${CX_REPO_ROOT}/rust/cxrs/Cargo.toml" -- parity
    return $?
  fi
  echo "cmd | rust | bash | json | logs | budget | result"
  echo "--- | --- | --- | --- | --- | --- | ---"
  echo "cxnext | false | true | false | false | true | FAIL"
  echo "cxparity: rust runtime unavailable for authoritative parity checks" >&2
  return 1
}

_cxlog_init() {
  local f dir
  f="$(_cx_log_file)"
  dir="$(dirname "$f")"
  mkdir -p "$dir" 2>/dev/null || true
  touch "$f" 2>/dev/null || true
  echo "$f"
}

_cx_schema_fail_log_file() {
  local root
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then
    echo "$root/.codex/cxlogs/schema_failures.jsonl"
  else
    echo "$HOME/.codex/cxlogs/schema_failures.jsonl"
  fi
}

_cx_schema_fail_log_init() {
  local f dir
  f="$(_cx_schema_fail_log_file)"
  dir="$(dirname "$f")"
  mkdir -p "$dir" 2>/dev/null || true
  touch "$f" 2>/dev/null || true
  echo "$f"
}

_cx_quarantine_dir() {
  local root
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then
    echo "$root/.codex/quarantine"
  else
    echo "$HOME/.codex/quarantine"
  fi
}

_cx_quarantine_init() {
  local d
  d="$(_cx_quarantine_dir)"
  mkdir -p "$d" 2>/dev/null || true
  echo "$d"
}

_cx_quarantine_store() {
  local tool="$1"
  local reason="$2"
  local raw="${3:-}"
  local schema="${4:-}"
  local prompt="${5:-}"
  local d id file

  if [[ "${CX_QUARANTINE_ENABLED:-1}" != "1" ]]; then
    printf "%s" ""
    return 0
  fi

  d="$(_cx_quarantine_init)"
  id="$(date -u +"%Y%m%dT%H%M%SZ")_$(printf "%s" "$tool" | tr -cs '[:alnum:]_-' '_')_$$"
  file="$d/${id}.json"
  jq -n \
    --arg id "$id" \
    --arg ts "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    --arg tool "$tool" \
    --arg reason "$reason" \
    --arg raw "$raw" \
    --arg schema "$schema" \
    --arg prompt "$prompt" \
    --arg prompt_sha "$(printf "%s" "$prompt" | shasum -a 256 | awk '{print $1}')" \
    --arg raw_sha "$(printf "%s" "$raw" | shasum -a 256 | awk '{print $1}')" \
    '{
      id: $id,
      ts: $ts,
      tool: $tool,
      reason: $reason,
      schema: $schema,
      prompt: $prompt,
      prompt_sha256: $prompt_sha,
      raw_response: $raw,
      raw_sha256: $raw_sha
    }' > "$file" 2>/dev/null || {
      rm -f "$file" 2>/dev/null || true
      printf "%s" ""
      return 1
    }
  printf "%s" "$id"
}

_cx_quarantine_file_by_id() {
  local id="$1"
  local d
  d="$(_cx_quarantine_init)"
  if [[ -f "$d/${id}.json" ]]; then
    echo "$d/${id}.json"
    return 0
  fi
  return 1
}

_cx_log_schema_failure() {
  local tool="$1"
  local reason="$2"
  local raw="${3:-}"
  local schema="${4:-}"
  local prompt="${5:-}"
  local f
  local qid=""
  local root scope runf
  f="$(_cx_schema_fail_log_init)"
  qid="$(_cx_quarantine_store "$tool" "$reason" "$raw" "$schema" "$prompt" 2>/dev/null || true)"
  {
    printf '{'
    printf '"ts":"%s",' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    printf '"tool":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
    printf '"reason":%s,' "$(printf "%s" "$reason" | _cx_json_escape)"
    printf '"quarantine_id":%s,' "$(printf "%s" "$qid" | _cx_json_escape)"
    printf '"raw_sha256":"%s"' "$(printf "%s" "$raw" | shasum -a 256 | awk '{print $1}')"
    printf '}\n'
  } >> "$f"
  runf="$(_cxlog_init)"
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then scope="repo"; else scope="global"; fi
  {
    printf '{'
    printf '"execution_id":%s,' "$(printf "%s" "$(date -u +"%Y%m%dT%H%M%SZ")_${tool}_$$" | _cx_json_escape)"
    printf '"command":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
    printf '"ts":"%s",' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    printf '"tool":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
    printf '"cwd":%s,' "$(pwd | tr -d '\n' | _cx_json_escape)"
    printf '"scope":"%s",' "$scope"
    printf '"repo_root":%s,' "$(printf "%s" "${root:-}" | _cx_json_escape)"
    printf '"backend_used":%s,' "$(printf "%s" "${CX_LLM_BACKEND:-codex}" | _cx_json_escape)"
    printf '"execution_mode":%s,' "$(printf "%s" "${CX_MODE:-lean}" | _cx_json_escape)"
    printf '"duration_ms":null,"input_tokens":null,"cached_input_tokens":null,"effective_input_tokens":null,"output_tokens":null,'
    printf '"prompt_len_raw":null,"prompt_len_processed":null,'
    printf '"system_output_len_raw":null,"system_output_len_processed":null,"system_output_len_clipped":null,'
    printf '"system_output_lines_raw":null,"system_output_lines_processed":null,"system_output_lines_clipped":null,'
    printf '"clipped":false,'
    printf '"budget_chars":%s,' "${CX_CONTEXT_BUDGET_CHARS:-12000}"
    printf '"budget_lines":%s,' "${CX_CONTEXT_BUDGET_LINES:-300}"
    printf '"clip_mode":%s,' "$(printf "%s" "${CX_CONTEXT_CLIP_MODE:-smart}" | _cx_json_escape)"
    printf '"clip_footer":%s,' "$([[ "${CX_CONTEXT_CLIP_FOOTER:-1}" == "1" ]] && echo "true" || echo "false")"
    printf '"rtk_used":false,'
    printf '"capture_provider":%s,' "$(printf "%s" "${CX_CAPTURE_PROVIDER:-auto}" | _cx_json_escape)"
    printf '"schema_enforced":true,'
    printf '"schema_valid":false,'
    printf '"schema_ok":false,'
    printf '"schema_reason":%s,' "$(printf "%s" "$reason" | _cx_json_escape)"
    printf '"quarantine_id":%s' "$(printf "%s" "$qid" | _cx_json_escape)"
    printf '}\n'
  } >> "$runf"
  printf "%s" "$qid"
}

_cx_state_file() {
  local root
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then
    echo "$root/.codex/state.json"
  else
    echo "$HOME/.codex/state.json"
  fi
}

_cx_state_init() {
  local f dir
  f="$(_cx_state_file)"
  dir="$(dirname "$f")"
  mkdir -p "$dir" 2>/dev/null || true
  if [[ ! -f "$f" || ! -s "$f" ]]; then
    cat > "$f" <<'EOF'
{
  "preferences": {
    "conventional_commits": true,
    "pr_summary_format": "standard"
  },
  "alert_overrides": {},
  "last_model": null
}
EOF
  fi
  if ! jq . "$f" >/dev/null 2>&1; then
    cat > "$f" <<'EOF'
{
  "preferences": {
    "conventional_commits": true,
    "pr_summary_format": "standard"
  },
  "alert_overrides": {},
  "last_model": null
}
EOF
  fi
  echo "$f"
}

_cx_state_set_path() {
  local key="$1"
  local value="$2"
  local f tmp parsed
  f="$(_cx_state_init)"
  tmp="$(mktemp)"
  parsed="$(jq -cn --arg v "$value" '$v | fromjson? // $v')"
  if ! jq --arg k "$key" --argjson v "$parsed" 'setpath($k|split("."); $v)' "$f" > "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  mv "$tmp" "$f"
}

_cx_state_get() {
  local key="$1"
  local f
  f="$(_cx_state_init)"
  jq -r --arg k "$key" 'getpath($k|split(".")) // empty' "$f"
}

cxstate() {
  local sub key value f
  sub="${1:-show}"
  f="$(_cx_state_init)"

  case "$sub" in
    show)
      jq . "$f"
      ;;
    get)
      key="${2:-}"
      if [[ -z "$key" ]]; then
        echo "Usage: cxstate get <key>" >&2
        return 2
      fi
      _cx_state_get "$key"
      ;;
    set)
      key="${2:-}"
      value="${3:-}"
      if [[ -z "$key" || $# -lt 3 ]]; then
        echo "Usage: cxstate set <key> <value>" >&2
        return 2
      fi
      if ! _cx_state_set_path "$key" "$value"; then
        echo "cxstate: failed to update state" >&2
        return 1
      fi
      jq . "$f"
      ;;
    *)
      echo "Usage: cxstate [show|get <key>|set <key> <value>]" >&2
      return 2
      ;;
  esac
}

cxlog_on()  { export CXLOG_ENABLED=1; echo "cx logging: ON -> $(_cx_log_file)"; }
cxlog_off() { export CXLOG_ENABLED=0; echo "cx logging: OFF"; }

cxalert_show() {
  echo "CXALERT_ENABLED=$CXALERT_ENABLED"
  echo "CXALERT_MAX_MS=$CXALERT_MAX_MS"
  echo "CXALERT_MAX_EFF_IN=$CXALERT_MAX_EFF_IN"
  echo "CXALERT_MAX_OUT=$CXALERT_MAX_OUT"
}

cxalert_on()  { export CXALERT_ENABLED=1; echo "cx alerts: ON"; }
cxalert_off() { export CXALERT_ENABLED=0; echo "cx alerts: OFF"; }

_cx_emit_alerts() {
  local tool="$1" dur="$2" eff="$3" out="$4" logf="$5"

  [[ "$CXALERT_ENABLED" == "1" ]] || return 0
  [[ -n "${dur:-}" && "$dur" != "null" ]] || return 0

  local warn=0 msgs=()

  if [[ -n "${CXALERT_MAX_MS:-}" && "$dur" != "null" ]] && (( dur > CXALERT_MAX_MS )); then
    warn=1; msgs+=("slow ${dur}ms>${CXALERT_MAX_MS}ms")
  fi

  if [[ -n "${eff:-}" && "$eff" != "null" ]] && (( eff > CXALERT_MAX_EFF_IN )); then
    warn=1; msgs+=("context ${eff}>${CXALERT_MAX_EFF_IN} eff_in")
  fi

  if [[ -n "${out:-}" && "$out" != "null" ]] && (( out > CXALERT_MAX_OUT )); then
    warn=1; msgs+=("output ${out}>${CXALERT_MAX_OUT} out")
  fi

  local summary="[cx] tool=${tool} dur=${dur}ms eff_in=${eff:-null} out=${out:-null}"
  if [[ "$warn" -eq 1 ]]; then
    printf "WARN %s: %s | log=%s\n" "$summary" "$(IFS="; "; echo "${msgs[*]}")" "$logf" >&2
  else
    printf "INFO %s | log=%s\n" "$summary" "$logf" >&2
  fi
}

# Input: prompt on stdin. Output: raw codex JSONL stream to stdout.
_cx_codex_jsonl_with_log() {
  local tool log_file
  tool="${1:-unknown}"
  log_file="$(_cxlog_init)"

  local prompt_raw prompt_processed
  prompt_raw="$(cat)"
  prompt_processed="$(printf "%s" "$prompt_raw" | _cx_prompt_preprocess)"

  local start_ms end_ms dur_ms
  start_ms="$(python3 - <<'PY'
import time
print(int(time.time()*1000))
PY
  )"

  local tmpjsonl
  tmpjsonl="$(mktemp)"

  printf "%s" "$prompt_processed" | codex exec --json - | tee "$tmpjsonl"

  end_ms="$(python3 - <<'PY'
import time
print(int(time.time()*1000))
PY
  )"
  dur_ms="$((end_ms - start_ms))"

  local usage_json in_tok cached_tok out_tok eff_in
  usage_json="$(jq -c 'select(.type=="turn.completed" and .usage) | .usage' "$tmpjsonl" | tail -n 1)"
  in_tok="$(printf "%s" "$usage_json" | jq -r '.input_tokens // empty' 2>/dev/null)"
  cached_tok="$(printf "%s" "$usage_json" | jq -r '.cached_input_tokens // empty' 2>/dev/null)"
  out_tok="$(printf "%s" "$usage_json" | jq -r '.output_tokens // empty' 2>/dev/null)"

  if [[ -n "${in_tok:-}" && -n "${cached_tok:-}" ]]; then
    eff_in="$((in_tok - cached_tok))"
  else
    eff_in="null"
  fi

  local prompt_hash prompt_preview root scope
  local prompt_hash_raw prompt_hash_processed prompt_len_raw prompt_len_processed rtk_error
  local schema_ok schema_reason quarantine_id
  local sys_len_raw sys_len_processed sys_len_clipped
  local sys_lines_raw sys_lines_processed sys_lines_clipped
  local clipped clip_mode clip_footer budget_chars budget_lines rtk_used
  prompt_hash_raw="$(printf "%s" "$prompt_raw" | shasum -a 256 | awk '{print $1}')"
  prompt_hash_processed="$(printf "%s" "$prompt_processed" | shasum -a 256 | awk '{print $1}')"
  prompt_len_raw="$(printf "%s" "$prompt_raw" | wc -c | tr -d ' ')"
  prompt_len_processed="$(printf "%s" "$prompt_processed" | wc -c | tr -d ' ')"
  rtk_error="${CX_RTK_LAST_ERROR:-0}"
  budget_chars="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  budget_lines="${CX_CONTEXT_BUDGET_LINES:-300}"
  clip_mode="${CX_CONTEXT_CLIP_MODE:-smart}"
  clip_footer="${CX_CONTEXT_CLIP_FOOTER:-1}"
  if [[ "${CX_SYSTEM_CAPTURE_SET:-0}" == "1" ]]; then
    sys_len_raw="${CX_SYSTEM_OUTPUT_LEN_RAW:-null}"
    sys_len_processed="${CX_SYSTEM_OUTPUT_LEN_PROCESSED:-null}"
    sys_len_clipped="${CX_SYSTEM_OUTPUT_LEN_CLIPPED:-$sys_len_processed}"
    sys_lines_raw="${CX_SYSTEM_OUTPUT_LINES_RAW:-null}"
    sys_lines_processed="${CX_SYSTEM_OUTPUT_LINES_PROCESSED:-null}"
    sys_lines_clipped="${CX_SYSTEM_OUTPUT_LINES_CLIPPED:-$sys_lines_processed}"
    clipped="$([[ "${CX_SYSTEM_CLIPPED:-0}" == "1" ]] && echo "true" || echo "false")"
    clip_mode="${CX_SYSTEM_CLIP_MODE_USED:-$clip_mode}"
    clip_footer="$([[ "${CX_SYSTEM_CLIP_FOOTER_USED:-$clip_footer}" == "1" ]] && echo "true" || echo "false")"
    if [[ "${CX_SYSTEM_RTK_USED:-0}" == "1" ]]; then
      rtk_used="true"
    else
      rtk_used="false"
    fi
  else
    sys_len_raw="null"
    sys_len_processed="null"
    sys_len_clipped="null"
    sys_lines_raw="null"
    sys_lines_processed="null"
    sys_lines_clipped="null"
    clipped="false"
    clip_footer="$([[ "$clip_footer" == "1" ]] && echo "true" || echo "false")"
    rtk_used="false"
  fi
  prompt_hash="$prompt_hash_processed"
  prompt_preview="$(printf "%s" "$prompt_processed" | tr '\n' ' ' | cut -c1-160)"
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then scope="repo"; else scope="global"; fi
  schema_ok="true"
  schema_reason=""
  quarantine_id=""

  if [[ "$CXLOG_ENABLED" == "1" ]]; then
    local execution_id backend_used execution_mode
    execution_id="$(date -u +"%Y%m%dT%H%M%SZ")_$(printf "%s" "$tool" | tr -cs '[:alnum:]_-' '_')_$$"
    backend_used="${CX_LLM_BACKEND:-codex}"
    execution_mode="${CX_MODE:-lean}"
    {
      printf '{'
      printf '"execution_id":%s,' "$(printf "%s" "$execution_id" | _cx_json_escape)"
      printf '"command":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
      printf '"ts":"%s",' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
      printf '"tool":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
      printf '"cwd":%s,'  "$(pwd | tr -d '\n' | _cx_json_escape)"
      printf '"scope":"%s",' "$scope"
      printf '"repo_root":%s,' "$(printf "%s" "${root:-}" | _cx_json_escape)"
      printf '"backend_used":%s,' "$(printf "%s" "$backend_used" | _cx_json_escape)"
      printf '"execution_mode":%s,' "$(printf "%s" "$execution_mode" | _cx_json_escape)"
      printf '"duration_ms":%s,' "$dur_ms"
      printf '"input_tokens":%s,' "${in_tok:-null}"
      printf '"cached_input_tokens":%s,' "${cached_tok:-null}"
      printf '"effective_input_tokens":%s,' "${eff_in}"
      printf '"output_tokens":%s,' "${out_tok:-null}"
      printf '"prompt_len_raw":%s,' "${prompt_len_raw:-0}"
      printf '"prompt_len_processed":%s,' "${prompt_len_processed:-0}"
      printf '"prompt_sha256_raw":"%s",' "$prompt_hash_raw"
      printf '"prompt_sha256_processed":"%s",' "$prompt_hash_processed"
      printf '"system_output_len_raw":%s,' "$sys_len_raw"
      printf '"system_output_len_processed":%s,' "$sys_len_processed"
      printf '"system_output_len_clipped":%s,' "$sys_len_clipped"
      printf '"system_output_lines_raw":%s,' "$sys_lines_raw"
      printf '"system_output_lines_processed":%s,' "$sys_lines_processed"
      printf '"system_output_lines_clipped":%s,' "$sys_lines_clipped"
      printf '"clipped":%s,' "$clipped"
      printf '"budget_chars":%s,' "$budget_chars"
      printf '"budget_lines":%s,' "$budget_lines"
      printf '"clip_mode":%s,' "$(printf "%s" "$clip_mode" | _cx_json_escape)"
      printf '"clip_footer":%s,' "$clip_footer"
      printf '"rtk_used":%s,' "$rtk_used"
      printf '"rtk_error":%s,' "$([[ "$rtk_error" == "1" ]] && echo "true" || echo "false")"
      printf '"capture_provider":%s,' "$(printf "%s" "${CX_CAPTURE_PROVIDER:-auto}" | _cx_json_escape)"
      printf '"schema_enforced":false,'
      printf '"schema_valid":%s,' "$schema_ok"
      printf '"schema_ok":%s,' "$schema_ok"
      printf '"schema_reason":%s,' "$(printf "%s" "$schema_reason" | _cx_json_escape)"
      printf '"quarantine_id":%s,' "$(printf "%s" "$quarantine_id" | _cx_json_escape)"
      printf '"prompt_sha256":"%s",' "$prompt_hash"
      printf '"prompt_preview":%s' "$(printf "%s" "$prompt_preview" | _cx_json_escape)"
      printf '}\n'
    } >> "$log_file"
  fi

  _cx_emit_alerts "$tool" "$dur_ms" "${eff_in:-null}" "${out_tok:-null}" "$log_file"
  unset CX_SYSTEM_CAPTURE_SET CX_SYSTEM_OUTPUT_LEN_RAW CX_SYSTEM_OUTPUT_LEN_PROCESSED CX_SYSTEM_OUTPUT_LEN_CLIPPED
  unset CX_SYSTEM_OUTPUT_LINES_RAW CX_SYSTEM_OUTPUT_LINES_PROCESSED CX_SYSTEM_OUTPUT_LINES_CLIPPED
  unset CX_SYSTEM_CLIPPED CX_SYSTEM_CLIP_MODE_USED CX_SYSTEM_CLIP_FOOTER_USED CX_SYSTEM_RTK_USED
  rm -f "$tmpjsonl" 2>/dev/null || true
}

_cx_extract_agent_message() {
  local mode="${1:-last}"
  jq -Rrs --arg mode "$mode" '
    split("\n")
    | map(fromjson? | select(.type=="item.completed" and .item.type=="agent_message") | (.item.text // ""))
    | if $mode == "all" then
        if length == 0 then "" else join("\n\n") end
      else
        (last // "")
      end
  '
}

_codex_last() {
  _cx_codex_jsonl_with_log "_codex_last" | _cx_extract_agent_message "last"
}

_codex_text() {
  _cx_codex_jsonl_with_log "_codex_text" | _cx_extract_agent_message "last"
}

cxmetrics() {
  local n f
  n="${1:-20}"
  f="$(_cxlog_init)"

  if [[ ! -s "$f" ]]; then
    echo "No logs yet: $f"
    return 0
  fi

  tail -n "$n" "$f" | jq -s '
    def nz: . // 0;
    {
      log_file: "'"$f"'",
      runs: length,
      avg_duration_ms: (map(.duration_ms|nz) | add / (length|nz)),
      avg_input_tokens: (map(.input_tokens|nz) | add / (length|nz)),
      avg_cached_input_tokens: (map(.cached_input_tokens|nz) | add / (length|nz)),
      avg_effective_input_tokens: (map(.effective_input_tokens|nz) | add / (length|nz)),
      avg_output_tokens: (map(.output_tokens|nz) | add / (length|nz)),
      by_tool: (
        group_by(.tool)
        | map({
            tool: .[0].tool,
            runs: length,
            avg_duration_ms: (map(.duration_ms|nz)|add/(length|nz)),
            avg_effective_input_tokens: (map(.effective_input_tokens|nz)|add/(length|nz)),
            avg_output_tokens: (map(.output_tokens|nz)|add/(length|nz))
          })
        | sort_by(-.runs)
      )
    }
  '
}

cxprofile() {
  local n f stats
  n="${1:-50}"
  f="$(_cxlog_init)"

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxprofile [positive_run_count]" >&2
    return 2
  fi

  if [[ ! -s "$f" ]]; then
    echo "== cxprofile (last $n runs) =="
    echo
    echo "Runs: 0"
    echo "Avg duration: 0ms"
    echo "Avg effective tokens: 0"
    echo "Cache hit rate: n/a"
    echo "Output/input ratio: n/a"
    echo "Slowest run: n/a"
    echo "Heaviest context: n/a"
    return 0
  fi

  stats="$(
    tail -n "$n" "$f" | jq -s '
      def nz: . // 0;
      def safe_div($a; $b): if ($b == 0) then null else ($a / $b) end;
      {
        runs: length,
        avg_duration_ms: (if length == 0 then 0 else (map(.duration_ms | nz) | add / length) end),
        avg_effective_input_tokens: (if length == 0 then 0 else (map(.effective_input_tokens | nz) | add / length) end),
        cache_hit_rate: (
          safe_div(
            (map(.cached_input_tokens | nz) | add);
            (map(.input_tokens | nz) | add)
          )
        ),
        output_input_ratio: (
          safe_div(
            (map(.output_tokens | nz) | add);
            (map(.effective_input_tokens | nz) | add)
          )
        ),
        slowest: (
          (map(select(.duration_ms != null)) | max_by(.duration_ms))
          // {duration_ms: null, tool: "n/a"}
          | {duration_ms: (.duration_ms // null), tool: (.tool // "unknown")}
        ),
        heaviest: (
          (map(select(.effective_input_tokens != null)) | max_by(.effective_input_tokens))
          // {effective_input_tokens: null, tool: "n/a"}
          | {effective_input_tokens: (.effective_input_tokens // null), tool: (.tool // "unknown")}
        )
      }
    '
  )"
  if [[ -z "$stats" ]]; then
    echo "cxprofile: failed to parse logs with jq" >&2
    return 1
  fi

  local runs avg_dur avg_eff cache_pct out_in_ratio slow_dur slow_tool heavy_eff heavy_tool
  runs="$(printf "%s" "$stats" | jq -r '.runs')"
  avg_dur="$(printf "%s" "$stats" | jq -r '(.avg_duration_ms | floor)')"
  avg_eff="$(printf "%s" "$stats" | jq -r '(.avg_effective_input_tokens | floor)')"
  cache_pct="$(printf "%s" "$stats" | jq -r 'if .cache_hit_rate == null then "n/a" else ((.cache_hit_rate * 100) | round | tostring + "%") end')"
  out_in_ratio="$(
    printf "%s" "$stats" | jq -r '.output_input_ratio' | awk '
      $0 == "null" || $0 == "" { print "n/a"; next }
      { printf "%.2f\n", $0 }
    '
  )"
  slow_dur="$(printf "%s" "$stats" | jq -r '.slowest.duration_ms')"
  slow_tool="$(printf "%s" "$stats" | jq -r '.slowest.tool')"
  heavy_eff="$(printf "%s" "$stats" | jq -r '.heaviest.effective_input_tokens')"
  heavy_tool="$(printf "%s" "$stats" | jq -r '.heaviest.tool')"

  echo "== cxprofile (last $n runs) =="
  echo
  echo "Runs: $runs"
  echo "Avg duration: ${avg_dur}ms"
  echo "Avg effective tokens: $avg_eff"
  echo "Cache hit rate: $cache_pct"
  echo "Output/input ratio: $out_in_ratio"
  if [[ "$slow_dur" == "null" ]]; then
    echo "Slowest run: n/a"
  else
    echo "Slowest run: ${slow_dur}ms (${slow_tool})"
  fi
  if [[ "$heavy_eff" == "null" ]]; then
    echo "Heaviest context: n/a"
  else
    echo "Heaviest context: ${heavy_eff} effective tokens (${heavy_tool})"
  fi
}

cxalert() {
  local n f stats
  local ms_thr eff_thr
  n="${1:-50}"
  f="$(_cxlog_init)"
  ms_thr="${CXALERT_MAX_MS:-8000}"
  eff_thr="${CXALERT_MAX_EFF_IN:-5000}"

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxalert [positive_run_count]" >&2
    return 2
  fi
  [[ "$ms_thr" =~ ^[0-9]+$ ]] || ms_thr=8000
  [[ "$eff_thr" =~ ^[0-9]+$ ]] || eff_thr=5000

  if [[ ! -s "$f" ]]; then
    echo "== cxalert (last $n runs) =="
    echo
    echo "Runs analyzed: 0"
    echo "Slow threshold violations (> ${ms_thr}ms): 0"
    echo "Effective token violations (> ${eff_thr}): 0"
    echo "Average cache hit rate: n/a"
    echo
    echo "Top 5 slowest runs:"
    echo "- n/a"
    echo
    echo "Top 5 heaviest runs:"
    echo "- n/a"
    echo
    echo "Top tools by effective token cost:"
    echo "- n/a"
    echo "log_file: $f"
    return 0
  fi

  stats="$(
    tail -n "$n" "$f" | jq -s --argjson ms_thr "$ms_thr" --argjson eff_thr "$eff_thr" '
      def nz: . // 0;
      def safe_div($a; $b): if ($b == 0) then null else ($a / $b) end;
      {
        runs: length,
        slow_violations: (
          map(select(.duration_ms != null and (.duration_ms > $ms_thr))) | length
        ),
        eff_violations: (
          map(select(.effective_input_tokens != null and (.effective_input_tokens > $eff_thr))) | length
        ),
        cache_hit_rate: (
          safe_div(
            (map(.cached_input_tokens | nz) | add);
            (map(.input_tokens | nz) | add)
          )
        ),
        slowest: (
          map(select(.duration_ms != null))
          | sort_by(.duration_ms)
          | reverse
          | .[0:5]
          | map({
              tool: (.tool // "unknown"),
              duration_ms: .duration_ms,
              ts: (.ts // "n/a")
            })
        ),
        heaviest: (
          map(select(.effective_input_tokens != null))
          | sort_by(.effective_input_tokens)
          | reverse
          | .[0:5]
          | map({
              tool: (.tool // "unknown"),
              effective_input_tokens: .effective_input_tokens,
              ts: (.ts // "n/a")
            })
        ),
        top_tools: (
          map(select(.tool != null))
          | group_by(.tool)
          | map({
              tool: .[0].tool,
              runs: length,
              total_effective_input_tokens: (map(.effective_input_tokens | nz) | add),
              avg_duration_ms: (if length == 0 then 0 else (map(.duration_ms | nz) | add / length | floor) end)
            })
          | sort_by(.total_effective_input_tokens)
          | reverse
          | .[0:5]
        )
      }
    '
  )"

  if [[ -z "$stats" ]]; then
    echo "cxalert: failed to parse logs with jq" >&2
    return 1
  fi

  local runs slow_count eff_count cache_rate
  local slow_lines heavy_lines tool_lines
  runs="$(printf "%s" "$stats" | jq -r '.runs')"
  slow_count="$(printf "%s" "$stats" | jq -r '.slow_violations')"
  eff_count="$(printf "%s" "$stats" | jq -r '.eff_violations')"
  cache_rate="$(printf "%s" "$stats" | jq -r 'if .cache_hit_rate == null then "n/a" else ((.cache_hit_rate * 100) | round | tostring + "%") end')"
  slow_lines="$(printf "%s" "$stats" | jq -r '.slowest[]? | "- \(.tool) | \(.duration_ms)ms | \(.ts)"')"
  heavy_lines="$(printf "%s" "$stats" | jq -r '.heaviest[]? | "- \(.tool) | \(.effective_input_tokens) effective | \(.ts)"')"
  tool_lines="$(printf "%s" "$stats" | jq -r '.top_tools[]? | "- \(.tool) | total_effective=\(.total_effective_input_tokens) | runs=\(.runs) | avg_duration=\(.avg_duration_ms)ms"')"

  echo "== cxalert (last $n runs) =="
  echo
  echo "Runs analyzed: $runs"
  echo "Slow threshold violations (> ${ms_thr}ms): $slow_count"
  echo "Effective token violations (> ${eff_thr}): $eff_count"
  echo "Average cache hit rate: $cache_rate"
  echo
  echo "Top 5 slowest runs:"
  if [[ -n "$slow_lines" ]]; then
    printf "%s\n" "$slow_lines"
  else
    echo "- n/a"
  fi
  echo
  echo "Top 5 heaviest runs:"
  if [[ -n "$heavy_lines" ]]; then
    printf "%s\n" "$heavy_lines"
  else
    echo "- n/a"
  fi
  echo
  echo "Top tools by effective token cost:"
  if [[ -n "$tool_lines" ]]; then
    printf "%s\n" "$tool_lines"
  else
    echo "- n/a"
  fi
  echo "log_file: $f"
}

cxtrace() {
  local n f total raw
  n="${1:-1}"
  f="$(_cxlog_init)"

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxtrace [positive_run_index_from_latest]" >&2
    return 2
  fi

  if [[ ! -s "$f" ]]; then
    echo "cxtrace: no logs yet"
    echo "log_file: $f"
    return 0
  fi

  total="$(wc -l < "$f" | tr -d ' ')"
  if (( n > total )); then
    echo "cxtrace: requested run #$n but only $total run(s) available"
    echo "log_file: $f"
    return 1
  fi

  raw="$(tail -n "$n" "$f" | head -n 1)"
  if [[ -z "$raw" ]]; then
    echo "cxtrace: could not read requested run"
    echo "log_file: $f"
    return 1
  fi

  if ! printf "%s\n" "$raw" | jq . >/dev/null 2>&1; then
    echo "cxtrace: selected log entry is not valid JSON"
    echo "log_file: $f"
    return 1
  fi

  local ts tool cwd duration_ms input_tokens cached_input_tokens effective_input_tokens output_tokens scope repo_root prompt_sha256 prompt_preview
  ts="$(printf "%s\n" "$raw" | jq -r '.ts // "n/a"')"
  tool="$(printf "%s\n" "$raw" | jq -r '.tool // "n/a"')"
  cwd="$(printf "%s\n" "$raw" | jq -r '.cwd // "n/a"')"
  duration_ms="$(printf "%s\n" "$raw" | jq -r 'if .duration_ms == null then "n/a" else (.duration_ms|tostring) end')"
  input_tokens="$(printf "%s\n" "$raw" | jq -r 'if .input_tokens == null then "n/a" else (.input_tokens|tostring) end')"
  cached_input_tokens="$(printf "%s\n" "$raw" | jq -r 'if .cached_input_tokens == null then "n/a" else (.cached_input_tokens|tostring) end')"
  effective_input_tokens="$(printf "%s\n" "$raw" | jq -r 'if .effective_input_tokens == null then "n/a" else (.effective_input_tokens|tostring) end')"
  output_tokens="$(printf "%s\n" "$raw" | jq -r 'if .output_tokens == null then "n/a" else (.output_tokens|tostring) end')"
  scope="$(printf "%s\n" "$raw" | jq -r '.scope // "n/a"')"
  repo_root="$(printf "%s\n" "$raw" | jq -r '.repo_root // "n/a"')"
  prompt_sha256="$(printf "%s\n" "$raw" | jq -r '.prompt_sha256 // "n/a"')"
  prompt_preview="$(printf "%s\n" "$raw" | jq -r '.prompt_preview // empty')"

  echo "== cxtrace (run #$n from latest) =="
  echo
  echo "ts: $ts"
  echo "tool: $tool"
  echo "cwd: $cwd"
  if [[ "$duration_ms" == "n/a" ]]; then
    echo "duration_ms: n/a"
  else
    echo "duration_ms: ${duration_ms}ms"
  fi
  echo "input_tokens: $input_tokens"
  echo "cached_input_tokens: $cached_input_tokens"
  echo "effective_input_tokens: $effective_input_tokens"
  echo "output_tokens: $output_tokens"
  echo "scope: $scope"
  echo "repo_root: $repo_root"
  echo "prompt_sha256: $prompt_sha256"
  if [[ -n "$prompt_preview" ]]; then
    echo "prompt_preview: $prompt_preview"
  fi
  echo "log_file: $f"
}

cxbench() {
  if [[ $# -lt 3 ]]; then
    echo "Usage: cxbench <runs> -- <command...>" >&2
    return 2
  fi

  local runs
  runs="$1"
  shift

  if [[ ! "$runs" =~ ^[0-9]+$ ]] || (( runs <= 0 )); then
    echo "Usage: cxbench <runs> -- <command...>" >&2
    return 2
  fi

  if [[ "${1:-}" != "--" ]]; then
    echo "Usage: cxbench <runs> -- <command...>" >&2
    return 2
  fi
  shift

  if [[ $# -lt 1 ]]; then
    echo "Usage: cxbench <runs> -- <command...>" >&2
    return 2
  fi

  local cmd_str logf bench_log prev_cxlog_enabled
  cmd_str="$*"
  logf="$(_cxlog_init)"
  bench_log="${CXBENCH_LOG:-1}"
  prev_cxlog_enabled="${CXLOG_ENABLED:-1}"

  if [[ "$bench_log" == "0" ]]; then
    export CXLOG_ENABLED=0
  fi

  local i status failures=0
  local sum_dur=0 min_dur=0 max_dur=0
  local sum_eff=0 sum_out=0 tok_count=0

  for ((i=1; i<=runs; i++)); do
    local before_lines after_lines delta_lines before_sha
    local start_ms end_ms runtime_ms
    local new_lines entry duration_ms eff_tok out_tok

    before_lines=0
    if [[ -f "$logf" ]]; then
      before_lines="$(wc -l < "$logf" | tr -d ' ')"
    fi
    before_sha=""
    if [[ "$before_lines" -gt 0 ]]; then
      before_sha="$(tail -n 1 "$logf" | jq -r '.prompt_sha256 // empty' 2>/dev/null)"
    fi

    start_ms="$(python3 - <<'PY'
import time
print(int(time.time()*1000))
PY
    )"
    bash -lc "$cmd_str" >/dev/null 2>&1
    status=$?
    end_ms="$(python3 - <<'PY'
import time
print(int(time.time()*1000))
PY
    )"
    runtime_ms="$((end_ms - start_ms))"
    duration_ms="$runtime_ms"
    eff_tok=""
    out_tok=""

    if [[ "$status" -ne 0 ]]; then
      failures=$((failures + 1))
    fi

    if [[ "$bench_log" != "0" && -f "$logf" ]]; then
      after_lines="$(wc -l < "$logf" | tr -d ' ')"
      if [[ -z "$after_lines" ]]; then after_lines=0; fi
      delta_lines="$((after_lines - before_lines))"
      if (( delta_lines > 0 )); then
        new_lines="$(tail -n "$delta_lines" "$logf")"
        entry="$(
          printf "%s\n" "$new_lines" | jq -c --arg before_sha "$before_sha" '
            [
              .[]
              | select(.duration_ms != null)
            ] as $all
            | (
                ($all | map(select((.prompt_sha256 // "") != "" and .prompt_sha256 != $before_sha)) | .[-1])
                // ($all | .[-1])
              )
          ' 2>/dev/null
        )"
        if [[ -n "$entry" && "$entry" != "null" ]]; then
          duration_ms="$(printf "%s" "$entry" | jq -r '.duration_ms // empty' 2>/dev/null)"
          eff_tok="$(printf "%s" "$entry" | jq -r '.effective_input_tokens // empty' 2>/dev/null)"
          out_tok="$(printf "%s" "$entry" | jq -r '.output_tokens // empty' 2>/dev/null)"
          if [[ -z "$duration_ms" ]]; then
            duration_ms="$runtime_ms"
          fi
        fi
      fi
    fi

    if [[ -z "$duration_ms" ]]; then
      duration_ms="$runtime_ms"
    fi

    if (( i == 1 )); then
      min_dur="$duration_ms"
      max_dur="$duration_ms"
    else
      if (( duration_ms < min_dur )); then min_dur="$duration_ms"; fi
      if (( duration_ms > max_dur )); then max_dur="$duration_ms"; fi
    fi
    sum_dur="$((sum_dur + duration_ms))"

    if [[ -n "${eff_tok:-}" && -n "${out_tok:-}" ]]; then
      sum_eff="$((sum_eff + eff_tok))"
      sum_out="$((sum_out + out_tok))"
      tok_count="$((tok_count + 1))"
    fi
  done

  export CXLOG_ENABLED="$prev_cxlog_enabled"

  local avg_dur avg_eff avg_out
  avg_dur="$((sum_dur / runs))"
  if (( tok_count > 0 )); then
    avg_eff="$((sum_eff / tok_count))"
    avg_out="$((sum_out / tok_count))"
  else
    avg_eff="n/a"
    avg_out="n/a"
  fi

  echo "== cxbench =="
  echo "Command: $cmd_str"
  echo "Runs: $runs"
  echo "Duration avg/min/max: ${avg_dur}ms / ${min_dur}ms / ${max_dur}ms"
  echo "Avg effective_input_tokens: $avg_eff"
  echo "Avg output_tokens: $avg_out"
  echo "Failures: $failures"
  if [[ "$bench_log" == "0" ]]; then
    echo "Logging: disabled via CXBENCH_LOG=0"
  else
    echo "Logging: enabled"
  fi
  echo "log_file: $logf"
}

cxworklog() {
  local n f stats
  n="${1:-50}"
  f="$(_cxlog_init)"

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxworklog [positive_run_count]" >&2
    return 2
  fi

  if [[ ! -s "$f" ]]; then
    echo "## cxworklog (last $n runs)"
    echo
    echo "- Runs analyzed: 0"
    echo
    echo "### By Tool"
    echo
    echo "| Tool | Runs | Avg Duration (ms) | Avg Effective Tokens |"
    echo "|---|---:|---:|---:|"
    echo "| n/a | 0 | 0 | 0 |"
    echo
    echo "### Chronological Runs"
    echo
    echo "- n/a"
    return 0
  fi

  stats="$(
    tail -n "$n" "$f" | jq -s '
      def nz: . // 0;
      {
        runs: length,
        by_tool: (
          map(select(.tool != null))
          | group_by(.tool)
          | map({
              tool: .[0].tool,
              run_count: length,
              avg_duration_ms: (if length == 0 then 0 else (map(.duration_ms | nz) | add / length | floor) end),
              avg_effective_tokens: (if length == 0 then 0 else (map(.effective_input_tokens | nz) | add / length | floor) end)
            })
          | sort_by(.tool)
        ),
        timeline: (
          map({
            ts: (.ts // "n/a"),
            tool: (.tool // "unknown"),
            duration_ms: (.duration_ms // null),
            effective_input_tokens: (.effective_input_tokens // null)
          })
        )
      }
    '
  )"

  if [[ -z "$stats" ]]; then
    echo "cxworklog: failed to parse logs with jq" >&2
    return 1
  fi

  local runs tool_rows timeline_rows
  runs="$(printf "%s" "$stats" | jq -r '.runs')"
  tool_rows="$(printf "%s" "$stats" | jq -r '.by_tool[]? | "| \(.tool) | \(.run_count) | \(.avg_duration_ms) | \(.avg_effective_tokens) |"')"
  timeline_rows="$(printf "%s" "$stats" | jq -r '.timeline[]? | "- `\(.ts)` `\(.tool)` duration=\((if .duration_ms==null then "n/a" else (.duration_ms|tostring + "ms") end)) eff=\((if .effective_input_tokens==null then "n/a" else (.effective_input_tokens|tostring) end))"')"

  echo "## cxworklog (last $n runs)"
  echo
  echo "- Runs analyzed: $runs"
  echo
  echo "### By Tool"
  echo
  echo "| Tool | Runs | Avg Duration (ms) | Avg Effective Tokens |"
  echo "|---|---:|---:|---:|"
  if [[ -n "$tool_rows" ]]; then
    printf "%s\n" "$tool_rows"
  else
    echo "| n/a | 0 | 0 | 0 |"
  fi
  echo
  echo "### Chronological Runs"
  echo
  if [[ -n "$timeline_rows" ]]; then
    printf "%s\n" "$timeline_rows"
  else
    echo "- n/a"
  fi
}

cxbudget() {
  local f last
  f="$(_cxlog_init)"
  echo "== cxbudget =="
  echo "CX_CONTEXT_BUDGET_CHARS=${CX_CONTEXT_BUDGET_CHARS:-12000}"
  echo "CX_CONTEXT_BUDGET_LINES=${CX_CONTEXT_BUDGET_LINES:-300}"
  echo "CX_CONTEXT_CLIP_MODE=${CX_CONTEXT_CLIP_MODE:-smart}"
  echo "CX_CONTEXT_CLIP_FOOTER=${CX_CONTEXT_CLIP_FOOTER:-1}"
  echo "log_file: $f"
  if [[ -s "$f" ]]; then
    last="$(tail -n 1 "$f")"
    echo
    echo "Last run clip fields:"
    printf "%s" "$last" | jq -r '
      "system_output_len_raw: \(.system_output_len_raw // "n/a")",
      "system_output_len_processed: \(.system_output_len_processed // "n/a")",
      "system_output_len_clipped: \(.system_output_len_clipped // "n/a")",
      "system_output_lines_raw: \(.system_output_lines_raw // "n/a")",
      "system_output_lines_processed: \(.system_output_lines_processed // "n/a")",
      "system_output_lines_clipped: \(.system_output_lines_clipped // "n/a")",
      "clipped: \(.clipped // false)",
      "budget_chars: \(.budget_chars // "n/a")",
      "budget_lines: \(.budget_lines // "n/a")",
      "clip_mode: \(.clip_mode // "n/a")",
      "clip_footer: \(.clip_footer // false)",
      "rtk_used: \(.rtk_used // false)"
    '
  fi
}

cxoptimize() {
  local n f sf stats
  local ms_thr eff_thr
  n="${1:-200}"
  f="$(_cxlog_init)"
  sf="$(_cx_schema_fail_log_init)"
  ms_thr="$(_cx_state_get "alert_overrides.CXALERT_MAX_MS" 2>/dev/null)"
  eff_thr="$(_cx_state_get "alert_overrides.CXALERT_MAX_EFF_IN" 2>/dev/null)"
  [[ -n "$ms_thr" ]] || ms_thr="${CXALERT_MAX_MS:-8000}"
  [[ -n "$eff_thr" ]] || eff_thr="${CXALERT_MAX_EFF_IN:-5000}"
  [[ "$ms_thr" =~ ^[0-9]+$ ]] || ms_thr=8000
  [[ "$eff_thr" =~ ^[0-9]+$ ]] || eff_thr=5000

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxoptimize [positive_run_count]" >&2
    return 2
  fi

  if [[ ! -s "$f" ]]; then
    echo "== cxoptimize (last $n runs) =="
    echo
    echo "Scoreboard:"
    echo "- Runs analyzed: 0"
    echo "- Alert-hit rate: n/a"
    echo "- Cache hit trend (first->second half): n/a"
    echo "- Schema failure rate: n/a"
    echo
    echo "Recommendations:"
    echo "- No run data available. Execute a few cx commands, then rerun cxoptimize."
    echo "log_file: $f"
    echo "schema_fail_log_file: $sf"
    return 0
  fi

  stats="$(
    tail -n "$n" "$f" | jq -s --argjson ms_thr "$ms_thr" --argjson eff_thr "$eff_thr" '
      def nz: . // 0;
      def safe_div($a; $b): if ($b == 0) then null else ($a / $b) end;
      . as $runs
      | (length) as $count
      | ($count / 2 | floor) as $half
      | {
          runs: $count,
          top_eff: (
            $runs
            | map(select(.tool != null))
            | group_by(.tool)
            | map({
                tool: .[0].tool,
                avg_effective_input_tokens: (if length == 0 then 0 else (map(.effective_input_tokens | nz) | add / length | floor) end),
                runs: length
              })
            | sort_by(.avg_effective_input_tokens)
            | reverse
            | .[0:5]
          ),
          top_dur: (
            $runs
            | map(select(.tool != null))
            | group_by(.tool)
            | map({
                tool: .[0].tool,
                avg_duration_ms: (if length == 0 then 0 else (map(.duration_ms | nz) | add / length | floor) end),
                runs: length
              })
            | sort_by(.avg_duration_ms)
            | reverse
            | .[0:5]
          ),
          cache_first: (
            if $half == 0 then null else safe_div(
              ($runs[0:$half] | map(.cached_input_tokens | nz) | add);
              ($runs[0:$half] | map(.input_tokens | nz) | add)
            ) end
          ),
          cache_second: (
            if ($count - $half) == 0 then null else safe_div(
              ($runs[$half:] | map(.cached_input_tokens | nz) | add);
              ($runs[$half:] | map(.input_tokens | nz) | add)
            ) end
          ),
          alert_hits: (
            $runs
            | map(select((.duration_ms != null and .duration_ms > $ms_thr) or (.effective_input_tokens != null and .effective_input_tokens > $eff_thr)))
            | length
          ),
          rtk_used_hits: (
            $runs
            | map(select(.rtk_used == true))
            | length
          ),
          schema_failures: 0
        }
    '
  )"

  if [[ -z "$stats" ]]; then
    echo "cxoptimize: failed to parse logs with jq" >&2
    return 1
  fi

  local runs alert_hits schema_failures rtk_used_hits
  local cache_first cache_second cache_trend
  local alert_rate schema_rate rtk_used_rate
  local top_eff_lines top_dur_lines
  runs="$(printf "%s" "$stats" | jq -r '.runs')"
  alert_hits="$(printf "%s" "$stats" | jq -r '.alert_hits')"
  rtk_used_hits="$(printf "%s" "$stats" | jq -r '.rtk_used_hits')"
  schema_failures=0
  if [[ -s "$sf" ]]; then
    schema_failures="$(tail -n "$n" "$sf" | wc -l | tr -d ' ')"
  fi
  cache_first="$(printf "%s" "$stats" | jq -r '.cache_first')"
  cache_second="$(printf "%s" "$stats" | jq -r '.cache_second')"
  alert_rate="$(printf "%s" "$stats" | jq -r 'if .runs == 0 then "n/a" else (((.alert_hits / .runs) * 100) | round | tostring + "%") end')"
  if [[ "$runs" =~ ^[0-9]+$ ]] && (( runs > 0 )); then
    schema_rate="$(awk -v f="$schema_failures" -v r="$runs" 'BEGIN{printf "%d%%", ((f/r)*100)+0.5}')"
    rtk_used_rate="$(awk -v f="$rtk_used_hits" -v r="$runs" 'BEGIN{printf "%d%%", ((f/r)*100)+0.5}')"
  else
    schema_rate="n/a"
    rtk_used_rate="n/a"
  fi
  cache_trend="$(printf "%s" "$stats" | jq -r '
    if .cache_first == null or .cache_second == null then "n/a"
    else
      ((.cache_first * 100) | round | tostring) + "% -> " +
      ((.cache_second * 100) | round | tostring) + "%"
    end
  ')"
  top_eff_lines="$(printf "%s" "$stats" | jq -r '.top_eff[]? | "- \(.tool): avg_eff=\(.avg_effective_input_tokens), runs=\(.runs)"')"
  top_dur_lines="$(printf "%s" "$stats" | jq -r '.top_dur[]? | "- \(.tool): avg_duration=\(.avg_duration_ms)ms, runs=\(.runs)"')"

  echo "== cxoptimize (last $n runs) =="
  echo
  echo "Scoreboard:"
  echo "- Runs analyzed: $runs"
  echo "- Alert-hit rate: $alert_rate (thresholds: dur>${ms_thr}ms, eff>${eff_thr})"
  echo "- RTK system-routing rate: $rtk_used_rate"
  echo "- Cache hit trend (first->second half): $cache_trend"
  echo "- Schema failure rate: $schema_rate"
  echo
  echo "Top tools by avg effective_input_tokens:"
  if [[ -n "$top_eff_lines" ]]; then
    printf "%s\n" "$top_eff_lines"
  else
    echo "- n/a"
  fi
  echo
  echo "Top tools by avg duration_ms:"
  if [[ -n "$top_dur_lines" ]]; then
    printf "%s\n" "$top_dur_lines"
  else
    echo "- n/a"
  fi
  echo
  echo "Recommendations:"
  if [[ -n "$top_eff_lines" ]]; then
    echo "- High effective-input tools above should adopt lean mode: trim prompt context and prefer schema-only outputs."
  fi
  if [[ -n "$top_dur_lines" ]]; then
    echo "- High latency tools above should split heavy workflows into smaller fast micro-queries and reduce repo-wide context."
  fi
  if [[ "$cache_first" != "null" && "$cache_second" != "null" ]]; then
    if awk "BEGIN{exit !($cache_second < $cache_first)}"; then
      echo "- Cache hit rate dropped between window halves; prompt drift likely. Compare prompt_preview hashes and stabilize templates."
    fi
  fi
  if [[ "$schema_failures" =~ ^[0-9]+$ ]] && (( schema_failures > 0 )); then
    echo "- Schema failures detected; tighten schemas and reduce optional ambiguity in structured prompts."
  fi
  if [[ "$alert_hits" =~ ^[0-9]+$ ]] && (( alert_hits > 0 )); then
    echo "- Frequent threshold alerts detected; raise only with evidence, otherwise optimize prompt size and command granularity."
  fi
  echo "log_file: $f"
  echo "schema_fail_log_file: $sf"
}

cxlog_tail() {
  local n f
  n="${1:-10}"
  f="$(_cxlog_init)"
  tail -n "$n" "$f" | jq .
}

cxhealth() {
  echo "== codex version =="
  codex --version
  echo
  echo "== codex json =="
  echo "ping" | codex exec --json - | tail -n 4
  echo
  echo "== _codex_text =="
  echo "2+2? (just answer)" | _codex_text
  echo
  echo "== cxo test =="
  cxo git status
  echo
  echo "All systems operational."
}

cxdoctor() (
  set -u

  echo
  echo "== binaries =="
  command -v codex >/dev/null 2>&1 || { echo "FAIL: codex not found in PATH"; return 1; }
  command -v jq >/dev/null 2>&1 || { echo "FAIL: jq not found (brew install jq)"; return 1; }
  command -v rtk >/dev/null 2>&1 || { echo "FAIL: rtk not found (brew install rtk)"; return 1; }

  echo "codex: $(command -v codex)"
  echo "jq:    $(command -v jq)"
  echo "rtk:   $(command -v rtk)"
  codex --version || { echo "FAIL: codex --version"; return 1; }

  echo
  echo "== codex config =="
  if [[ -f "$HOME/.codex/config.toml" ]]; then
    echo "config: $HOME/.codex/config.toml"
  else
    echo "WARN: missing ~/.codex/config.toml (Codex will use defaults)"
  fi

  echo
  echo "== codex json pipeline =="
  local agent_count reasoning_count last_text
  agent_count="$(
    echo "ping" | codex exec --json - \
      | jq -r 'select(.type=="item.completed") | .item.type' \
      | awk '$1=="agent_message"{c++} END{print c+0}'
  )" || { echo "FAIL: could not parse codex JSONL"; return 1; }

  reasoning_count="$(
    echo "ping" | codex exec --json - \
      | jq -r 'select(.type=="item.completed") | .item.type' \
      | awk '$1=="reasoning"{c++} END{print c+0}'
  )" || { echo "FAIL: could not parse codex JSONL (reasoning)"; return 1; }

  echo "agent_message events: $agent_count"
  echo "reasoning events:     $reasoning_count"
  [[ "$agent_count" -ge 1 ]] || { echo "FAIL: expected >=1 agent_message event"; return 1; }

  echo
  echo "== _codex_text =="
  if ! type _codex_text >/dev/null 2>&1; then
    echo "FAIL: _codex_text not defined"
    return 1
  fi
  last_text="$(echo "2+2? (just the number)" | _codex_text 2>/dev/null | tr -d '\r')"
  echo "_codex_text output: $last_text"
  [[ "$last_text" == "4" ]] || echo "WARN: _codex_text returned '$last_text' (expected '4')"

  echo
  echo "== git context (optional) =="
  if command -v git >/dev/null 2>&1; then
    if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      echo "in git repo: yes"
      echo "branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
    else
      echo "in git repo: no (skip git-based checks)"
    fi
  else
    echo "WARN: git not found"
  fi

  echo
  echo "== functions present =="
  local fn missing=0
  for fn in cx cxj cxo cxcopy cxtask cxdiffsum_staged cxcommitjson cxcommitmsg cxnext cxfix cxfix_run cxhealth cxversion cxcore cxwhere cxstate cxpolicy cxprofile cxalert cxtrace cxbench cxworklog cxbudget cxoptimize cxprompt cxroles cxfanout cxpromptlint cxreplay; do
    if type "$fn" >/dev/null 2>&1; then
      echo "OK: $fn"
    else
      echo "MISSING: $fn"
      missing=1
    fi
  done

  [[ "$missing" -eq 0 ]] || echo "WARN: some functions missing (see above)"

  echo
  echo "PASS: core pipeline looks healthy."
)

# Keep interactive shell resilient against leaked nounset from hooks.
set +u
