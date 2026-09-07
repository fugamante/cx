#!/usr/bin/env bash
set -euo pipefail

repo_root="${1:-}"
snapshot_dir="${2:-}"

[[ -n "$repo_root" && -n "$snapshot_dir" ]] || {
  echo "usage: compat_git_snapshot.sh <repo-root> <empty-snapshot-dir>" >&2
  exit 2
}
[[ -d "$repo_root" ]] || {
  echo "compat-git-snapshot: repository root not found: $repo_root" >&2
  exit 2
}
[[ -d "$snapshot_dir" ]] || {
  echo "compat-git-snapshot: snapshot directory not found: $snapshot_dir" >&2
  exit 2
}
[[ -z "$(find "$snapshot_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]] || {
  echo "compat-git-snapshot: snapshot directory must be empty" >&2
  exit 2
}

git -C "$repo_root" rev-parse --is-inside-work-tree >/dev/null
chmod 700 "$snapshot_dir"
git init --bare --quiet "$snapshot_dir"
git --git-dir="$snapshot_dir" fetch --quiet --no-write-fetch-head \
  "$repo_root" \
  HEAD:refs/heads/compat-snapshot \
  '+refs/tags/*:refs/tags/*'
git --git-dir="$snapshot_dir" symbolic-ref HEAD refs/heads/compat-snapshot
git --git-dir="$snapshot_dir" config core.bare false
git --git-dir="$snapshot_dir" config core.worktree /work
git --git-dir="$snapshot_dir" read-tree HEAD
