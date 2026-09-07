#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <repo-root> <base-sha> <head-sha>" >&2
  exit 2
fi

repo_root="$1"
base_sha="$2"
head_sha="$3"

cd "$repo_root"

if ! git cat-file -e "$head_sha^{commit}" 2>/dev/null; then
  echo "Command surface docs gate could not resolve head commit: $head_sha" >&2
  exit 1
fi

if ! git cat-file -e "$base_sha^{commit}" 2>/dev/null; then
  git fetch --no-tags --depth=1 origin "$base_sha" >/dev/null 2>&1 || true
fi

if ! git cat-file -e "$base_sha^{commit}" 2>/dev/null; then
  fallback="$(git rev-parse --verify "$head_sha^" 2>/dev/null || git rev-list --max-parents=0 "$head_sha" | tail -n 1)"
  echo "Command surface docs gate could not resolve base commit $base_sha; falling back to $fallback." >&2
  base_sha="$fallback"
fi

changed="$(git diff --name-only "$base_sha" "$head_sha")"
cmd_changed="$(echo "$changed" | grep -E '^(bin/(cx|xs|xshelf)(-(install|uninstall))?|lib/cx\.sh|cx\.sh|rust/cxrs/src/app/mod\.rs|rust/cxrs/src/modules/(.*cmd.*\.rs|help_data\.rs|native_dispatch\.rs|compat_dispatch\.rs|command_names\.rs|routing\.rs))$' || true)"
contract_changed="$(echo "$changed" | grep -E '^(rust/cxrs/src/modules/(broker\.rs|contract_versions\.rs|contracts_cmd\.rs|policy\.rs|settings_cmds\.rs)|rust/cxrs/tests/fixtures/.*_contract\.json|rust/cxrs/tests/fixtures/eval_lab_bundle\.json)$' || true)"

if [[ -z "$cmd_changed" && -z "$contract_changed" ]]; then
  echo "command surface: unchanged"
  exit 0
fi

required_docs=("CHANGELOG.md")
if [[ -n "$cmd_changed" ]]; then
  required_docs+=("README.md" "docs/project/XSHELF_RENAME_MIGRATION.md")
fi
if [[ -n "$contract_changed" ]]; then
  required_docs+=("docs/providers/CONTRACT_COMPATIBILITY.md")
fi

missing=()
for path in "${required_docs[@]}"; do
  if ! echo "$changed" | grep -qx "$path"; then
    missing+=("$path")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Command or contract surface changed without required compatibility docs updates." >&2
  if [[ -n "$cmd_changed" ]]; then
    echo "Changed command files:" >&2
    echo "$cmd_changed" >&2
  fi
  if [[ -n "$contract_changed" ]]; then
    echo "Changed contract files:" >&2
    echo "$contract_changed" >&2
  fi
  echo "Missing required doc updates:" >&2
  printf '%s\n' "${missing[@]}" >&2
  exit 1
fi

echo "command/contract surface doc gate: PASS"
