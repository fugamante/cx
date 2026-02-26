# Good First Issues

This list is intentionally scoped for first-time contributors. Every item includes a target area and clear acceptance criteria.

## 1) Add log-field regression test for `timeout_frequency`

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: assert `optimize --json` includes `scoreboard.timeout_frequency`.
- Acceptance:
  - test fails before field removal
  - test passes with current implementation

## 2) Add parity check for `policy_blocked` required field

- Area: `rust/cxrs/src/modules/bench_parity_support.rs`
- Goal: extend log invariant checks to include `policy_blocked`.
- Acceptance:
  - parity suite validates `policy_blocked` key presence
  - no behavior regressions in existing parity output

## 3) Add explicit test for `CX_TIMEOUT_GIT_SECS`

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: verify git-class timeout override is respected.
- Acceptance:
  - mock `git` command path
  - timeout event logs `timed_out=true` and expected timeout seconds

## 4) Improve `cx help` examples for task runner

- Area: `rust/cxrs/src/modules/help.rs`
- Goal: add practical examples for `task run` and `task run-all`.
- Acceptance:
  - examples render in `cx help`
  - no command parser changes required

## 5) Add test for native capture fallback when `rtk --version` malformed

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: verify malformed RTK version always falls back to native capture.
- Acceptance:
  - `capture_provider=native`
  - `rtk_used=false`

## 6) Add `--json` output for `cx policy show`

- Area: `rust/cxrs/src/modules/policy.rs`
- Goal: machine-readable policy output for CI checks.
- Acceptance:
  - preserves current human output by default
  - `--json` prints valid JSON and exits 0

## 7) Tighten README command examples to avoid absolute paths

- Area: `README.md`, `rust/cxrs/README.md`
- Goal: keep examples repo-relative and portable.
- Acceptance:
  - no `/Users/...` references in command examples

## 8) Add quarantine fixture helper for tests

- Area: `rust/cxrs/tests/`
- Goal: deduplicate quarantine fixture setup used in replay tests.
- Acceptance:
  - no behavior changes
  - test readability improved

## 9) Add test for `CX_SCHEMA_RELAXED=1` replay behavior

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: document/verify relaxed-mode impact in replay path.
- Acceptance:
  - expected pass/fail semantics captured

## 10) Add command-level changelog lint check

- Area: `.github/workflows/cxrs-compat.yml`
- Goal: ensure command-surface changes include docs/changelog updates.
- Acceptance:
  - CI check fails if command files changed without doc update

