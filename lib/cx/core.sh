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
  for fn in cx cxj cxo cxcopy cxdiffsum_staged cxcommitjson cxcommitmsg cxnext cxfix cxfix_run cxhealth cxpolicy cxprofile cxalert cxtrace cxbench cxworklog; do
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
