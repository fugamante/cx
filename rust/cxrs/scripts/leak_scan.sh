#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

mode="${1:---repo}"

if [[ "$mode" != "--repo" && "$mode" != "--staged" ]]; then
  echo "usage: $0 [--repo|--staged]" >&2
  exit 2
fi

declare -a checks=(
  "local_unix_path|/Users/[A-Za-z0-9_.-]+/"
  "local_home_path|/home/[A-Za-z0-9_.-]+/"
  "windows_user_path|C:\\\\Users\\\\[A-Za-z0-9_.-]+\\\\"
  "email_address|\\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}\\b"
  "github_pat|\\bgh[pousr]_[A-Za-z0-9]{20,}\\b"
  "aws_access_key|\\bAKIA[0-9A-Z]{16}\\b"
  "openai_secret|\\bsk-[A-Za-z0-9]{20,}\\b"
  "slack_token|\\bxox[baprs]-[A-Za-z0-9-]{10,}\\b"
  "private_key|-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----"
)

is_allowed_match() {
  local check_name="$1"
  local matched_line="$2"

  case "$check_name" in
    email_address)
      [[ "$matched_line" =~ users\.noreply\.github\.com|example\.com ]] && return 0
      ;;
  esac

  return 1
}

scan_file_path() {
  local logical_file="$1"
  local physical_path="$2"
  local had_match=0

  for spec in "${checks[@]}"; do
    local check_name="${spec%%|*}"
    local pattern="${spec#*|}"
    local matches
    matches="$(rg -n --pcre2 -e "$pattern" "$physical_path" || true)"
    [[ -z "$matches" ]] && continue

    while IFS= read -r hit; do
      [[ -z "$hit" ]] && continue
      local hit_line="${hit#*:}"
      hit_line="${hit_line#*:}"
      if is_allowed_match "$check_name" "$hit_line"; then
        continue
      fi
      local line_no="${hit#*:}"
      line_no="${line_no%%:*}"
      echo "${logical_file}:${line_no}:${hit_line}" >&2
      had_match=1
    done <<< "$matches"
  done

  return "$had_match"
}

scan_staged() {
  local failed=0
  local staged
  staged="$(git diff --cached --name-only --diff-filter=ACMR)"
  [[ -z "$staged" ]] && return 0

  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    if ! git cat-file -e ":$file" 2>/dev/null; then
      continue
    fi

    local tmp
    tmp="$(mktemp)"
    git show ":$file" > "$tmp" 2>/dev/null || true
    if scan_file_path "$file" "$tmp"; then
      :
    else
      failed=1
    fi
    rm -f "$tmp"
  done <<< "$staged"

  return "$failed"
}

scan_repo() {
  local failed=0
  local files
  files="$(git ls-files)"
  [[ -z "$files" ]] && return 0

  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    if scan_file_path "$file" "$file"; then
      :
    else
      failed=1
    fi
  done <<< "$files"

  return "$failed"
}

if [[ "$mode" == "--staged" ]]; then
  if ! scan_staged; then
    echo "leak-scan: blocked sensitive/identifiable content in staged changes" >&2
    exit 1
  fi
  echo "leak-scan: staged scan PASS" >&2
  exit 0
fi

if ! scan_repo; then
  echo "leak-scan: blocked sensitive/identifiable content in tracked files" >&2
  exit 1
fi
echo "leak-scan: repo scan PASS" >&2
