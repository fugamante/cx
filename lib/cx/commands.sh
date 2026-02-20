#!/usr/bin/env bash

# cx command helpers built on top of core.sh

if [[ -n "${CX_COMMANDS_LOADED:-}" ]]; then
  return 0
fi
export CX_COMMANDS_LOADED=1

_cx_codex_json() {
  local tool_name="$1"
  local schema_description="$2"
  local prompt_text="$3"
  local full_prompt raw

  full_prompt="$(
    {
      echo "You are a structured output generator."
      echo "Return STRICT JSON ONLY. No markdown. No prose. No code fences."
      echo "Schema:"
      printf "%s\n" "$schema_description"
      echo
      echo "Task input:"
      printf "%s\n" "$prompt_text"
    }
  )"

  raw="$(printf "%s" "$full_prompt" | _codex_text)"
  if [[ -n "${CODEX_MODEL:-}" ]]; then
    _cx_state_set_path "last_model" "$CODEX_MODEL" >/dev/null 2>&1 || true
  fi
  if ! printf "%s" "$raw" | jq . >/dev/null 2>&1; then
    echo "${tool_name}: invalid JSON response from Codex" >&2
    echo "${tool_name}: raw response follows:" >&2
    printf "%s\n" "$raw" >&2
    return 1
  fi

  printf "%s\n" "$raw"
}

_cx_json_require_keys() {
  local tool_name="$1"
  local json="$2"
  shift 2
  local k

  for k in "$@"; do
    if ! printf "%s" "$json" | jq -e --arg k "$k" 'has($k)' >/dev/null 2>&1; then
      echo "${tool_name}: missing required key '$k'" >&2
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

cx() {
  local out
  out="$(rtk "$@")" || return $?
  printf "%s\n" "$out" | codex exec -
}

cxj() {
  local out
  out="$(rtk "$@")" || return $?
  printf "%s\n" "$out" | codex exec --json -
}

cxo() {
  local out
  out="$(rtk "$@")" || return $?

  printf "%s\n" "$out" \
    | codex exec --json - \
    | jq -r 'select(.type=="item.completed" and .item.type=="agent_message") | .item.text' \
    | tail -n 1
}

cxol() {
  local out tmp
  out="$(rtk "$@")" || return $?
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
  out="$(rtk "$@")" || return $?

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

  json="$(_cx_codex_json "cxnext" "$schema" "$prompt")" || return 1
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
  out="$(rtk err "$@" 2>&1)"
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
  out="$("$@" 2>&1)"
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
  json="$(_cx_codex_json "cxfix_run" "$schema" "$prompt")" || return $status
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

cxdiffsum() {
  local diff_out
  local schema prompt json pr_fmt
  diff_out="$(rtk git diff --no-color)" || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxdiffsum: no unstaged changes." >&2
    return 1
  fi

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
      echo "Write a PR-ready summary of this diff."
      echo "Keep bullets concise and actionable."
      echo "Preferred PR summary format: $pr_fmt"
      echo
      echo "DIFF:"
      printf "%s\n" "$diff_out"
    }
  )"
  json="$(_cx_codex_json "cxdiffsum" "$schema" "$prompt")" || return 1
  _cx_json_require_keys "cxdiffsum" "$json" "title" "summary" "risk_edge_cases" "suggested_tests" || return 1

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

cxdiffsum_staged() {
  local diff_out
  diff_out="$(rtk git diff --staged --no-color)" || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxdiffsum_staged: no staged changes." >&2
    return 1
  fi

  {
    echo "Write a PR-ready summary of the STAGED diff."
    echo "Format:"
    echo "- Title: <short>"
    echo "- Summary: 3-6 bullets"
    echo "- Risk/edge cases: 2-5 bullets"
    echo "- Suggested tests: bullets"
    echo
    echo "STAGED DIFF:"
    printf "%s\n" "$diff_out"
  } | _codex_text
}

cxcommitjson() {
  local diff_out
  local schema prompt json cc_pref style_hint
  diff_out="$(rtk git diff --staged --no-color)" || return $?

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
  json="$(_cx_codex_json "cxcommitjson" "$schema" "$prompt")" || return 1
  _cx_json_require_keys "cxcommitjson" "$json" "subject" "body" "breaking" "scope" "tests" || return 1
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
