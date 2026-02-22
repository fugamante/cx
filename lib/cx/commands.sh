#!/usr/bin/env bash

# cx command helpers built on top of core.sh

if [[ -n "${CX_COMMANDS_LOADED:-}" ]] && declare -F cxo >/dev/null 2>&1 && declare -F cxcommitjson >/dev/null 2>&1; then
  return 0
fi
CX_COMMANDS_LOADED=1

_cx_prompt_test_checklist() {
  local mode="$1"
  case "$mode" in
    implement)
      cat <<'EOF'
- Run syntax checks for touched files.
- Run targeted tests for changed behavior.
- Verify no regressions in related command paths.
EOF
      ;;
    fix)
      cat <<'EOF'
- Reproduce the failure path.
- Validate the fix with the failing case.
- Run adjacent smoke checks to avoid regression.
EOF
      ;;
    test)
      cat <<'EOF'
- Add/adjust focused tests for requested behavior.
- Run full relevant test suite segment.
- Confirm deterministic pass/fail output.
EOF
      ;;
    doc)
      cat <<'EOF'
- Verify examples run as written.
- Confirm commands/paths reflect current repo state.
- Check formatting and section completeness.
EOF
      ;;
    ops)
      cat <<'EOF'
- Validate command safety and idempotency.
- Verify logs/metrics/alerts reflect expected changes.
- Confirm non-interactive behavior for automation paths.
EOF
      ;;
    *)
      cat <<'EOF'
- Run syntax checks for touched files.
- Validate changed behavior.
- Confirm no regressions in adjacent flows.
EOF
      ;;
  esac
}

_cx_role_header() {
  local role="$1"
  case "$role" in
    architect)
      cat <<'EOF'
Role: Architect
Focus:
- Define minimal, robust design and interfaces.
- Identify risks/tradeoffs before implementation.
- Keep plan implementation-ready and testable.
EOF
      ;;
    implementer)
      cat <<'EOF'
Role: Implementer
Focus:
- Deliver concrete code changes with minimal surface area.
- Preserve existing behavior unless explicitly changed.
- Keep edits deterministic and operationally safe.
EOF
      ;;
    reviewer)
      cat <<'EOF'
Role: Reviewer
Focus:
- Find correctness, safety, and regression risks.
- Verify schema contracts and edge-case handling.
- Call out missing tests or brittle assumptions.
EOF
      ;;
    tester)
      cat <<'EOF'
Role: Tester
Focus:
- Build focused validation for behavior and regressions.
- Stress critical paths and failure modes.
- Report concise, reproducible results.
EOF
      ;;
    doc)
      cat <<'EOF'
Role: Doc
Focus:
- Produce precise, actionable documentation updates.
- Keep examples executable and aligned with code.
- Highlight behavior changes and migration notes.
EOF
      ;;
    *)
      return 1
      ;;
  esac
}

_cx_mode_render_hint() {
  local mode
  mode="$(_cx_mode_normalize)"
  case "$mode" in
    lean) echo "Keep output compact and direct." ;;
    deterministic) echo "Return deterministic, format-locked output only." ;;
    verbose) echo "Provide expanded detail and rationale." ;;
    *) echo "Keep output compact and direct." ;;
  esac
}

