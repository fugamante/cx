#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT/rust/cxrs"

echo "guardrails: cargo fmt --check"
cargo fmt --check

echo "guardrails: cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments"
cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments

echo "guardrails: cargo test --tests -- --test-threads=1"
cargo test --tests -- --test-threads=1

echo "guardrails: PASS"
