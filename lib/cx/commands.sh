#!/usr/bin/env bash

# cx command helpers built on top of core.sh

if [[ -n "${CX_COMMANDS_LOADED:-}" ]]; then
  return 0
fi
export CX_COMMANDS_LOADED=1

_cx_is_safe_suggested_cmd() {
  local line="$1"
  local compact
  compact="$(printf "%s" "$line" | tr -s ' ')"

  if [[ "$compact" =~ (^|[[:space:]])sudo([[:space:]]|$) ]]; then return 1; fi
  if [[ "$compact" =~ (^|[[:space:]])(reboot|shutdown|halt|poweroff)([[:space:]]|$) ]]; then return 1; fi
  if [[ "$compact" =~ (^|[[:space:]])rm[[:space:]]+-rf([[:space:]]|$) ]]; then return 1; fi
  if [[ "$compact" =~ (^|[[:space:]])(mkfs|fdisk|diskutil|dd)([[:space:]]|$) ]]; then return 1; fi
  if [[ "$compact" == *">/dev/"* || "$compact" == *" >/dev/"* ]]; then return 1; fi
  if [[ "$compact" =~ (^|[[:space:]])(chown|chmod)[[:space:]]+-R[[:space:]]+/ ]]; then return 1; fi

  return 0
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
  out="$(rtk "$@")" || return $?

  {
    echo "Based on the terminal output, propose the NEXT commands to run."
    echo "Return ONLY a bash code block with 1-6 commands. No commentary."
    echo
    echo "TERMINAL OUTPUT:"
    printf "%s\n" "$out"
  } | _codex_text \
    | awk '
      BEGIN{in=0}
      /^```bash[[:space:]]*$/{in=1; next}
      /^```[[:space:]]*$/{if(in){exit};}
      {if(in) print}
    '
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

  local cmd out status json cmds ans
  cmd="$*"
  out="$("$@" 2>&1)"
  status=$?

  json="$({
    echo "You are my terminal debugging assistant."
    echo "Given the command, exit status, and output,"
    echo "return STRICT JSON ONLY with this exact schema:"
    echo '{'
    echo '  "analysis": "short explanation",'
    echo '  "commands": ["cmd1", "cmd2"]'
    echo '}'
    echo
    echo "No markdown. No commentary. JSON only."
    echo
    echo "Command:"
    echo "$cmd"
    echo
    echo "Exit status: $status"
    echo
    echo "Output:"
    printf "%s\n" "$out"
  } | _codex_text)"

  if ! echo "$json" | jq . >/dev/null 2>&1; then
    echo "cxfix_run: Codex did not return valid JSON."
    echo "Raw response:"
    echo "$json"
    return $status
  fi

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
      if ! _cx_is_safe_suggested_cmd "$line"; then
        echo "WARN skipped potentially unsafe command: $line" >&2
        continue
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
  diff_out="$(rtk git diff --no-color)" || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxdiffsum: no unstaged changes." >&2
    return 1
  fi

  {
    echo "Write a PR-ready summary of this diff."
    echo "Format:"
    echo "- Title: <short>"
    echo "- Summary: 3-6 bullets"
    echo "- Risk/edge cases: 2-5 bullets"
    echo "- Suggested tests: bullets"
    echo
    echo "DIFF:"
    printf "%s\n" "$diff_out"
  } | _codex_text
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
  diff_out="$(rtk git diff --staged --no-color)" || return $?

  if [[ -z "$diff_out" ]]; then
    echo "cxcommitjson: no staged changes. Run: git add -p" >&2
    return 1
  fi

  {
    echo "Generate a JSON object for a git commit based on the STAGED diff."
    echo "Return JSON ONLY (no markdown). Schema:"
    echo '{'
    echo '  "subject": "string <= 72 chars",'
    echo '  "body": ["bullet string", "..."],'
    echo '  "breaking": false,'
    echo '  "scope": "optional string",'
    echo '  "tests": ["bullet string", "..."]'
    echo '}'
    echo
    echo "STAGED DIFF:"
    printf "%s\n" "$diff_out"
  } | _codex_text
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
