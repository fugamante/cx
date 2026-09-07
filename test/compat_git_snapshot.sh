#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
trap 'rm -rf -- "$fixture_root"' EXIT

source_repo="$fixture_root/source"
linked_repo="$fixture_root/linked"
standalone_snapshot="$fixture_root/standalone-snapshot"
linked_snapshot="$fixture_root/linked-snapshot"

git init --quiet "$source_repo"
printf 'fixture\n' >"$source_repo/fixture.txt"
git -C "$source_repo" add fixture.txt
git -C "$source_repo" \
  -c user.name="XSHELF Test" \
  -c user.email="test.invalid" \
  commit --quiet -m fixture
git -C "$source_repo" tag v2026.01.01
git -C "$source_repo" worktree add --quiet -b linked-fixture "$linked_repo"

mkdir "$standalone_snapshot" "$linked_snapshot"
"$repo_root/scripts/compat_git_snapshot.sh" "$source_repo" "$standalone_snapshot"
"$repo_root/scripts/compat_git_snapshot.sh" "$linked_repo" "$linked_snapshot"

for snapshot in "$standalone_snapshot" "$linked_snapshot"; do
  test "$(git --git-dir="$snapshot" rev-parse HEAD)" = "$(git -C "$source_repo" rev-parse HEAD)"
  git --git-dir="$snapshot" tag --merged HEAD --list v2026.01.01 | grep -qx v2026.01.01
  test "$(git --git-dir="$snapshot" for-each-ref --format='%(refname)' refs/heads)" = \
    refs/heads/compat-snapshot
  test ! -e "$snapshot/worktrees"
  test "$(git --git-dir="$snapshot" config core.worktree)" = /work
  test -f "$snapshot/index"
  test -z "$(git --git-dir="$snapshot" --work-tree="$source_repo" status --porcelain)"
  if grep -F "$fixture_root" "$snapshot/config" "$snapshot/HEAD" >/dev/null; then
    echo "compat git snapshot leaked a host worktree path" >&2
    exit 1
  fi
done

echo "PASS: standalone and linked-worktree Git snapshots"
