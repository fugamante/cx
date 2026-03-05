#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT/rust/cxrs"

echo "guardrails: cargo fmt --check"
cargo fmt --check

echo "guardrails: cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments"
cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments

echo "guardrails: function name length (max=52)"
python3 ./scripts/check_fn_name_length.py --root . --max-len 52

echo "guardrails: rust symbol naming (fn/struct/enum/trait/type/const)"
python3 ./scripts/check_rust_naming.py --root . --max-fn-len 52 --max-type-len 48 --max-const-len 48

echo "guardrails: test function naming segments (max=7)"
python3 ./scripts/check_fn_name_length.py --root ./tests --max-len 52 --max-segments 7

echo "guardrails: #[test] naming convention (max_len=48, max_segments=7)"
python3 ./scripts/check_test_naming.py --root ./tests --max-len 48 --max-segments 7

echo "guardrails: cargo test --tests -- --test-threads=1"
cargo test --tests -- --test-threads=1

echo "guardrails: PASS"