_cx_clip_text() {
  local out_var=""
  if [[ "${1:-}" == "--var" ]]; then
    out_var="${2:-}"
    shift 2
  fi
  local input budget_chars budget_lines mode mode_used footer_enabled clipped
  local orig_chars orig_lines kept_chars kept_lines
  input="$(cat)"
  budget_chars="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  budget_lines="${CX_CONTEXT_BUDGET_LINES:-300}"
  mode="${CX_CONTEXT_CLIP_MODE:-smart}"
  footer_enabled="${CX_CONTEXT_CLIP_FOOTER:-1}"
  [[ "$budget_chars" =~ ^[0-9]+$ ]] || budget_chars=12000
  [[ "$budget_lines" =~ ^[0-9]+$ ]] || budget_lines=300
  [[ "$mode" =~ ^(smart|head|tail)$ ]] || mode="smart"

  orig_chars="$(printf "%s" "$input" | wc -c | tr -d ' ')"
  orig_lines="$(printf "%s" "$input" | wc -l | tr -d ' ')"

  if [[ "$mode" == "smart" ]]; then
    if printf "%s" "$input" | grep -Eiq 'error|fail|warning'; then
      mode_used="tail"
    else
      mode_used="head"
    fi
  else
    mode_used="$mode"
  fi

  local clipped_text
  clipped_text="$input"
  if (( orig_lines > budget_lines )); then
    if [[ "$mode_used" == "tail" ]]; then
      clipped_text="$(printf "%s" "$clipped_text" | tail -n "$budget_lines")"
    else
      clipped_text="$(printf "%s" "$clipped_text" | head -n "$budget_lines")"
    fi
  fi

  kept_chars="$(printf "%s" "$clipped_text" | wc -c | tr -d ' ')"
  if (( kept_chars > budget_chars )); then
    if [[ "$mode_used" == "tail" ]]; then
      clipped_text="$(printf "%s" "$clipped_text" | tail -c "$budget_chars")"
    else
      clipped_text="$(printf "%s" "$clipped_text" | cut -c1-"$budget_chars")"
    fi
  fi

  kept_chars="$(printf "%s" "$clipped_text" | wc -c | tr -d ' ')"
  kept_lines="$(printf "%s" "$clipped_text" | wc -l | tr -d ' ')"
  if (( kept_chars < orig_chars || kept_lines < orig_lines )); then
    clipped=1
  else
    clipped=0
  fi

  export CX_CLIP_ORIGINAL_CHARS="$orig_chars"
  export CX_CLIP_ORIGINAL_LINES="$orig_lines"
  export CX_CLIP_KEPT_CHARS="$kept_chars"
  export CX_CLIP_KEPT_LINES="$kept_lines"
  export CX_CLIP_CLIPPED="$clipped"
  export CX_CLIP_MODE_USED="$mode_used"
  export CX_CLIP_FOOTER_USED="$footer_enabled"

  local final_out
  final_out="$clipped_text"
  if [[ "$clipped" == "1" && "$footer_enabled" == "1" ]]; then
    final_out="${final_out}"$'\n'"[cx] output clipped: original=${orig_chars}/${orig_lines}, kept=${kept_chars}/${kept_lines}, mode=${mode_used}"
  fi

  if [[ -n "$out_var" ]]; then
    printf -v "$out_var" "%s" "$final_out"
  else
    printf "%s" "$final_out"
  fi
}

_cx_chunk_text() {
  local input chunk_size len total i start
  input="$(cat)"
  chunk_size="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  [[ "$chunk_size" =~ ^[0-9]+$ ]] || chunk_size=12000
  if (( chunk_size <= 0 )); then
    printf "%s" "$input"
    return 0
  fi

  len="$(printf "%s" "$input" | wc -c | tr -d ' ')"
  if (( len == 0 )); then
    return 0
  fi
  total=$(( (len + chunk_size - 1) / chunk_size ))
  for ((i=1; i<=total; i++)); do
    start=$(( (i - 1) * chunk_size + 1 ))
    printf -- "----- cx chunk %d/%d -----\n" "$i" "$total"
    printf "%s" "$input" | cut -c"${start}-$((start + chunk_size - 1))"
    if (( i < total )); then
      printf "\n"
    fi
  done
}

