#!/usr/bin/env bash

# cx core utilities: logging, alerts, codex JSONL capture, diagnostics.
# Source this file from interactive bash shells.

if [[ -n "${CX_CORE_LOADED:-}" ]]; then
  return 0
fi
export CX_CORE_LOADED=1

# Logging paths (repo-aware; fallback to global outside git repos).
export CXLOG_DIR="${CXLOG_DIR:-$HOME/.codex/cxlogs}"
export CXLOG_FILE="${CXLOG_FILE:-$CXLOG_DIR/runs.jsonl}"
export CXLOG_ENABLED="${CXLOG_ENABLED:-1}"

# Alert thresholds.
export CXALERT_ENABLED="${CXALERT_ENABLED:-1}"
export CXALERT_MAX_MS="${CXALERT_MAX_MS:-8000}"
export CXALERT_MAX_EFF_IN="${CXALERT_MAX_EFF_IN:-5000}"
export CXALERT_MAX_OUT="${CXALERT_MAX_OUT:-1000}"

_cx_json_escape() {
  jq -Rs .
}

_cx_git_root() {
  git rev-parse --show-toplevel 2>/dev/null || true
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

_cxlog_init() {
  local f dir
  f="$(_cx_log_file)"
  dir="$(dirname "$f")"
  mkdir -p "$dir" 2>/dev/null || true
  touch "$f" 2>/dev/null || true
  echo "$f"
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

  local prompt
  prompt="$(cat)"

  local start_ms end_ms dur_ms
  start_ms="$(python3 - <<'PY'
import time
print(int(time.time()*1000))
PY
  )"

  local tmpjsonl
  tmpjsonl="$(mktemp)"

  printf "%s" "$prompt" | codex exec --json - | tee "$tmpjsonl"

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
  prompt_hash="$(printf "%s" "$prompt" | shasum -a 256 | awk '{print $1}')"
  prompt_preview="$(printf "%s" "$prompt" | tr '\n' ' ' | cut -c1-160)"
  root="$(_cx_git_root)"
  if [[ -n "$root" ]]; then scope="repo"; else scope="global"; fi

  if [[ "$CXLOG_ENABLED" == "1" ]]; then
    {
      printf '{'
      printf '"ts":"%s",' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
      printf '"tool":%s,' "$(printf "%s" "$tool" | _cx_json_escape)"
      printf '"cwd":%s,'  "$(pwd | tr -d '\n' | _cx_json_escape)"
      printf '"scope":"%s",' "$scope"
      printf '"repo_root":%s,' "$(printf "%s" "${root:-}" | _cx_json_escape)"
      printf '"duration_ms":%s,' "$dur_ms"
      printf '"input_tokens":%s,' "${in_tok:-null}"
      printf '"cached_input_tokens":%s,' "${cached_tok:-null}"
      printf '"effective_input_tokens":%s,' "${eff_in}"
      printf '"output_tokens":%s,' "${out_tok:-null}"
      printf '"prompt_sha256":"%s",' "$prompt_hash"
      printf '"prompt_preview":%s' "$(printf "%s" "$prompt_preview" | _cx_json_escape)"
      printf '}\n'
    } >> "$log_file"
  fi

  _cx_emit_alerts "$tool" "$dur_ms" "${eff_in:-null}" "${out_tok:-null}" "$log_file"
  rm -f "$tmpjsonl" 2>/dev/null || true
}

_codex_text() {
  _cx_codex_jsonl_with_log "_codex_text" \
    | jq -Rr 'fromjson? | select(.type=="item.completed" and .item.type=="agent_message") | .item.text' \
    | tail -n 1
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
  for fn in cx cxj cxo cxcopy cxdiffsum_staged cxcommitjson cxcommitmsg cxnext cxfix cxfix_run cxhealth; do
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
