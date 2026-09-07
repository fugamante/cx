#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT/rust/cxrs"

echo "guardrails: cargo fmt --check"
cargo fmt --check

echo "guardrails: cargo clippy --locked --all-targets -- -D warnings -D clippy::too_many_arguments"
cargo clippy --locked --all-targets -- -D warnings -D clippy::too_many_arguments

echo "guardrails: rust toolchain sync"
python3 "$REPO_ROOT/scripts/check_rust_toolchain_sync.py" --repo-root "$REPO_ROOT"

echo "guardrails: release version sync"
python3 "$REPO_ROOT/scripts/check_version_sync.py" --repo-root "$REPO_ROOT"

echo "guardrails: function name length (max=52)"
python3 ./scripts/check_fn_name_length.py --root . --max-len 52 --max-segments 3 --allowlist ./config/fn_segments_allowlist.txt

echo "guardrails: file naming segments (max=3; grandfathered allowlist)"
python3 ./scripts/check_name_segments.py --root . --max-segments 3 --allowlist ./config/file_segments_allowlist.txt

echo "guardrails: rust symbol naming (fn/struct/enum/trait/type/const)"
python3 ./scripts/check_rust_naming.py --root . --max-fn-len 52 --max-type-len 48 --max-const-len 48

echo "guardrails: test function naming segments (max=7)"
python3 ./scripts/check_fn_name_length.py --root ./tests --max-len 52 --max-segments 7

echo "guardrails: #[test] naming convention (max_len=48, max_segments=7)"
python3 ./scripts/check_test_naming.py --root ./tests --max-len 48 --max-segments 7

if [[ "${CX_GUARDRAILS_SKIP_TESTS:-0}" == "1" ]]; then
  echo "guardrails: cargo test --locked --tests -- --test-threads=1 (skipped by CX_GUARDRAILS_SKIP_TESTS=1)"
else
  echo "guardrails: cargo test --locked --tests -- --test-threads=1"
  cargo test --locked --tests -- --test-threads=1
fi

echo "guardrails: python3 -m unittest tools.test_release_check"
python3 -m unittest tools.test_release_check

echo "guardrails: packaging lifecycle tests"
python3 "$REPO_ROOT/test/package_release_test.py"
python3 "$REPO_ROOT/test/package_signing_test.py"
python3 "$REPO_ROOT/test/reproduce_packages_test.py"

echo "guardrails: python3 tools/release_check.py --repo-root \"$REPO_ROOT\" --max-version-age-days 14"
python3 tools/release_check.py \
  --repo-root "$REPO_ROOT" \
  --max-version-age-days 14 \
  --require-published-status-docs

echo "guardrails: PASS"