_cx_system_capture() {
  local out_var=""
  if [[ "${1:-}" == "--var" ]]; then
    out_var="${2:-}"
    shift 2
  fi

  if [[ $# -lt 1 ]]; then
    echo "_cx_system_capture: missing command" >&2
    return 2
  fi

  local cmd="$1"
  local raw_out processed_out
  local raw_status processed_status
  local rtk_candidate=0
  local rtk_used=0

  case "$cmd" in
    git|diff|ls|tree|grep|test|log|read) rtk_candidate=1 ;;
  esac

  raw_out="$("$@" 2>/dev/null)"
  raw_status=$?
  processed_out="$raw_out"
  processed_status=$raw_status

  if [[ "${CX_RTK_SYSTEM:-0}" == "1" ]] && [[ "$rtk_candidate" -eq 1 ]] && _cx_rtk_usable; then
    processed_out="$(rtk "$@" 2>/dev/null)"
    processed_status=$?
    if [[ "$processed_status" -eq 0 ]]; then
      rtk_used=1
    else
      processed_out="$raw_out"
      processed_status=$raw_status
      rtk_used=0
    fi
  fi

  local clipped_out
  _cx_clip_text --var clipped_out <<< "$processed_out"
  export CX_SYSTEM_OUTPUT_LEN_RAW="$(printf "%s" "$raw_out" | wc -c | tr -d ' ')"
  export CX_SYSTEM_OUTPUT_LINES_RAW="$(printf "%s" "$raw_out" | wc -l | tr -d ' ')"
  export CX_SYSTEM_OUTPUT_LEN_PROCESSED="$(printf "%s" "$processed_out" | wc -c | tr -d ' ')"
  export CX_SYSTEM_OUTPUT_LINES_PROCESSED="$(printf "%s" "$processed_out" | wc -l | tr -d ' ')"
  export CX_SYSTEM_OUTPUT_LEN_CLIPPED="${CX_CLIP_KEPT_CHARS:-$(printf "%s" "$clipped_out" | wc -c | tr -d ' ')}"
  export CX_SYSTEM_OUTPUT_LINES_CLIPPED="${CX_CLIP_KEPT_LINES:-$(printf "%s" "$clipped_out" | wc -l | tr -d ' ')}"
  export CX_SYSTEM_CLIPPED="${CX_CLIP_CLIPPED:-0}"
  export CX_SYSTEM_CLIP_MODE_USED="${CX_CLIP_MODE_USED:-${CX_CONTEXT_CLIP_MODE:-smart}}"
  export CX_SYSTEM_CLIP_FOOTER_USED="${CX_CLIP_FOOTER_USED:-${CX_CONTEXT_CLIP_FOOTER:-1}}"
  export CX_SYSTEM_RTK_USED="$rtk_used"
  export CX_SYSTEM_CAPTURE_SET=1

  if [[ -n "$out_var" ]]; then
    printf -v "$out_var" "%s" "$clipped_out"
  else
    printf "%s" "$clipped_out"
  fi
  return "$processed_status"
}

_cx_codex_json() {
  local tool_name="$1"
  local schema_description="$2"
  local prompt_text="$3"
  local full_prompt raw mode

  mode="$(_cx_mode_normalize)"
  full_prompt="$(
    {
      echo "You are a structured output generator."
      echo "Return STRICT JSON ONLY. No markdown. No prose. No code fences."
      echo "Output MUST be a single valid JSON object matching the schema."
      if [[ "${CX_SCHEMA_STRICT:-0}" == "1" ]]; then
        echo "Schema-strict mode: deterministic JSON only; reject ambiguity."
      elif [[ "$mode" == "deterministic" ]]; then
        echo "Deterministic constraints: stable keys, stable ordering, no optional chatter."
      elif [[ "$mode" == "lean" ]]; then
        echo "Lean constraints: keep values concise."
      else
        echo "Verbose constraints: include fuller detail inside schema fields only."
      fi
      echo "Schema:"
      printf "%s\n" "$schema_description"
      echo
      echo "Task input:"
      printf "%s\n" "$prompt_text"
    }
  )"

  raw="$(printf "%s" "$full_prompt" | _codex_text)"
  export CX_LAST_SCHEMA_TOOL="$tool_name"
  export CX_LAST_SCHEMA_DESC="$schema_description"
  export CX_LAST_SCHEMA_PROMPT="$prompt_text"
  export CX_LAST_SCHEMA_RAW="$raw"
  if [[ -n "${CODEX_MODEL:-}" ]]; then
    _cx_state_set_path "last_model" "$CODEX_MODEL" >/dev/null 2>&1 || true
  fi
  if ! printf "%s" "$raw" | jq . >/dev/null 2>&1; then
    local qid=""
    qid="$(_cx_log_schema_failure "$tool_name" "invalid_json" "$raw" "$schema_description" "$prompt_text" 2>/dev/null || true)"
    echo "${tool_name}: invalid JSON response from Codex" >&2
    if [[ -n "$qid" ]]; then
      echo "${tool_name}: quarantine_id=${qid}" >&2
    fi
    echo "${tool_name}: raw response follows:" >&2
    printf "%s\n" "$raw" >&2
    return 1
  fi

  printf "%s\n" "$raw"
}

_cx_codex_json_schema() {
  local tool_name="$1"
  local schema_description="$2"
  local prompt_text="$3"

  if [[ "${CX_SCHEMA_RELAXED:-0}" == "1" ]]; then
    _cx_codex_json "$tool_name" "$schema_description" "$prompt_text"
    return $?
  fi

  CX_MODE=deterministic CX_SCHEMA_STRICT=1 _cx_codex_json "$tool_name" "$schema_description" "$prompt_text"
}

_cx_json_require_keys() {
  local tool_name="$1"
  local json="$2"
  shift 2
  local k

  for k in "$@"; do
    if ! printf "%s" "$json" | jq -e --arg k "$k" 'has($k)' >/dev/null 2>&1; then
      local qid=""
      qid="$(_cx_log_schema_failure "$tool_name" "missing_key:$k" "$json" "${CX_LAST_SCHEMA_DESC:-}" "${CX_LAST_SCHEMA_PROMPT:-}" 2>/dev/null || true)"
      echo "${tool_name}: missing required key '$k'" >&2
      if [[ -n "$qid" ]]; then
        echo "${tool_name}: quarantine_id=${qid}" >&2
      fi
      echo "${tool_name}: raw response follows:" >&2
      printf "%s\n" "$json" >&2
      return 1
    fi
  done
}

_cx_is_dangerous_cmd() {
  local line="$1"
  local compact
  compact="$(printf "%s" "$line" | tr -s ' ')"

  # Requirement set:
  # - rm -rf
  # - sudo (any)
  # - curl | bash
  # - chmod/chown on system paths
  # - writing to /System, /Library, /usr (except /usr/local)
  if [[ "$compact" =~ (^|[[:space:]])sudo([[:space:]]|$) ]]; then return 0; fi
  if [[ "$compact" == *"rm -rf"* || "$compact" == *"rm -fr"* || "$compact" == *"rm -r -f"* || "$compact" == *"rm -f -r"* ]]; then return 0; fi
  if [[ "$compact" =~ curl[^|]*\|[[:space:]]*(bash|sh|zsh)([[:space:]]|$) ]]; then return 0; fi
  if [[ "$compact" =~ (^|[[:space:]])(chmod|chown)([[:space:]]|$) ]] && [[ "$compact" =~ /(System|Library|usr)(/|$) ]] && [[ "$compact" != *"/usr/local"* ]]; then return 0; fi
  if [[ "$compact" =~ (>|>>)[[:space:]]*/(System|Library|usr)(/|$) ]] && [[ "$compact" != *"/usr/local"* ]]; then return 0; fi
  if [[ "$compact" =~ (^|[[:space:]])tee([[:space:]]|$).*[[:space:]]/(System|Library|usr)(/|$) ]] && [[ "$compact" != *"/usr/local"* ]]; then return 0; fi

  return 1
}

_cx_is_safe_suggested_cmd() {
  _cx_is_dangerous_cmd "$1"
  if [[ $? -eq 0 ]]; then
    return 1
  fi
  return 0
}

cxpolicy() {
  cat <<'EOF'
== cxpolicy ==

Dangerous command classifier: _cx_is_dangerous_cmd
Semantics:
- returns 0: dangerous
- returns 1: safe

Active dangerous patterns:
- sudo <anything>
- rm -rf / rm -fr / rm -r -f / rm -f -r
- curl ... | bash|sh|zsh
- chmod/chown targeting /System, /Library, /usr (except /usr/local)
- write redirection (>, >>) to /System, /Library, /usr (except /usr/local)
- tee writes to /System, /Library, /usr (except /usr/local)

cxfix_run enforcement:
- dangerous commands are blocked by default
- override with CXFIX_FORCE=1 to allow execution

Examples:
- dangerous: sudo rm -rf /tmp/x
- dangerous: curl -fsSL https://example.com/install.sh | bash
- dangerous: echo hi > /usr/bin/tool
- safe: echo hi > /usr/local/bin/tool
- safe: ls -la
EOF
}

cxroles() {
  local role="${1:-}"
  if [[ -z "$role" ]]; then
    cat <<'EOF'
Available roles:
- architect: design, interfaces, risk/tradeoff framing
- implementer: code changes and integration details
- reviewer: bug/risk/regression detection
- tester: validation plans and execution checks
- doc: docs, examples, migration notes

Usage:
- cxroles
- cxroles <architect|implementer|reviewer|tester|doc>
EOF
    return 0
  fi

  if ! _cx_role_header "$role"; then
    echo "Usage: cxroles <architect|implementer|reviewer|tester|doc>" >&2
    return 2
  fi
}

cxprompt() {
  local mode request
  mode="${1:-}"
  request="${2:-}"

  if [[ -z "$mode" || -z "$request" ]]; then
    echo "Usage: cxprompt <implement|fix|test|doc|ops> \"<request>\"" >&2
    return 2
  fi
  case "$mode" in
    implement|fix|test|doc|ops) ;;
    *)
      echo "Usage: cxprompt <implement|fix|test|doc|ops> \"<request>\"" >&2
      return 2
      ;;
  esac

  cat <<EOF
You are working on the "cx" toolchain.
From now on, EVERY new feature must be implemented in TWO places:
1) Repo canonical implementation under ~/cxcodex (sourceable bash entrypoint: cxcodex/cx.sh)
2) Shell bootstrap loader (minimal; should source repo canonical file when present)

Mode:
- ${mode}

Context:
- Repo canonical source of truth: ~/cxcodex/cx.sh and ~/cxcodex/lib/cx/*
- Shell bootstrap: startup profile sources repo canonical entrypoint
- Existing stack: JSONL Codex pipeline, schema-enforced structured commands, repo-aware logs, cxstate/cxpolicy/cxoptimize

Goal:
- ${request}

Requirements:
- Keep behavior deterministic and non-interactive.
- Preserve stdout pipeline compatibility.
- Warnings/errors go to stderr where appropriate.
- Handle null/missing log fields safely.
- Keep implementation compact and maintainable.

Constraints:
- Do not auto-run cxdoctor during sourcing.
- Do not redefine cd or shell navigation behavior.
- Avoid side effects during source beyond function/env setup.
- Prefer minimal diffs and robust fallbacks.

Deliverables:
- Canonical repo code changes under ~/cxcodex (including cx.sh wiring if needed)
- Shell bootstrap updates (minimal, delegate to repo when present)
- Validation outputs for changed commands/paths
- Short manual test checklist

Test Checklist:
$(_cx_prompt_test_checklist "$mode")
EOF
}

cxfanout() {
  local objective="${1:-}"
  local role idx=1 max_tasks
  local diff_out chunks chunk_list chunk_count
  if [[ -z "${objective:-}" ]]; then
    echo "Usage: cxfanout \"<objective>\"" >&2
    return 2
  fi
  max_tasks=6

  _cx_system_capture --var diff_out git diff --no-color >/dev/null 2>&1 || true
  chunks="$(printf "%s" "$diff_out" | _cx_chunk_text)"
  chunk_list="$(printf "%s\n" "$chunks" | awk '
    /^----- cx chunk [0-9]+\/[0-9]+ -----$/ { h=$0; next }
    {
      if (h != "" && !(h in seen) && $0 != "") {
        print h " " substr($0, 1, 180)
        seen[h]=1
      }
    }
  ')"
  chunk_count="$(printf "%s\n" "$chunk_list" | awk 'NF>0{c++} END{print c+0}')"
  if (( chunk_count <= 0 )); then
    chunk_list="----- cx chunk 1/1 -----
(no diff context available)"
    chunk_count=1
  fi
  while IFS= read -r role; do
    local chunk_preview
    [[ -n "${role:-}" ]] || continue
    chunk_preview="$(printf "%s\n" "$chunk_list" | sed -n "${idx}p")"
    if [[ -z "$chunk_preview" ]]; then
      chunk_preview="$(printf "%s\n" "$chunk_list" | sed -n "$(( ((idx - 1) % chunk_count) + 1 ))p")"
    fi
    echo "### Subtask $idx [$role]"
    _cx_role_header "$role"
    cat <<EOF
Objective:
- ${objective}

Bounded context:
- ${chunk_preview}

Task:
- Produce one independent deliverable for this role against the bounded context.
- Keep scope small enough to complete without cross-subtask dependency.

Deliverables:
- Concrete output artifacts for this role.
- Explicit changes with file/function targets.
- A role-specific test checklist with pass/fail criteria.
EOF
    echo
    idx=$((idx + 1))
    if (( idx > max_tasks )); then
      break
    fi
  done <<EOF
architect
implementer
reviewer
tester
doc
implementer
EOF
}

cxreplay() {
  local id="${1:-}"
  local qf tool schema prompt prev_mode prev_chars prev_lines
  local chars lines
  if [[ -z "${id:-}" ]]; then
    echo "Usage: cxreplay <quarantine_id>" >&2
    return 2
  fi
  qf="$(_cx_quarantine_file_by_id "$id" 2>/dev/null || true)"
  if [[ -z "$qf" || ! -f "$qf" ]]; then
    echo "cxreplay: quarantine_id not found: $id" >&2
    return 1
  fi

  tool="$(jq -r '.tool // "cxreplay"' "$qf" 2>/dev/null)"
  schema="$(jq -r '.schema // ""' "$qf" 2>/dev/null)"
  prompt="$(jq -r '.prompt // ""' "$qf" 2>/dev/null)"
  if [[ -z "$schema" || -z "$prompt" ]]; then
    echo "cxreplay: quarantine entry missing schema/prompt payload: $qf" >&2
    return 1
  fi

  prev_mode="${CX_MODE:-lean}"
  prev_chars="${CX_CONTEXT_BUDGET_CHARS:-12000}"
  prev_lines="${CX_CONTEXT_BUDGET_LINES:-300}"
  chars="$prev_chars"
  lines="$prev_lines"
  [[ "$chars" =~ ^[0-9]+$ ]] || chars=12000
  [[ "$lines" =~ ^[0-9]+$ ]] || lines=300
  export CX_MODE="deterministic"
  export CX_CONTEXT_BUDGET_CHARS="$(( chars / 2 ))"
  export CX_CONTEXT_BUDGET_LINES="$(( lines / 2 ))"
  if (( CX_CONTEXT_BUDGET_CHARS < 1000 )); then export CX_CONTEXT_BUDGET_CHARS=1000; fi
  if (( CX_CONTEXT_BUDGET_LINES < 40 )); then export CX_CONTEXT_BUDGET_LINES=40; fi

  _cx_codex_json "${tool}_replay" "$schema" "$prompt"
  local rc=$?

  export CX_MODE="$prev_mode"
  export CX_CONTEXT_BUDGET_CHARS="$prev_chars"
  export CX_CONTEXT_BUDGET_LINES="$prev_lines"
  return $rc
}

cxpromptlint() {
  local n f stats
  n="${1:-200}"
  f="$(_cxlog_init)"

  if [[ ! "$n" =~ ^[0-9]+$ ]] || (( n <= 0 )); then
    echo "Usage: cxpromptlint [positive_run_count]" >&2
    return 2
  fi

  if [[ ! -s "$f" ]]; then
    echo "== cxpromptlint (last $n runs) =="
    echo "- No logs available."
    echo "log_file: $f"
    return 0
  fi

  stats="$(
    tail -n "$n" "$f" | jq -s '
      def nz: . // 0;
      {
        runs: length,
        heavy: (
          map(select(.tool != null))
          | group_by(.tool)
          | map({
              tool: .[0].tool,
              avg_effective_input_tokens: (if length == 0 then 0 else (map(.effective_input_tokens|nz) | add / length | floor) end),
              runs: length
            })
          | sort_by(.avg_effective_input_tokens)
          | reverse
          | .[0:5]
        ),
        poor_cache: (
          map(select(.tool != null))
          | group_by(.tool)
          | map({
              tool: .[0].tool,
              cache_hit_rate: (
                (map(.cached_input_tokens|nz)|add) as $c
                | (map(.input_tokens|nz)|add) as $i
                | if $i == 0 then null else ($c / $i) end
              )
            })
          | map(select(.cache_hit_rate != null and .cache_hit_rate < 0.30))
          | sort_by(.cache_hit_rate)
          | .[0:5]
        ),
        drift: (
          map(select(.tool != null)) as $r
          | ($r | length / 2 | floor) as $half
          | ($r[0:$half]) as $first
          | ($r[$half:]) as $second
          | (
              ($first | map(select(.tool != null)) | group_by(.tool) | map({
                tool: .[0].tool,
                first_avg_eff: (if length == 0 then 0 else (map(.effective_input_tokens|nz)|add/length) end)
              })) as $fa
              | ($second | map(select(.tool != null)) | group_by(.tool) | map({
                tool: .[0].tool,
                second_avg_eff: (if length == 0 then 0 else (map(.effective_input_tokens|nz)|add/length) end)
              })) as $sa
              | [
                  $fa[] as $f
                  | ($sa[] | select(.tool == $f.tool)) as $s
                  | {
                      tool: $f.tool,
                      first_avg_eff: ($f.first_avg_eff|floor),
                      second_avg_eff: ($s.second_avg_eff|floor),
                      ratio: (if $f.first_avg_eff == 0 then null else ($s.second_avg_eff / $f.first_avg_eff) end)
                    }
                  | select(.ratio != null and .ratio > 1.25)
                ]
              | sort_by(.ratio)
              | reverse
              | .[0:5]
            )
        )
      }
    '
  )"

  if [[ -z "$stats" ]]; then
    echo "cxpromptlint: failed to parse logs with jq" >&2
    return 1
  fi

  echo "== cxpromptlint (last $n runs) =="
  echo "- Runs analyzed: $(printf "%s" "$stats" | jq -r '.runs')"
  echo
  echo "Top token-heavy prompt types:"
  local heavy_lines drift_lines cache_lines
  heavy_lines="$(printf "%s" "$stats" | jq -r '.heavy[]? | "- \(.tool): avg_eff=\(.avg_effective_input_tokens), runs=\(.runs)"')"
  cache_lines="$(printf "%s" "$stats" | jq -r '.poor_cache[]? | "- \(.tool): cache_hit=\((.cache_hit_rate*100|round))%"')"
  drift_lines="$(printf "%s" "$stats" | jq -r '.drift[]? | "- \(.tool): first=\(.first_avg_eff), second=\(.second_avg_eff), ratio=\((.ratio*100|round)/100)x"')"
  if [[ -n "$heavy_lines" ]]; then printf "%s\n" "$heavy_lines"; else echo "- n/a"; fi
  echo
  echo "Prompt drift (same tool, increasing effective tokens):"
  if [[ -n "$drift_lines" ]]; then printf "%s\n" "$drift_lines"; else echo "- n/a"; fi
  echo
  echo "Poor cache-hit prompts:"
  if [[ -n "$cache_lines" ]]; then printf "%s\n" "$cache_lines"; else echo "- n/a"; fi
  echo
  echo "Actionable suggestions:"
  echo "- For token-heavy tools: trim prompt context and prefer schema-only responses."
  echo "- For drifted tools: standardize prompt templates and compare prompt_preview shifts."
  echo "- For poor cache-hit tools: reduce prompt variability and keep stable scaffolding text."
  echo "log_file: $f"
}

cx() {
  local out
  _cx_system_capture --var out "$@" || return $?
  printf "%s\n" "$out" | codex exec -
}

cxj() {
  local out
  _cx_system_capture --var out "$@" || return $?
  printf "%s\n" "$out" | _cx_codex_jsonl_with_log "cxj"
}

cxo() {
  local out
  _cx_system_capture --var out "$@" || return $?

  printf "%s\n" "$out" | _cx_codex_jsonl_with_log "cxo" | _cx_extract_agent_message "last"
}

cxol() {
  local out tmp
  _cx_system_capture --var out "$@" || return $?
  tmp="$(mktemp)"
  printf "%s\n" "$out" | codex exec -o "$tmp" - >/dev/null
  cat "$tmp"
  rm -f "$tmp"
}

cxcopy() {
  local txt
  txt="$(cxo "$@")" || return $?
  if [[ -z "$txt" ]]; then
    echo "cxcopy: nothing to copy (empty output)" >&2
    return 1
  fi
  printf "%s" "$txt" | pbcopy
  echo "Copied to clipboard."
}

cxnext() {
  local out
  local schema prompt json
  _cx_system_capture --var out "$@" || return $?

  schema='{
  "commands": ["bash command 1", "bash command 2"]
}'
  prompt="$(
    {
      echo "Based on the terminal output, propose the NEXT commands to run."
      echo "Return 1-6 shell commands in order of execution."
      echo
      echo "TERMINAL OUTPUT:"
      printf "%s\n" "$out"
    }
  )"

  json="$(_cx_codex_json_schema "cxnext" "$schema" "$prompt")" || return 1
  _cx_json_require_keys "cxnext" "$json" "commands" || return 1

  printf "%s" "$json" | jq -r '.commands[]?'
}

cxfix() {
  if [[ $# -lt 1 ]]; then
    echo "Usage: cxfix <command> [args...]" >&2
    return 2
  fi

  local cmd out status
  cmd="$*"
  _cx_system_capture --var out "$@"
  status=$?

  {
    echo "You are my terminal debugging assistant."
    echo "Task:"
    echo "1) Explain what happened (brief)."
    echo "2) If the command failed, diagnose likely cause(s)."
    echo "3) Propose the next 3 commands to run to confirm/fix."
    echo "4) If it's a configuration issue, point to exact file/line patterns to check."
    echo
    echo "Command:"
    echo "$cmd"
    echo
    echo "Exit status: $status"
    echo
    echo "Output:"
    printf "%s\n" "$out"
  } | _codex_text

  return $status
}

cxfix_run() {
  if [[ $# -lt 1 ]]; then
    echo "Usage: cxfix_run <command> [args...]" >&2
    return 2
  fi

  local cmd out status schema prompt json cmds ans
  cmd="$*"
  _cx_system_capture --var out "$@"
  status=$?

  schema='{
  "analysis": "short explanation",
  "commands": ["cmd1", "cmd2"]
}'
  prompt="$(
    {
      echo "You are my terminal debugging assistant."
      echo "Given the command, exit status, and output, provide concise remediation."
      echo
      echo "Command:"
      echo "$cmd"
      echo
      echo "Exit status: $status"
      echo
      echo "Output:"
      printf "%s\n" "$out"
    }
  )"
  json="$(_cx_codex_json_schema "cxfix_run" "$schema" "$prompt")" || return $status
  _cx_json_require_keys "cxfix_run" "$json" "analysis" "commands" || return $status

  cmds="$(echo "$json" | jq -r '.commands[]?')"

  if [[ -z "$cmds" ]]; then
    echo "cxfix_run: No commands suggested."
    echo
    echo "Analysis:"
    echo "$json" | jq -r '.analysis'
    return $status
  fi

  echo
  echo "Analysis:"
  echo "$json" | jq -r '.analysis'
  echo
  echo "Suggested commands:"
  echo "-------------------"
  echo "$cmds"
  echo "-------------------"

  read -r -p "Run these now? [y/N] " ans
  if [[ "$ans" == "y" || "$ans" == "Y" ]]; then
    while IFS= read -r line; do
      [[ -n "${line// }" ]] || continue
      if _cx_is_dangerous_cmd "$line"; then
        if [[ "${CXFIX_FORCE:-0}" != "1" ]]; then
          echo "WARN blocked dangerous command (set CXFIX_FORCE=1 to override): $line" >&2
          continue
        fi
        echo "WARN force-running dangerous command due to CXFIX_FORCE=1: $line" >&2
      fi
      echo "-> $line"
      bash -lc "$line"
    done <<< "$cmds"
  else
    echo "Not running."
  fi

  return $status
}

_cxdiffsum_from_diff() {
  local tool_name="$1"
  local diff_label="$2"
  local diff_out="$3"
  local schema prompt json pr_fmt
  pr_fmt="$(_cx_state_get "preferences.pr_summary_format" 2>/dev/null)"
  if [[ -z "$pr_fmt" ]]; then
    pr_fmt="standard"
  fi

  schema='{
  "title": "short title",
  "summary": ["bullet", "bullet"],
  "risk_edge_cases": ["bullet", "bullet"],
  "suggested_tests": ["bullet", "bullet"]
}'
  prompt="$(
    {
      echo "Write a PR-ready summary of this ${diff_label}."
      echo "Keep bullets concise and actionable."
      echo "$(_cx_mode_render_hint)"
      echo "Preferred PR summary format: $pr_fmt"
      echo
      echo "${diff_label^^}:"
      printf "%s\n" "$diff_out"
    }
  )"
  json="$(_cx_codex_json_schema "$tool_name" "$schema" "$prompt")" || return 1
  _cx_json_require_keys "$tool_name" "$json" "title" "summary" "risk_edge_cases" "suggested_tests" || return 1

  printf "%s" "$json" | jq -r '
    "Title: " + (.title // ""),
    "",
    "Summary:",
    (if (.summary|type)=="array" then .summary[] | "- " + tostring else "- " + (.summary|tostring) end),
    "",
    "Risk/edge cases:",
    (if (.risk_edge_cases|type)=="array" then .risk_edge_cases[] | "- " + tostring else "- " + (.risk_edge_cases|tostring) end),
    "",
    "Suggested tests:",
    (if (.suggested_tests|type)=="array" then .suggested_tests[] | "- " + tostring else "- " + (.suggested_tests|tostring) end)
  '
}

cxdiffsum() {
  local diff_out
  _cx_system_capture --var diff_out git diff --no-color || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxdiffsum: no unstaged changes." >&2
    return 1
  fi

  _cxdiffsum_from_diff "cxdiffsum" "diff" "$diff_out"
}

cxdiffsum_staged() {
  local diff_out
  _cx_system_capture --var diff_out git diff --staged --no-color || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxdiffsum_staged: no staged changes." >&2
    return 1
  fi

  _cxdiffsum_from_diff "cxdiffsum_staged" "staged diff" "$diff_out"
}

cxcommitjson() {
  local diff_out
  local schema prompt json cc_pref style_hint
  _cx_system_capture --var diff_out git diff --staged --no-color || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxcommitjson: no staged changes. Run: git add -p" >&2
    return 1
  fi

  cc_pref="$(_cx_state_get "preferences.conventional_commits" 2>/dev/null)"
  if [[ "$cc_pref" == "true" || "$cc_pref" == "1" ]]; then
    style_hint="Use concise conventional-commit style subject."
  else
    style_hint="Use concise imperative subject (non-conventional format)."
  fi

  schema='{
  "subject": "string <= 72 chars",
  "body": ["bullet string", "..."],
  "breaking": false,
  "scope": "optional string",
  "tests": ["bullet string", "..."]
}'
  prompt="$(
    {
      echo "Generate a commit object from this STAGED diff."
      echo "$style_hint"
      echo
      echo "STAGED DIFF:"
      printf "%s\n" "$diff_out"
    }
  )"
  json="$(_cx_codex_json_schema "cxcommitjson" "$schema" "$prompt")" || return 1
  _cx_json_require_keys "cxcommitjson" "$json" "subject" "body" "breaking" "tests" || return 1
  json="$(printf "%s" "$json" | jq 'if has("scope") then . else . + {scope:null} end')"
  printf "%s\n" "$json"
}

cxcommitmsg() {
  cxcommitjson \
    | jq -r '
      .subject
      + "\n\n"
      + (if (.body|length)>0 then "- " + (.body|join("\n- ")) else "" end)
      + (if (.tests|length)>0 then "\n\nTests:\n- " + (.tests|join("\n- ")) else "" end)
    '
}
